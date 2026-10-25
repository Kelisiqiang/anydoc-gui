# -*- coding: utf-8 -*-
"""
AnyDoc 本地转换器 —— 后端服务
把 Firecrawl 官方 anydoc 库包成本地网页工具：双击启动、浏览器使用，全程不用命令行。
所有文件在本机转换，除用户主动开启"联网 OCR"外，数据不出电脑。
"""

import io
import logging
import os
import re
import socket
import sys
import threading
import time
import uuid
import webbrowser
import zipfile
from pathlib import Path

from flask import Flask, jsonify, request, send_file, abort

import anydoc
from ocr_services import OcrError, parse_with_mineru

# 支持 PyInstaller 打包运行：
# - 源码/脚本运行：资源与输出都在本目录
# - exe 运行：界面资源在包内(_MEIPASS)，转换结果输出到 exe 所在目录
if getattr(sys, "frozen", False):
    APP_DIR = Path(sys.executable).resolve().parent
    RES_DIR = Path(getattr(sys, "_MEIPASS", APP_DIR))
else:
    APP_DIR = Path(__file__).resolve().parent
    RES_DIR = APP_DIR

OUTPUT_DIR = APP_DIR / "output"
OUTPUT_DIR.mkdir(exist_ok=True)

MAX_FILE_MB = 500
DEFAULT_PORT = 8123

app = Flask(__name__, static_folder=str(RES_DIR / "static"))
app.config["MAX_CONTENT_LENGTH"] = MAX_FILE_MB * 1024 * 1024

# job_id -> {batch_id, md, name}，供下载使用；重启后失效（本地工具，可接受）
JOBS = {}
LOCK = threading.Lock()

SUPPORTED_EXTS = {
    ".doc", ".docx", ".docm",
    ".ppt", ".pps", ".pot", ".pptx", ".pptm", ".ppsx", ".ppsm",
    ".xls", ".xlsx", ".xlsm", ".xlsb",
    ".odt", ".ods", ".odp",
    ".rtf", ".epub", ".csv", ".pdf",
}
SUPPORTED_LABELS = (
    "Word(.doc/.docx/.docm) · PowerPoint(.ppt/.pps/.pot/.pptx…) · Excel(.xls/.xlsx…) "
    "· PDF · OpenDocument(.odt/.ods/.odp) · RTF · EPUB · CSV"
)

ERROR_MESSAGES = {
    "UnsupportedError": "文件格式不支持，或该格式无法转换",
    "NeedsOcrError": "这是扫描版/纯图片 PDF，本地识别不了，请在下方开启「联网 OCR」后重试",
    "MalformedError": "文件结构损坏，无法提取出有效内容",
    "EncryptedError": "文件已加密或受密码保护，无法打开",
    "ResourceLimitError": "文件超出安全解析限制（过大、嵌套过深或节点过多）",
    "MissingPartError": "文件缺少必要组成部分，无法转换",
    "HostedError": "联网 OCR（Firecrawl）解析失败，多为云端额度受限或文件过大",
    "OSError": "文件无法读取，请确认文件未被占用或已损坏",
}


def sanitize_filename(name):
    """保留中文的安全文件名：去掉路径分隔符与非法字符。"""
    name = os.path.basename(name or "")
    name = re.sub(r'[\\/:*?"<>|\x00-\x1f]', "_", name).strip()
    name = name[:120] or "document"
    return name


def human_error(error):
    name = type(error).__name__
    msg = str(error).strip()
    if name == "NeedsOcrError" and msg:
        return f"{ERROR_MESSAGES.get(name)}（原因：{msg}）"
    return ERROR_MESSAGES.get(name, f"{name}: {error}")


