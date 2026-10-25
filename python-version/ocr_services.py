# -*- coding: utf-8 -*-
"""
自定义在线 OCR 服务：MinerU 精准解析 API（v4 协议）+ 多 Token 自动轮询。

流程（对应 https://mineru.net/doc/docs/ ）：
1. POST /api/v4/file-urls/batch 申请上传链接（Authorization: Bearer <token>）
2. PUT 上传文件到 upload_url（不设置 Content-Type）
3. GET /api/v4/extract-results/batch/{batch_id} 轮询解析结果
4. 结果 done 后下载 full_zip_url 压缩包，解压取 full.md

多 Token 轮询：按顺序尝试，Token 认证失败（A0202/A0211、HTTP 401/403）或网络/服务异常时
自动切换下一个 Token；全部失败则抛出带各 Token 错误原因的 OcrError。
"""

import io
import json
import time
import urllib.error
import urllib.request
import zipfile

MINERU_BASE = "https://mineru.net/api/v4"
MINERU_BATCH = MINERU_BASE + "/file-urls/batch"
MINERU_RESULTS = MINERU_BASE + "/extract-results/batch"


class OcrError(Exception):
    """OCR 服务调用失败。retryable=True 表示换一个 Token/服务重试可能成功。"""

    def __init__(self, msg, retryable=True):
        super().__init__(msg)
        self.retryable = retryable


def _http(method, url, headers=None, data=None, timeout=60):
    req = urllib.request.Request(url, data=data, headers=headers or {}, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return resp.status, resp.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()
    except urllib.error.URLError as e:
        raise OcrError(f"网络错误：{e.reason}", retryable=True)


def _mineru_token_retryable(code):
    """MinerU 错误码是否属于『换 Token 可解』。"""
    if isinstance(code, str) and code.startswith("A0"):
        return True  # A0202 Token 错误 / A0211 Token 过期 等认证类
    return False


def _download_markdown(zip_url, timeout=90):
    """下载 MinerU 结果 zip 并提取 full.md。"""
    status, blob = _http("GET", zip_url, timeout=timeout)
    if status != 200:
        raise OcrError(f"结果下载失败 HTTP {status}", retryable=True)
    try:
        with zipfile.ZipFile(io.BytesIO(blob)) as zf:
            md_name = None
            for n in zf.namelist():
                low = n.lower()
                if low.endswith("full.md") or low.endswith(".md"):
                    md_name = n
                    break
            if md_name is None:
                raise OcrError("结果包中未找到 Markdown 文件", retryable=False)
            return zf.read(md_name).decode("utf-8", "replace")
    except zipfile.BadZipFile:
        raise OcrError("结果包损坏，无法解压", retryable=True)


def mineru_parse(token, file_path, filename, model_version="vlm",
                 timeout=60, poll_interval=5, poll_max=900):
    """用单个 MinerU Token 解析本地文件，返回 Markdown 文本。"""
    headers = {
        "Content-Type": "application/json",
        "Authorization": "Bearer " + token,
    }

    # 1) 申请上传链接
    body = json.dumps({
        "files": [{"name": filename}],
        "model_version": model_version,
    }).encode("utf-8")
    status, raw = _http("POST", MINERU_BATCH, headers=headers, data=body, timeout=timeout)
    if status != 200:
        raise OcrError(f"申请上传链接失败 HTTP {status}", retryable=(status in (401, 403, 429) or status >= 500))
    try:
        j = json.loads(raw.decode("utf-8", "replace"))
    except ValueError:
        raise OcrError("申请上传链接响应异常", retryable=True)
    if j.get("code") != 0:
        code = j.get("code")
        raise OcrError(f"{j.get('msg') or '未知错误'} (code={code})",
                       retryable=_mineru_token_retryable(code))
    batch_id = j["data"]["batch_id"]
    upload_url = j["data"]["file_urls"][0]

    # 2) 上传文件（按官方要求不设置 Content-Type）
    with open(file_path, "rb") as f:
        blob = f.read()
    status, raw = _http("PUT", upload_url, data=blob, timeout=timeout * 2)
    if status != 200:
        raise OcrError(f"文件上传失败 HTTP {status}", retryable=(status >= 500 or status in (401, 403, 429)))

    # 3) 轮询结果
    deadline = time.monotonic() + poll_max
    while time.monotonic() < deadline:
        time.sleep(poll_interval)
        status, raw = _http("GET", f"{MINERU_RESULTS}/{batch_id}",
                            headers={"Authorization": "Bearer " + token}, timeout=timeout)
        if status != 200:
            raise OcrError(f"查询结果失败 HTTP {status}", retryable=True)
        try:
            j = json.loads(raw.decode("utf-8", "replace"))
        except ValueError:
            raise OcrError("查询结果响应异常", retryable=True)
        if j.get("code") != 0:
            code = j.get("code")
            raise OcrError(f"{j.get('msg') or '未知错误'} (code={code})",
                           retryable=_mineru_token_retryable(code))
        result = j["data"]["extract_result"][0]
        state = result.get("state")
        if state == "done":
            zip_url = result.get("full_zip_url")
            if not zip_url:
                raise OcrError("解析完成但缺少结果链接", retryable=False)
            return _download_markdown(zip_url, timeout=timeout)
        if state == "failed":
            raise OcrError(f"解析失败：{result.get('err_msg') or '未知原因'}",
                           retryable=False)
        # waiting-file / pending / running / converting → 继续等待
    raise OcrError("解析超时（超过 15 分钟）", retryable=True)


def parse_with_mineru(tokens, file_path, filename):
    """多个 Token 自动轮询：按顺序尝试，失败换下一个，全部失败给出汇总原因。"""
    errors = []
    for i, token in enumerate(tokens):
        try:
            return mineru_parse(token, file_path, filename)
        except OcrError as e:
            short = (token[:6] + "…") if len(token) > 8 else token
            errors.append(f"Token{i + 1}({short})：{e}")
            if not e.retryable:
                # 参数/服务端拒绝类错误，换 Token 通常也无意义，但继续尝试后面的也无妨
                continue
    if errors:
        raise OcrError("；".join(errors))
    raise OcrError("未配置 MinerU Token")
