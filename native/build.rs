// 构建脚本：把应用图标编译进 Windows exe
//
// 用 windres（mingw-w64）把 .rc 编译成 .o，再由 cargo 链进二进制。
// 这样 exe 自带图标，用户不需要额外的 .ico 文件。

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=resources/anydoc.ico");

    let target = std::env::var("TARGET").unwrap_or_default();

    // 只有 Windows 目标需要资源
    if !target.contains("windows") {
        return;
    }

    // 找 windres（优先用带 target 前缀的）
    let windres = if target.starts_with("x86_64") {
        "x86_64-w64-mingw32-windres"
    } else if target.starts_with("i686") {
        "i686-w64-mingw32-windres"
    } else {
        "windres"
    };

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let obj = format!("{}/anydoc-app-res.o", out_dir);
    let rc = "app.rc";

    let status = Command::new(windres)
        .args(["-O", "coff", "-I", "."])
        .arg(rc)
        .arg("-o")
        .arg(&obj)
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("cargo:rustc-link-arg-bins={}", obj);
        }
        Ok(s) => {
            println!("cargo:warning=windres failed with {s}, icon not embedded");
        }
        Err(e) => {
            println!("cargo:warning=could not run {windres}: {e}, icon not embedded");
        }
    }
}