def extract_pdf_text_fallback(path, min_chars=300):
    """pdfminer 纯文本兜底：anydoc 因"含少量图片页"拒绝整本时，提取可复制的文字层。

    财新周刊这类"文字层完整、夹带少量整版图页"的 PDF，anydoc 会抛 NeedsOcrError；
    此时用 pdfminer 把文字层提取出来作为保底（内容完整、排版简化）。
    返回 None 表示确实提取不出文字（真·扫描版）。
    """
    try:
        logging.getLogger("pdfminer").setLevel(logging.ERROR)
        from pdfminer.high_level import extract_text
        text = extract_text(str(path))
    except Exception:
        return None
    text = (text or "").strip()
    if len(text) < min_chars:
        return None
    return text


@app.route("/")
def index():
    return app.send_static_file("index.html")


@app.route("/api/info")
def api_info():
    try:
        version = anydoc.version("firecrawl-anydoc")
    except Exception:
        version = "unknown"
    return jsonify(
        ok=True,
        engine="firecrawl-anydoc",
        version=str(version),
        max_file_mb=MAX_FILE_MB,
    )


@app.route("/api/convert", methods=["POST"])
def api_convert():
    upload = request.files.get("file")
    if upload is None or not upload.filename:
        return jsonify(ok=False, error="未收到文件"), 400

    ext = os.path.splitext(upload.filename)[1].lower()
    if ext not in SUPPORTED_EXTS:
        return jsonify(ok=False, error=f"暂不支持「{ext}」格式。支持的格式：{SUPPORTED_LABELS}"), 400

    ocr = request.form.get("ocr") == "1"
    api_key = (request.form.get("api_key") or "").strip() or None
    mineru_keys = [k.strip() for k in
                   (request.form.get("mineru_keys") or "").replace("\r", "\n").split("\n")
                   if k.strip()]

    batch_id = time.strftime("%Y%m%d-%H%M%S") + "-" + uuid.uuid4().hex[:6]
    batch_dir = OUTPUT_DIR / batch_id
    batch_dir.mkdir(parents=True, exist_ok=True)

    safe = sanitize_filename(upload.filename)
    src_path = batch_dir / safe
    upload.save(str(src_path))

    md, err = None, None
    if not ocr:
        # 纯本地转换
        try:
            md = anydoc.to_markdown(str(src_path))
        except Exception as exc:  # anydoc.ConvertError 的各类子类及 OSError
            if type(exc).__name__ == "NeedsOcrError":
                # 文字层完整但夹带少量图片页 → pdfminer 纯文本兜底（不花钱、不联网）
                fallback = extract_pdf_text_fallback(str(src_path))
                if fallback is not None:
                    md = ("> 说明：本文件含少量整版图片页，已用「本地纯文本模式」提取（内容完整、排版简化）。\n\n"
                          + fallback)
                else:
                    err = human_error(exc)
            else:
                err = human_error(exc)
    else:
        # 开启联网 OCR：本地优先（含纯文本兜底）→ MinerU 轮询 → Firecrawl 兜底
        # 1) 本地先试：文字版 PDF/文档不需要云端，更快更省
        try:
            md = anydoc.to_markdown(str(src_path))
        except Exception as exc:
            if type(exc).__name__ == "NeedsOcrError":
                fallback = extract_pdf_text_fallback(str(src_path))
                if fallback is not None:
                    md = ("> 说明：本文件含少量整版图片页，已用「本地纯文本模式」提取（内容完整、排版简化）。\n\n"
                          + fallback)
            elif type(exc).__name__ != "NeedsOcrError":
                err = human_error(exc)
        # 2) 扫描版：MinerU Token 轮询（可多个）
        if md is None and err is None:
            ocr_errors = []
            if mineru_keys:
                try:
                    md = parse_with_mineru(mineru_keys, str(src_path), safe)
                except OcrError as e:
                    ocr_errors.append(f"MinerU：{e}")
            # 3) Firecrawl 官方托管 OCR 兜底
            if md is None:
                try:
                    kw = {"ocr": "hosted"}
                    if api_key:
                        kw["api_key"] = api_key
                    md = anydoc.to_markdown(str(src_path), **kw)
                except Exception as exc:
                    size_mb = src_path.stat().st_size / (1024 * 1024)
                    hint = human_error(exc)
                    if type(exc).__name__ == "HostedError":
                        hint += "。建议：① 在「MinerU API Token」框填写 Token 后重试（独立额度、支持大文件）；② 稍后重试"
                    ocr_errors.append(f"Firecrawl：{hint}")
            if md is None:
                err = "；".join(ocr_errors) if ocr_errors else \
                    "联网 OCR 转换失败：未配置任何 OCR 服务（可在上方填写 MinerU Token 或 Firecrawl Key）"

    if err:
        return jsonify(ok=False, name=upload.filename, error=err), 200

    md_name = Path(safe).stem + ".md"
    md_path = batch_dir / md_name
    try:
        md_path.write_text(md, encoding="utf-8")
    except Exception as exc:
        return jsonify(ok=False, name=upload.filename, error=f"写入结果失败：{exc}"), 200

    job_id = uuid.uuid4().hex
    with LOCK:
        JOBS[job_id] = {"batch_id": batch_id, "md": str(md_path), "name": md_name}

    return jsonify(
        ok=True, name=md_name, job_id=job_id, batch_id=batch_id,
        markdown=md, chars=len(md),
    )


