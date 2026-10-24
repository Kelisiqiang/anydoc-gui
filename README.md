# AnyDoc for Windows

> 把 Word / PowerPoint / Excel / PDF / EPUB 等文档转成干净的 Markdown
>
> **解析内核：[firecrawl/anydoc](https://github.com/firecrawl/anydoc) v0.2.4（MIT）**

一个用官方 anydoc 内核做的 **Windows 原生桌面工具** —— 拖文件进去，出来 Markdown。

---

## 为什么有这个项目

[firecrawl/anydoc](https://github.com/firecrawl/anydoc) 是目前最干净的文档转换引擎：

- 纯 Rust，无 Python/Node 依赖，无外部服务
- 14 种格式统一输出 GFM Markdown
- **按文件字节签名识别格式**，改过扩展名的文件照样解析对
- 官方 benchmark 中位 4.4ms

但它官方只提供 CLI 和各语言 binding，没有 Windows GUI。本项目补上这一块。

---

## 下载

Releases 页提供一个包：

| 文件 | 大小 | 说明 |
|------|------|------|
| `AnyDoc-v0.5.0-win-x64-portable.zip` | ~3.7MB | **免安装版**：解压即用，不需要装任何运行时 |

> 只要免安装版。解压后目录结构要保留（`resources/` 里有图标资源）。

---

## 使用

1. 解压整个文件夹
2. 双击 `AnyDoc.exe`
3. 拖入文档（或点「添加文件 / 添加文件夹」）→ 确认输出目录 → 点「开始转换」

**转换进度直接显示在软件窗口里**，不用切到浏览器；转换完成后点行尾的
文件夹图标即可打开输出目录。

> 全部转换在本地完成，文件不上传任何服务器。

### 命令行（可选）

同一个 exe 也带 CLI 模式：

```cmd
AnyDoc.exe report.docx              :: 单文件
AnyDoc.exe .\docs\ -o .\md\         :: 指定输出目录
```

---

## 支持格式（14 种）

| 类别 | 扩展名 |
|------|--------|
| Word | `.doc` `.docx` `.docm` |
| PowerPoint | `.ppt` `.pptx` `.pptm` |
| Excel | `.xls` `.xlsx` `.xlsm` `.xlsb` |
| OpenDocument | `.odt` `.ods` `.odp` |
| 其他 | `.rtf` `.epub` `.csv` `.pdf` |

### 格式识别看字节，不只看后缀

anydoc 读取文件签名（PDF header、RTF open group、OLE stream name、ZIP 内部 mimetype）判断真实类型。

把 `report.docx` 改名成 `report.pdf`，**它依然能正确解析** —— 这在处理用户上传的乱标文件时很关键。

---

## 已知边界

| 情况 | 行为 |
|------|------|
| 扫描件 / 图片版 PDF | ❌ 不支持，anydoc 本身不含 OCR（界面提示「需 OCR」） |
| 加密文档 | ❌ 报错 `文件已加密` |
| 结构损坏文件 | ❌ 报错 `文件结构损坏`（不会返回半成品） |
| 超出安全限制 | ❌ 报错 `超出安全限制` |
| Excel 公式 | 读取计算结果，不保留公式 |

界面报错时会显示具体原因，不是笼统的「转换失败」。

---

## 从源码编译

```bash
# 需要 Rust 1.88+（anydoc 0.2.4 要求 edition 2024）
git clone <this-repo>
cd native

# 交叉编译 Windows
rustup target add x86_64-pc-windows-gnu
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
cargo build --release --target x86_64-pc-windows-gnu
```

核心逻辑测试：

```bash
cd core-test && cargo test
```

---

## 致谢

**解析内核 anydoc** —— Copyright Firecrawl, Inc.，MIT License
https://github.com/firecrawl/anydoc

本项目仅提供 Windows GUI 封装与分发，解析能力全部来自 anydoc。

---

## 许可

MIT
