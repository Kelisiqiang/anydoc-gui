---
name: anydoc
description: Convert Word/PPT/Excel/PDF/EPUB/ODT/RTF/CSV documents to clean Markdown. Use when the user needs to process office documents, PDFs, or ebooks into LLM-readable Markdown format. Ideal for RAG preprocessing, knowledge base building, and Agent file processing pipelines.
---

# AnyDoc Skill

把任意办公文档（Word/PPT/Excel/PDF/EPUB 等）转成干净的 Markdown，喂给 LLM 吃。

## 适用场景

- 用户上传了 .docx/.pptx/.xlsx/.pdf/.epub 等文件，需要提取内容给 LLM
- 批量处理一个文件夹里的文档，做 RAG 知识库预处理
- 需要把 Office 文档转成 Markdown 用于版本控制
- 把 PDF/电子书转成结构化文本供大模型分析

## 支持格式

| 格式 | 扩展名 | 说明 |
|------|--------|------|
| Word | .docx | Microsoft Word 文档 |
| PowerPoint | .pptx | PowerPoint 演示文稿 |
| Excel | .xlsx | Excel 表格 |
| PDF | .pdf | PDF 文档 |
| EPUB | .epub | 电子书 |
| OpenDocument | .odt/.ods/.odp | 开源办公格式 |
| RTF | .rtf | 富文本格式 |
| CSV | .csv | 逗号分隔值 |

## 使用方法

### 方式 1：命令行（最快）

```bash
# 单个文件
anydoc report.docx

# 批量转换目录（递归）
anydoc ./documents/ --recursive

# 指定输出目录
anydoc report.pdf -o ./output/

# 查看帮助
anydoc --help
```

### 方式 2：GUI 工具（小白友好）

启动 `anydoc-gui.exe`，拖拽文件进去即可。

### 方式 3：编程调用

#### Rust

```rust
use anydoc::convert;

let markdown = convert("report.docx")?;
println!("{}", markdown);
```

#### Python

```python
import anydoc
md = anydoc.convert("report.pdf")
print(md)
```

#### Node.js

```javascript
const anydoc = require('@anydoc/node');
const md = await anydoc.convert('presentation.pptx');
console.log(md);
```

## 工作流建议

### RAG 预处理

```bash
# 1. 批量转换
anydoc ./knowledge-base/ -o ./md-corpus/ --recursive

# 2. 分块
# 用 LangChain / LlamaIndex 等工具对生成的 .md 文件做 chunking

# 3. 向量化入库
```

### Agent 文件处理

当用户在对话中上传了一个 .docx/.pdf 文件：

1. 调用 anydoc 转换
2. 检查输出（结构、长度）
3. 喂给 LLM 做后续分析

## 输出格式示例

### Word 文档

输入：report.docx
输出：

```markdown
# 标题

## 副标题

正文内容...

| 列1 | 列2 |
| --- | --- |
| 数据1 | 数据2 |
```

### Excel 表格

输入：data.xlsx
输出：

```markdown
## Sheet1

| 姓名 | 年龄 | 城市 |
| --- | --- | --- |
| 张三 | 28 | 北京 |
| 李四 | 32 | 上海 |
```

### PDF

输入：contract.pdf
输出：

```markdown
# 合同标题

第一条 ...

第二条 ...
```

## 性能特点

- ⚡ 中位转换时间 4.4ms（基于 Rust 内核）
- 📦 单一可执行文件，无需安装额外依赖
- 🎯 输出结构稳定，LLM 友好
- 🔄 批量处理高效，支持并发

## 链接

- 仓库：https://github.com/firecrawl/anydoc
- GUI 工具：https://github.com/yourname/anydoc-gui
- 原项目：https://github.com/firecrawl/anydoc
