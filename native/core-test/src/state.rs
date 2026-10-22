// 任务模型与应用状态
//
// 状态机：Wait → Running → Done / Error
// 所有 UI 更新必须发生在主线程；后台线程只通过 channel 回传结果。

use std::path::{Path, PathBuf};

/// 单个转换任务的状态
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// 等待转换
    Wait,
    /// 正在转换
    Running,
    /// 转换成功，括号内是耗时毫秒
    Done(u64),
    /// 转换失败，括号内是给用户看的原因
    Error(String),
}

impl Status {
    /// 是否已经结束（成功或失败）
    pub fn is_settled(&self) -> bool {
        matches!(self, Status::Done(_) | Status::Error(_))
    }
}

/// 一个待转换的文档
#[derive(Debug, Clone)]
pub struct Task {
    pub id: usize,
    /// 文件名（含扩展名）
    pub name: String,
    /// 完整路径
    pub path: PathBuf,
    /// 相对目录（选择文件夹时保留层级），如 "2026/Q1/"
    pub rel: String,
    /// 小写扩展名，不含点
    pub ext: String,
    /// 源文件大小（字节）
    pub size: u64,
    pub status: Status,
    /// 转换后 Markdown 大小
    pub out_size: u64,
}

impl Task {
    pub fn new(id: usize, path: PathBuf, rel: String) -> Self {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

        Self {
            id,
            name,
            path,
            rel,
            ext,
            size,
            status: Status::Wait,
            out_size: 0,
        }
    }

    /// 输出文件路径
    ///
    /// out_dir 是「输出根目录」。当用户没有指定输出目录时，
    /// 根目录就是源文件所在目录，此时不能再叠加 rel（否则会重复层级）。
    pub fn output_path(&self, out_root: &Path) -> PathBuf {
        let stem = self
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        let rel = self.rel.trim_end_matches('/');

        // 根目录 == 源文件所在目录 → 说明是「同目录」模式，不叠加 rel
        let same_dir = self
            .path
            .parent()
            .map(|p| p == out_root)
            .unwrap_or(false);

        if rel.is_empty() || same_dir {
            out_root.join(format!("{}.md", stem))
        } else {
            out_root.join(rel).join(format!("{}.md", stem))
        }
    }
}

/// 一次转换的统计
#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub running: usize,
}

impl Stats {
    /// 已处理（成功+失败）
    pub fn settled(&self) -> usize {
        self.done + self.failed
    }
    /// 百分比 0-100
    pub fn percent(&self) -> u32 {
        if self.total == 0 {
            0
        } else {
            (self.settled() * 100 / self.total) as u32
        }
    }
}

/// 后台线程回传的消息
#[derive(Debug)]
pub enum WorkerMsg {
    /// 某个任务完成
    Finished {
        id: usize,
        result: Result<(u64, String), String>,
    },
    /// 全部结束
    AllDone,
}

/// 应用全局状态
pub struct AppState {
    pub tasks: Vec<Task>,
    pub next_id: usize,
    /// 输出目录。None 表示"与源文件同目录"（按任务分别决定）
    pub out_dir: Option<PathBuf>,
    /// 正在转换
    pub running: bool,
    /// 本轮统计
    pub stats: Stats,
    /// 状态栏文字
    pub status: String,
    /// 输出目录已由用户确认
    pub out_confirmed: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            next_id: 1,
            out_dir: None,
            running: false,
            stats: Stats::default(),
            status: "就绪 · 拖入文档开始".to_string(),
            out_confirmed: false,
        }
    }

    /// 加入文件，返回是否被接受
    pub fn add_file(&mut self, path: PathBuf, rel: String) -> bool {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !is_supported(&ext) {
            return false;
        }
        // 同一个文件不重复加入
        if self.tasks.iter().any(|t| t.path == path) {
            return false;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.tasks.push(Task::new(id, path, rel));
        true
    }

    /// 重新统计（从任务列表推导，避免状态不同步）
    pub fn recount(&mut self) {
        let mut s = Stats {
            total: self.tasks.len(),
            ..Default::default()
        };
        for t in &self.tasks {
            match t.status {
                Status::Done(_) => s.done += 1,
                Status::Error(_) => s.failed += 1,
                Status::Running => s.running += 1,
                Status::Wait => {}
            }
        }
        self.stats = s;
    }

    /// 移除已结束的任务（清空时用）
    pub fn clear(&mut self) {
        self.tasks.clear();
        self.stats = Stats::default();
        self.running = false;
        self.status = "就绪 · 拖入文档开始".to_string();
    }

    /// 找到任务的输出目录：用户指定了就用它，否则用源文件所在目录
    pub fn output_dir_for(&self, task: &Task) -> PathBuf {
        match &self.out_dir {
            Some(d) => d.clone(),
            None => task
                .path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".")),
        }
    }
}

/// 支持的扩展名
pub fn is_supported(ext: &str) -> bool {
    matches!(
        ext,
        "doc" | "docx" | "docm"
            | "ppt" | "pptx" | "pptm"
            | "xls" | "xlsx" | "xlsm" | "xlsb"
            | "odt" | "ods" | "odp"
            | "rtf" | "epub" | "csv" | "pdf"
    )
}

/// 人类可读的文件大小
pub fn fmt_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