@app.route("/api/download/<job_id>")
def api_download(job_id):
    with LOCK:
        job = JOBS.get(job_id)
    if not job:
        abort(404)
    return send_file(job["md"], as_attachment=True, download_name=job["name"],
                     mimetype="text/markdown")


@app.route("/api/download-zip", methods=["POST"])
def api_download_zip():
    data = request.get_json(silent=True) or {}
    job_ids = data.get("job_ids") or []
    if not job_ids:
        abort(400)
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        for jid in job_ids:
            with LOCK:
                job = JOBS.get(jid)
            if job and os.path.isfile(job["md"]):
                zf.write(job["md"], arcname=job["name"])
    buf.seek(0)
    stamp = time.strftime("%Y%m%d-%H%M%S")
    return send_file(buf, as_attachment=True,
                     download_name=f"anydoc-markdown-{stamp}.zip",
                     mimetype="application/zip")


@app.route("/api/open-output")
def api_open_output():
    try:
        os.startfile(str(OUTPUT_DIR))  # noqa: S606 本地工具，用户主动点击
        return jsonify(ok=True)
    except Exception as exc:
        return jsonify(ok=False, error=str(exc))


@app.route("/api/shutdown", methods=["POST"])
def api_shutdown():
    """界面按钮退出：短暂延迟后结束进程。"""
    def _die():
        time.sleep(0.6)
        os._exit(0)
    threading.Thread(target=_die, daemon=True).start()
    return jsonify(ok=True)


def _port_free(port):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        try:
            s.bind(("127.0.0.1", port))
            return True
        except OSError:
            return False


SERVER_PORT = {"value": None}


def start_server(open_browser=True):
    """启动本地 Flask 服务（阻塞）。实际端口写入 SERVER_PORT。"""
    port = int(os.environ.get("ANYDOC_PORT", str(DEFAULT_PORT)))
    while not _port_free(port):
        port += 1
    SERVER_PORT["value"] = port
    if open_browser:
        threading.Timer(1.2, lambda: webbrowser.open(f"http://127.0.0.1:{port}")).start()
    try:
        print(f"AnyDoc local converter is running at http://127.0.0.1:{port}")
        print("Close this window to exit.")
    except Exception:
        pass
    app.run(host="127.0.0.1", port=port, debug=False)


if __name__ == "__main__":
    try:
        start_server()
    except Exception:
        import traceback
        try:
            with open(APP_DIR / "AnyDocConverter-error.log", "w", encoding="utf-8") as f:
                traceback.print_exc(file=f)
        except Exception:
            pass
        raise
