# AnyDoc v0.2.0 — 文档秒变 Markdown

**解析内核：[firecrawl/anydoc](https://github.com/firecrawl/anydoc) v0.2.4（MIT License）**

---

## 这个包里有什么

```
anydoc-gui/
├── anydoc-gui.exe      ← GUI 启动器（双击这个）
├── anydoc.exe          ← 命令行版（可选）
└── gui/                ← GUI 资源（请勿删除）
    ├── index.html
    ├── app.css
    ├── app.js
    ├── anydoc_wasm.js
    └── anydoc_wasm_bg.wasm   ← 官方 anydoc 内核（WebAssembly）
```

---

## GUI 用法（推荐给不熟命令行的朋友）

1. 解压整个文件夹
2. **双击 `anydoc-gui.exe`**
3. 浏览器自动打开界面（默认 Edge/Chrome）
4. 把文档拖进网页 → 点「开始转换」→ 点「另存为」选目录

> ⚠️ 请务必**完整解压**再运行，不要只单独拷 exe。
> ⚠️ `gui/` 目录是内核文件，删了就用不了。

### 转换在哪里发生？

**全部在你的电脑本地完成。** 内核是 WebAssembly，文件不上传、不联网、不经过任何服务器。

---

## 命令行用法（给开发者）

```cmd
:: 单文件
anydoc.exe report.docx

:: 整个目录（递归）
anydoc.exe .\docs\ -r

:: 指定输出目录 + 详细输出
anydoc.exe .\docs\ -o .\markdown\ -r -v

:: 查看帮助
anydoc.exe --help
```

---

## 支持的 14 种格式

| 类别 | 扩展名 |
|------|--------|
| Word | `.doc` `.docx` `.docm` |
| PowerPoint | `.ppt` `.pptx` `.pptm` |
| Excel | `.xls` `.xlsx` `.xlsm` |
| OpenDocument | `.odt` `.ods` `.odp` |
| 其他 | `.rtf` `.epub` `.csv` `.pdf` |

### 格式识别看内容，不只看后缀

anydoc 读**文件字节签名**判断真实格式（PDF 头、RTF 起始标记、OLE 流名、ZIP 内部 mimetype）。

所以：把 `报告.docx` 改名成 `报告.pdf`，**它照样能正确解析**。这在处理用户上传的乱标文件时很关键。

---

## 已知边界（请务必了解）

| 情况 | 行为 |
|------|------|
| 扫描件 / 图片版 PDF | ❌ **不支持**，需要 OCR（anydoc 本身不含 OCR） |
| 加密 / 有密码的文档 | ❌ 报错 `encrypted` |
| 结构损坏的文件 | ❌ 报错 `malformed`，不会给你半成品 |
| 超出安全限制（解压炸弹等） | ❌ 报错 `resourceLimit` |
| Excel 公式 | 读取计算结果，不保留公式本身 |

界面报错时会给出**具体原因**，不是笼统的「转换失败」。

---

## 关于数字签名提示

本程序**未使用受信任的代码签名证书**，Windows 可能提示：

> Windows 已保护你的电脑 → 更多信息 → 仍要运行

这是因为签名证书需要向 CA 机构购买（每年数百至数千元）。点「仍要运行」即可正常使用。

---

## 许可与致谢

- 本项目代码：MIT
- **解析内核 anydoc：Copyright Firecrawl, Inc.，MIT License** — https://github.com/firecrawl/anydoc
- 感谢 Firecrawl 团队开源了这个优秀的文档转换引擎