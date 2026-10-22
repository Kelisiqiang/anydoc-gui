// 核心逻辑测试（不依赖 GUI，验证状态机 / 路径计算 / 转换内核）
//
// 这个测试用 #[test] 跑，但编译到 Linux 上执行 —— 所以只测不依赖 Win32 的部分。

#[cfg(test)]
mod tests {
    use crate::state::{fmt_size, is_supported, AppState, Status, Task};
    use std::path::PathBuf;

    #[test]
    fn 支持的格式判断() {
        for ext in ["docx", "pptx", "xlsx", "pdf", "epub", "odt", "rtf", "csv", "doc", "xlsb"] {
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

        // 加 3 个文件
        st.add_file(PathBuf::from("/tmp/a.docx"), String::new());
        st.add_file(PathBuf::from("/tmp/b.xlsx"), String::new());
        st.add_file(PathBuf::from("/tmp/c.pdf"), String::new());
        st.recount();
        assert_eq!(st.stats.total, 3);
        assert_eq!(st.stats.percent(), 0);

        // 重复添加应该被拒绝
        assert!(!st.add_file(PathBuf::from("/tmp/a.docx"), String::new()));
        assert_eq!(st.tasks.len(), 3);

        // 不支持的格式被拒绝
        assert!(!st.add_file(PathBuf::from("/tmp/x.txt"), String::new()));
        assert_eq!(st.tasks.len(), 3);

        // 模拟：一个完成、一个失败、一个进行中
        st.tasks[0].status = Status::Done(12);
        st.tasks[1].status = Status::Error("需 OCR".into());
        st.tasks[2].status = Status::Running;
        st.recount();
        assert_eq!(st.stats.done, 1);
        assert_eq!(st.stats.failed, 1);
        assert_eq!(st.stats.running, 1);
        assert_eq!(st.stats.settled(), 2);
        assert_eq!(st.stats.percent(), 66); // 2/3
    }

    #[test]
    fn 输出路径计算() {
        let t = Task::new(1, PathBuf::from("/docs/2026/report.docx"), "2026/".into());

        // 默认：源文件同目录
        let same = t.output_path(Path::new("/docs/2026"));
        assert_eq!(same, PathBuf::from("/docs/2026/report.md"));

        // 指定目录 + 保留子目录结构
        let custom = t.output_path(Path::new("/out"));
        assert_eq!(custom, PathBuf::from("/out/2026/report.md"));

        // 无相对目录
        let t2 = Task::new(2, PathBuf::from("/docs/a.docx"), String::new());
        assert_eq!(
            t2.output_path(Path::new("/out")),
            PathBuf::from("/out/a.md")
        );
    }

    #[test]
    fn 输出目录默认值() {
        let mut st = AppState::new();
        st.add_file(PathBuf::from("/docs/sub/a.docx"), String::new());
        let t = &st.tasks[0];
        // 未指定输出目录 → 源文件所在目录
        assert_eq!(st.output_dir_for(t), PathBuf::from("/docs/sub"));

        // 指定后 → 用指定的
        st.out_dir = Some(PathBuf::from("/out"));
        assert_eq!(st.output_dir_for(t), PathBuf::from("/out"));
    }

    #[test]
    fn 官方内核能转换() {
        // 真实跑一次 anydoc 内核
        let p = "/workspace/anydoc-gui/examples/test.xlsx";
        if std::path::Path::new(p).exists() {
            let md = anydoc::to_markdown(p).expect("应该转换成功");
            assert!(md.contains("姓名"), "输出应含表头: {}", &md[..40.min(md.len())]);
            assert!(md.contains("张三"), "输出应含数据");
        }
    }

    #[test]
    fn 官方内核认字节签名() {
        use anydoc::Format;
        // docx 内容但扩展名是 pdf —— 官方内核应识别为 docx
        let p = "/workspace/anydoc-gui/examples/test.docx";
        if std::path::Path::new(p).exists() {
            let bytes = std::fs::read(p).unwrap();
            assert_eq!(
                Format::from_bytes(&bytes),
                Some(Format::Docx),
                "应通过字节签名识别为 docx"
            );
        }
    }

    #[test]
    fn 加密和损坏文件报错清晰() {
        // 不存在的文件
        let r = anydoc::to_markdown("/tmp/definitely-not-exist-xyz.docx");
        assert!(r.is_err());

        // 伪装成 docx 的垃圾
        let junk = std::env::temp_dir().join("anydoc-test-junk.docx");
        std::fs::write(&junk, b"this is not a zip archive at all").unwrap();
        let r = anydoc::to_markdown(&junk);
        assert!(r.is_err(), "垃圾文件应报错");
        let _ = std::fs::remove_file(&junk);
    }
}
