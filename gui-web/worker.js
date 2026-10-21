// AnyDoc 转换 Worker
//
// 为什么用 Worker：anydoc 的 WASM 转换是同步 CPU 密集操作，
// 放主线程会冻结界面，进度条就不动了。放到 Worker 里，
// 主线程保持响应，能真实反映每个任务的进度。

let anydoc = null;
let loading = null;

/** 懒加载并缓存 WASM 内核（多个 worker 各自持有一份实例） */
async function ensureKernel() {
  if (anydoc) return anydoc;
  if (!loading) {
    loading = import('./anydoc_wasm.js').then(async (mod) => {
      await mod.default();          // 实例化 wasm
      anydoc = mod;
      return mod;
    });
  }
  return loading;
}

/** 粗略判断内容是否为纯文本（用于 CSV 兜底） */
function looksLikeText(buf) {
  const n = Math.min(buf.length, 2048);
  if (n === 0) return false;
  // 有 NUL 字节 → 二进制
  for (let i = 0; i < n; i++) {
    if (buf[i] === 0) return false;
  }
  // 可打印字符占比
  let printable = 0;
  for (let i = 0; i < n; i++) {
    const c = buf[i];
    if (c === 9 || c === 10 || c === 13 || (c >= 32 && c < 127) || c >= 128) printable++;
  }
  return printable / n > 0.9;
}

self.onmessage = async (e) => {  const { id, name, ext, bytes } = e.data;

  try {
    const t0 = performance.now();
    const mod = await ensureKernel();

    const buf = new Uint8Array(bytes);

    // 格式判定策略（优先级从高到低）：
    //   1. 字节签名 —— 最可靠，PDF header / RTF group / OLE stream / ZIP mimetype
    //   2. 纯文本内容 → CSV —— CSV 无签名，但内容特征明确。
    //      放在扩展名之前，可解决「把 .csv 改名成 .docx」这类用户改后缀场景
    //      （否则会因按 docx 解析而报 malformed）
    //   3. 扩展名 —— 兜底，覆盖被改名的二进制格式
    let format = mod.formatFromBytes(buf);

    if (!format && looksLikeText(buf)) {
      format = 'csv';
    }
    if (!format) {
      format = mod.formatFromExtension(ext);
    }
    if (!format) {
      throw Object.assign(new Error('无法识别的文件格式'), { code: 'unsupported' });
    }

    const md = mod.toMarkdownBytes(buf, format);
    const ms = performance.now() - t0;

    // 注意：md 是字符串，不能放进 transfer list（字符串没有 buffer）。
    // 直接结构化克隆即可，Markdown 文本体积小，开销可忽略。
    self.postMessage({ id, ok: true, name, md, format, ms });
  } catch (err) {
    self.postMessage({
      id,
      ok: false,
      name,
      code: err?.code || 'error',
      message: err?.message || String(err),
    });
  }
};
