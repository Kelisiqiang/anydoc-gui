// 后台转换线程池
//
// 设计要点：
//   - 转换是 CPU 密集型，放后台线程，UI 线程绝不阻塞
//   - 结果通过 mpsc channel 回传，主线程用 SetTimer 轮询收取
//     （Windows 控件非线程安全，所有 UI 操作必须回主线程）
//   - 并发数 = CPU 核数 - 1，上限 4
//
// 进度：
//   官方 anydoc 的 to_markdown 是一次性同步调用，没有进度回调。
//   所以这里自己接管 IO，把一次转换拆成三个可观测阶段：
//     ① 读取文件  0%→30%   真实字节进度（分块读）
//     ② 解析转换  30%→95%  不可知，用不确定态动画 + 阶段文字
//     ③ 写入文件  95%→100% 真实字节进度（分块写）
//   每个任务独立一条进度条，互不影响。

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;

use crate::state::{Task, WorkerMsg};

/// 阶段划分（百分比）
const READ_END: u8 = 30;
const PARSE_END: u8 = 95;

/// 读取阶段的分块大小
const CHUNK: usize = 256 * 1024;

/// 任务被取消
const CANCELLED: &str = "已取消";

/// 分阶段读取文件，回传真实字节进度
fn read_with_progress(
    path: &PathBuf,
    total_hint: u64,
    tx: &std::sync::mpsc::Sender<WorkerMsg>,
    task: &Task,
) -> Result<Vec<u8>, String> {
    let id = task.id;
    let mut file = std::fs::File::open(path).map_err(|e| format!("读取失败: {}", e))?;
    let mut buf = Vec::with_capacity(total_hint as usize);
    let mut chunk = vec![0u8; CHUNK];
    let mut read_total: u64 = 0;

    loop {
        let n = file
            .read(&mut chunk)
            .map_err(|e| format!("读取失败: {}", e))?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        read_total += n as u64;

        if task.is_cancelled() {
            return Err(CANCELLED.to_string());
        }

        // 真实字节进度：映射到 0%→30%
        let pct = if total_hint > 0 {
            ((read_total as f64 / total_hint as f64) * READ_END as f64) as u8
        } else {
            0
        };
        let _ = tx.send(WorkerMsg::Progress {
            id,
            percent: pct.min(READ_END),
            phase: "读取文件".to_string(),
        });
    }

    let _ = tx.send(WorkerMsg::Progress {
        id,
        percent: READ_END,
        phase: "解析转换".to_string(),
    });
    Ok(buf)
}

/// 分阶段写入文件，回传真实字节进度
fn write_with_progress(
    path: &PathBuf,
    data: &[u8],
    tx: &std::sync::mpsc::Sender<WorkerMsg>,
    task: &Task,
) -> Result<(), String> {
    let id = task.id;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    let mut file = std::fs::File::create(path).map_err(|e| format!("写入失败: {}", e))?;
    let total = data.len() as u64;
    let mut written: u64 = 0;

    for chunk in data.chunks(CHUNK) {
        file.write_all(chunk).map_err(|e| format!("写入失败: {}", e))?;
        written += chunk.len() as u64;

        if task.is_cancelled() {
            // 取消：删掉半成品，不留垃圾文件
            let _ = std::fs::remove_file(path);
            return Err(CANCELLED.to_string());
        }
        let pct = if total > 0 {
            (PARSE_END as u64
                + ((written as f64 / total as f64) * (100 - PARSE_END) as f64) as u64)
                as u8
        } else {
            100
        };
        let _ = tx.send(WorkerMsg::Progress {
            id,
            percent: pct.min(100),
            phase: "写入文件".to_string(),
        });
    }

    let _ = file.flush();
    Ok(())
}

/// 把内核错误码翻译成用户能懂的话
fn map_error(e: anydoc::ConvertError) -> String {
    match e {
        anydoc::ConvertError::NeedsOcr { .. } => "需 OCR（本地不含 OCR）".to_string(),
        anydoc::ConvertError::Encrypted => "文件已加密".to_string(),
        anydoc::ConvertError::Malformed { .. } => "文件结构损坏".to_string(),
        anydoc::ConvertError::ResourceLimit { .. } => "超出安全限制".to_string(),
        anydoc::ConvertError::MissingPart { .. } => "缺少必要内容".to_string(),
        anydoc::ConvertError::Unsupported(_) => "不支持的格式".to_string(),
        anydoc::ConvertError::Io(_) => "文件读取失败".to_string(),
        other => format!("转换失败: {}", other),
    }
}

#[cfg(feature = "ocr")]
/// 问用户：这份扫描件要不要走云端 OCR
///
/// 官方 anydoc 本地不做 OCR（遇到扫描页直接报 needsOcr），
/// 官方给的解法是 `--ocr hosted` —— 把文档交给 Firecrawl Parse 云端处理。
/// 这里保持同样的语义：先问，用户同意才上传。
fn ask_cloud_ocr(
    task: &Task,
    tx: &std::sync::mpsc::Sender<WorkerMsg>,
    allow_all: &std::sync::atomic::AtomicBool,
) -> bool {
    use std::sync::atomic::Ordering;
    if allow_all.load(Ordering::Relaxed) {
        return true;
    }
    let _ = tx.send(WorkerMsg::NeedsOcrPrompt {
        id: task.id,
        name: task.name.clone(),
        size: task.size,
    });
    let (lock, cv) = &*task.ocr_decision;
    let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    let (g, _) = cv
        .wait_timeout(guard, std::time::Duration::from_secs(300))
        .unwrap_or_else(|e| e.into_inner());
    *g
}

