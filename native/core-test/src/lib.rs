pub mod frontmatter;
pub mod state;

// 核心逻辑测试 —— 纯 Rust，不依赖 Win32，可在 Linux 上跑
//
// 覆盖：格式判断、状态机、输出路径计算、官方内核行为

use crate::state::{fmt_size, is_supported, AppState, Status, Task};
use std::path::{Path, PathBuf};

#[test]
fn 支持的格式判断() {
    for ext in [
        "docx", "doc", "docm", "pptx", "ppt", "xlsx", "xls", "xlsm", "xlsb", "pdf", "epub", "odt",
        "ods", "odp", "rtf", "csv",
    ] {
        assert!(is_supported(ext), "应该支持 {ext}");
    }
    for ext in ["txt", "md", "exe", "zip", "jpg", ""] {
        assert!(!is_supported(ext), "不该支持 {ext}");
    }
}

#[test]
fn 大小格式化() {
    assert_eq!(fmt_size(512), "512 B");
    assert_eq!(fmt_size(2048), "2.0 KB");
    assert_eq!(fmt_size(3 * 1024 * 1024), "3.0 MB");
}

#[test]
fn 状态机流转() {
    let mut st = AppState::new();
    assert_eq!(st.stats.total, 0);
    assert_eq!(st.stats.percent(), 0);

    st.add_file(PathBuf::from("/tmp/a.docx"), String::new());
    st.add_file(PathBuf::from("/tmp/b.xlsx"), String::new());
    st.add_file(PathBuf::from("/tmp/c.pdf"), String::new());
    st.recount();
    assert_eq!(st.stats.total, 3);
    assert_eq!(st.stats.percent(), 0);

    // 重复添加被拒绝
    assert!(!st.add_file(PathBuf::from("/tmp/a.docx"), String::new()));
    assert_eq!(st.tasks.len(), 3);

    // 不支持的格式被拒绝
    assert!(!st.add_file(PathBuf::from("/tmp/x.txt"), String::new()));
    assert_eq!(st.tasks.len(), 3);

    st.tasks[0].status = Status::Done(12);
    st.tasks[1].status = Status::Error("需 OCR".into());
    st.tasks[2].status = Status::Running;
    st.recount();
    assert_eq!(st.stats.done, 1);
    assert_eq!(st.stats.failed, 1);
    assert_eq!(st.stats.running, 1);
    assert_eq!(st.stats.settled(), 2);
    assert_eq!(st.stats.percent(), 66);
}

#[test]
fn 输出路径计算() {
    let t = Task::new(1, PathBuf::from("/docs/2026/report.docx"), "2026/".into());

    let same = t.output_path(Path::new("/docs/2026"));
    assert_eq!(same, PathBuf::from("/docs/2026/report.md"));

    let custom = t.output_path(Path::new("/out"));
    assert_eq!(custom, PathBuf::from("/out/2026/report.md"));

    let t2 = Task::new(2, PathBuf::from("/docs/a.docx"), String::new());
    assert_eq!(t2.output_path(Path::new("/out")), PathBuf::from("/out/a.md"));
}

#[test]
fn 输出目录默认值() {
    let mut st = AppState::new();
    st.add_file(PathBuf::from("/docs/sub/a.docx"), String::new());
    let t = &st.tasks[0];

    // 默认 = 源文件同目录
    assert_eq!(st.output_dir_for(t), PathBuf::from("/docs/sub"));

    // 用户指定后 = 指定目录
    st.out_dir = Some(PathBuf::from("/out"));
    assert_eq!(st.output_dir_for(t), PathBuf::from("/out"));
}

#[test]
fn 清空重置状态() {
    let mut st = AppState::new();
    st.add_file(PathBuf::from("/tmp/a.docx"), String::new());
    st.recount();
    assert_eq!(st.stats.total, 1);
    st.clear();
    assert_eq!(st.tasks.len(), 0);
    assert_eq!(st.stats.total, 0);
}

#[test]
fn 官方内核能转换xlsx() {
    let p = "/workspace/anydoc-gui/examples/test.xlsx";
    if !Path::new(p).exists() {
        eprintln!("跳过：样本不存在");
        return;
    }
    let md = anydoc::to_markdown(p).expect("应该转换成功");
    assert!(md.contains("姓名"), "输出应含表头");
    assert!(md.contains("张三"), "输出应含数据");
    assert!(md.contains("| --- |"), "应是 markdown 表格");
}

#[test]
fn 官方内核按字节签名识别() {
    use anydoc::Format;
    let p = "/workspace/anydoc-gui/examples/test.docx";
    if !Path::new(p).exists() {
        eprintln!("跳过");
        return;
    }
    let bytes = std::fs::read(p).unwrap();
    assert_eq!(
        Format::from_bytes(&bytes),
        Some(Format::Docx),
        "应通过字节签名识别为 docx（不靠扩展名）"
    );
}

#[test]
fn 改后缀照样识别() {
    use anydoc::Format;
    let src = "/workspace/anydoc-gui/examples/test.xlsx";
    if !Path::new(src).exists() {
        eprintln!("跳过");
        return;
    }
    let bytes = std::fs::read(src).unwrap();
    // 内容是 xlsx，无论叫什么
    assert_eq!(Format::from_bytes(&bytes), Some(Format::Excel));
    // 转成 markdown 成功
    let md = anydoc::to_markdown_bytes(&bytes, Format::Excel).unwrap();
    assert!(md.contains("张三"));
}

#[test]
fn 损坏文件报错而非产出垃圾() {
    let junk = std::env::temp_dir().join("anydoc-core-test-junk.docx");
    std::fs::write(&junk, b"this is not a zip archive at all").unwrap();
    let r = anydoc::to_markdown(&junk);
    assert!(r.is_err(), "垃圾文件应报错而不是返回半成品");
    let _ = std::fs::remove_file(&junk);
}

#[test]
fn 不存在的文件报错() {
    assert!(anydoc::to_markdown("/tmp/definitely-not-exist-xyz.docx").is_err());
}

// ── YAML Frontmatter 测试 ──

#[test]
fn frontmatter_格式正确() {
    use std::path::PathBuf;
    let t = Task::new(1, PathBuf::from("/docs/年度报告.docx"), String::new());
    let fm = crate::frontmatter::build(&t);
    assert!(fm.starts_with("---\n"));
    assert!(fm.ends_with("---"));
    assert!(fm.contains("title: "));
    assert!(fm.contains("type: docx"));
    assert!(fm.contains("size: "));
    assert!(fm.contains("converted_at: "));
}

#[test]
fn frontmatter_注入到正文前() {
    use std::path::PathBuf;
    let t = Task::new(1, PathBuf::from("/docs/a.docx"), String::new());
    let out = crate::frontmatter::inject(&t, "# 标题\n\n正文");
    assert!(out.starts_with("---\n"));
    assert!(out.contains("\n\n# 标题"));
    assert!(out.trim_end().ends_with("正文"));
}

#[test]
fn frontmatter_转义特殊字符() {
    assert_eq!(crate::frontmatter::yaml_str("普通文本"), "普通文本");
    assert_eq!(crate::frontmatter::yaml_str("a: b"), "\"a: b\"");
    assert_eq!(crate::frontmatter::yaml_str("has#hash"), "\"has#hash\"");
    assert_eq!(crate::frontmatter::yaml_str("say \"hi\""), "\"say \\\"hi\\\"\"");
    assert_eq!(crate::frontmatter::yaml_str(""), "\"\"");
}
