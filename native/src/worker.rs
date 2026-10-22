// 后台转换线程池
//
// 设计要点：
//   - 转换是 CPU 密集型，放后台线程，UI 线程绝不阻塞
//   - 结果通过 mpsc channel 回传，主线程用 SetTimer 轮询收取
//     （Windows 控件非线程安全，所有 UI 操作必须回主线程）
//   - 并发数 = CPU 核数 - 1，上限 4

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;

use crate::state::{Task, WorkerMsg};

/// 转换一个文件：返回 (md 字节数, markdown 内容) 或错误原因
fn convert_one(path: &PathBuf) -> Result<(u64, String), String> {
    // 官方内核：字节签名识别格式，改过扩展名的文件也能正确解析
    let md = anydoc::to_markdown(path).map_err(|e| {
        // 把内部错误码翻译成用户能懂的话
        match e {
            anydoc::ConvertError::NeedsOcr { .. } => "需 OCR（anydoc 不含 OCR）".to_string(),
            anydoc::ConvertError::Encrypted => "文件已加密".to_string(),
            anydoc::ConvertError::Malformed { .. } => "文件结构损坏".to_string(),
            anydoc::ConvertError::ResourceLimit { .. } => "超出安全限制".to_string(),
            anydoc::ConvertError::MissingPart { .. } => "缺少必要内容".to_string(),
            anydoc::ConvertError::Unsupported(_) => "不支持的格式".to_string(),
            anydoc::ConvertError::Io(_) => "文件读取失败".to_string(),
            other => format!("转换失败: {}", other),
        }
    })?;
    Ok((md.len() as u64, md))
}

/// 启动转换。
///
/// 返回接收端，主线程应周期性 try_recv 取结果。
pub fn spawn(tasks: Vec<Task>, out_root: Option<PathBuf>) -> Receiver<WorkerMsg> {
    let out_root: Arc<Option<PathBuf>> = Arc::new(out_root);
    let (tx, rx) = channel::<WorkerMsg>();

    let threads = thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1))
        .unwrap_or(2)
        .clamp(2, 4);

    // 简单工作窃取：所有线程共享一个任务队列
    let queue = std::sync::Arc::new(std::sync::Mutex::new(
        tasks.into_iter().collect::<Vec<_>>(),
    ));
    let done_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let total = queue.lock().map(|q| q.len()).unwrap_or(0);

    for _ in 0..threads {
        let queue = queue.clone();
        let tx = tx.clone();
        let done_count = done_count.clone();
        let out_root = out_root.clone();

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

                // 转换 + 写盘
                let result = match convert_one(&task.path) {
                    Ok((size, md)) => {
                        // 输出路径（含 rel 与同目录判定）
                        let out_path = task.output_path(&out_root);
                        // 父目录可能不存在（选目录时的子目录结构）
                        if let Some(parent) = out_path.parent() {
                            if let Err(e) = std::fs::create_dir_all(parent) {
                                let _ = tx.send(WorkerMsg::Finished {
                                    id: task.id,
                                    result: Err(format!("创建目录失败: {}", e)),
                                });
                                continue;
                            }
                        }
                        match std::fs::write(&out_path, md.as_bytes()) {
                            Ok(_) => Ok((size, md)),
                            Err(e) => Err(format!("写入失败: {}", e)),
                        }
                    }
                    Err(e) => Err(e),
                };

                if tx.send(WorkerMsg::Finished { id: task.id, result }).is_err() {
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