/// 调用官方内核转换
///
/// 返回 (Markdown, 是否走云端 OCR)
fn parse_bytes(
    task: &Task,
    bytes: &[u8],
    tx: &std::sync::mpsc::Sender<WorkerMsg>,
    allow_all: &std::sync::atomic::AtomicBool,
) -> Result<(String, bool), String> {
    // 字节签名识别格式，改过扩展名的文件也能正确解析
    let format = anydoc::Format::from_bytes(bytes)
        .or_else(|| anydoc::Format::from_extension(&task.ext));

    let md = match anydoc::to_markdown_bytes(bytes, format) {
        Ok(m) => (m, false),
        Err(anydoc::ConvertError::NeedsOcr { .. }) => {
            // 本地内核没有 OCR 能力（官方 anydoc 也是如此）
            #[cfg(feature = "ocr")]
            {
                // 编译时开了云端 OCR：问用户后交给 Firecrawl Parse
                if task.ext != "pdf" {
                    return Err("需 OCR（该格式不支持云端 OCR）".to_string());
                }
                if !ask_cloud_ocr(task, tx, allow_all) {
                    return Err("需 OCR（已跳过）".to_string());
                }
                if task.is_cancelled() {
                    return Err(CANCELLED.to_string());
                }
                let _ = tx.send(WorkerMsg::Progress {
                    id: task.id,
                    percent: 40,
                    phase: "云端 OCR".to_string(),
                });
                let md = match crate::ocr::hosted_markdown(bytes, &task.name) {
                    Ok(m) => m,
                    Err(e) => {
                        crate::ui::diag(&format!("云端 OCR 失败: {e}"));
                        return Err(e);
                    }
                };
                (md, true)
            }
            #[cfg(not(feature = "ocr"))]
            {
                let _ = (tx, allow_all);
                return Err("需 OCR（本地不含 OCR）".to_string());
            }
        }
        Err(e) => return Err(map_error(e)),
    };
    Ok(md)
}

/// 启动转换。
///
/// 返回接收端，主线程应周期性 try_recv 取结果。
pub fn spawn(
    tasks: Vec<Task>,
    out_root: Option<PathBuf>,
    inject_frontmatter: bool,
    // 用户是否同意「本次所有扫描件都走云端 OCR」
    allow_all: Arc<std::sync::atomic::AtomicBool>,
) -> Receiver<WorkerMsg> {
    let out_root: Arc<Option<PathBuf>> = Arc::new(out_root);
    let (tx, rx) = channel::<WorkerMsg>();

    let threads = thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1))
        .unwrap_or(2)
        .clamp(2, 4);

    // 简单工作窃取：所有线程共享一个任务队列
    let queue = Arc::new(std::sync::Mutex::new(tasks.into_iter().collect::<Vec<_>>()));
    let done_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let total = queue.lock().map(|q| q.len()).unwrap_or(0);

    for _ in 0..threads {
        let queue = queue.clone();
        let tx = tx.clone();
        let done_count = done_count.clone();
        let out_root = out_root.clone();
        let inject_frontmatter = inject_frontmatter;
        let allow_all = allow_all.clone();

        thread::spawn(move || {
            loop {
                // 取一个任务
                let task = {
                    let mut q = match queue.lock() {
                        Ok(q) => q,
                        Err(_) => break,
                    };
                    if q.is_empty() {
                        None
                    } else {
                        Some(q.remove(0))
                    }
                };

                let task = match task {
                    Some(t) => t,
                    None => break, // 队列空了，这个线程退出
                };

                // 决定输出根目录（不预先拼 rel，交给 output_path 处理）
                let out_root: PathBuf = match &*out_root {
                    Some(root) => root.clone(),
                    None => task
                        .path
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| PathBuf::from(".")),
                };

                // 通知主线程：这个任务开始处理了
                let _ = tx.send(WorkerMsg::Started { id: task.id });

                let t_start = std::time::Instant::now();
                let result = process(&task, &out_root, inject_frontmatter, &tx, &allow_all);

                let elapsed_ms = t_start.elapsed().as_millis() as u64;
                if tx
                    .send(WorkerMsg::Finished {
                        id: task.id,
                        elapsed_ms,
                        result,
                    })
                    .is_err()
                {
                    break; // 主线程已关闭
                }

                let n = done_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                if n >= total && tx.send(WorkerMsg::AllDone).is_err() {
                    break;
                }
            }
        });
    }

    rx
}

/// 单个任务的完整流程（带进度回传）
fn process(
    task: &Task,
    out_root: &PathBuf,
    inject_frontmatter: bool,
    tx: &std::sync::mpsc::Sender<WorkerMsg>,
    allow_all: &std::sync::atomic::AtomicBool,
) -> Result<(u64, String, bool), String> {
    // ① 读取（真实字节进度）
    let bytes = read_with_progress(&task.path, task.size, tx, task)?;

    if task.is_cancelled() {
        return Err(CANCELLED.to_string());
    }

    // ② 解析（官方内核，一次性；进度停在 30%→95%）
    let (md, via_cloud) = parse_bytes(task, &bytes, tx, allow_all)?;

    if task.is_cancelled() {
        return Err(CANCELLED.to_string());
    }

    // 可选：注入 YAML Frontmatter
    let md = if inject_frontmatter {
        crate::frontmatter::inject(task, &md)
    } else {
        md
    };
    let size = md.len() as u64;

    // ③ 写入（真实字节进度）
    let out_path = task.output_path(out_root);
    write_with_progress(&out_path, md.as_bytes(), tx, task)?;

    Ok((size, md, via_cloud))
}
