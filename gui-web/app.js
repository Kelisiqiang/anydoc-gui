// AnyDoc — 前端主逻辑
//
// 职责：
//   1. 收集文件（拖拽 / 选择文件 / 选择目录）
//   2. 用 Worker 池并发转换（WASM 内核，不卡界面）
//   3. 分任务进度 + 总进度
//   4. 通过宿主 HTTP API 写盘（浏览器无法直接写任意路径）

// ─────────── 状态 ───────────
const SUPPORTED = new Set([
  'doc','docx','docm','ppt','pptx','pptm','xls','xlsx','xlsm',
  'odt','ods','odp','rtf','epub','csv','pdf',
]);

const ICON = {
  doc:'📄', docx:'📘', docm:'📘', ppt:'📙', pptx:'📙', pptm:'📙',
  xls:'📗', xlsx:'📗', xlsm:'📗', odt:'📄', ods:'📊', odp:'🎞️',
  rtf:'📃', epub:'📚', csv:'📋', pdf:'📕',
};

let items = [];              // {id, name, path, ext, size, file, status, ms, md, err}
let nextId = 1;
let running = false;
let outDir = '';             // 输出目录（宿主路径）
let total = 0, done = 0, failed = 0;
let summary = '';            // 本轮转换的统计摘要

const $ = id => document.getElementById(id);
const el = {
  drop: $('drop'), rows: $('rows'), list: $('list'), empty: $('empty'),
  count: $('count'), stat: $('stat'), pct: $('pct'), fill: $('fill'),
  kernel: $('kernel'), foot: $('foot'),
  fPicker: $('fPicker'), dPicker: $('dPicker'),
  btnFiles: $('btnFiles'), btnDir: $('btnDir'),
  btnOut: $('btnOut'), outLabel: $('outLabel'),
  btnClear: $('btnClear'), btnAll: $('btnAll'),
};

// ─────────── 工具 ───────────
const fmtSize = b =>
  b < 1024 ? `${b} B`
  : b < 1048576 ? `${(b/1024).toFixed(1)} KB`
  : `${(b/1048576).toFixed(1)} MB`;

const extOf = name => {
  const i = name.lastIndexOf('.');
  return i < 0 ? '' : name.slice(i+1).toLowerCase();
};

const baseOf = name => name.replace(/\.[^.]+$/, '');

function setKernel(state, text) {
  el.kernel.className = 'kernel ' + state;
  el.kernel.textContent = text;
}

// ─────────── 收集文件 ───────────
function addFiles(files) {
  const before = items.length;

  for (const f of files) {
    const name = f.name || f.webkitRelativePath?.split('/').pop() || 'unknown';
    const ext = extOf(name);
    if (!SUPPORTED.has(ext)) continue;

    // 相对路径（选目录时保留子目录结构）
    let rel = f.webkitRelativePath || '';
    if (rel) {
      const parts = rel.split('/');
      parts.pop();
      rel = parts.join('/') + '/';
    }

    items.push({
      id: nextId++,
      name,
      rel,
      ext,
      size: f.size,
      file: f,
      status: 'wait',
      ms: 0,
      md: null,
      err: null,
    });
  }

  if (items.length > before) {
    render();
    if (!running) setStat(`待转换 ${items.length} 个文件`);
  }
}

// ─────────── 渲染 ───────────
function render() {
  const has = items.length > 0;
  el.list.hidden = !has;
  el.empty.hidden = has;
  if (!has) { el.count.textContent = '0 个文件'; return; }

  el.count.textContent = `${items.length} 个文件`;

  // 增量更新：只重绘变化过的行，避免大列表卡顿
  const frag = document.createDocumentFragment();
  for (const it of items) {
    const row = document.createElement('div');
    row.className = `row ${it.status}`;

    let st = '', sub = '';
    if (it.status === 'wait') {
      st = '<span class="st wait">等待</span>';
      sub = `${fmtSize(it.size)} · ${it.ext.toUpperCase()}`;
    } else if (it.status === 'run') {
      st = '<span class="st run"><span class="spin"></span>转换中</span>';
      sub = `${fmtSize(it.size)} · ${it.ext.toUpperCase()}`;
    } else if (it.status === 'done') {
      st = `<span class="st done">✓ ${it.ms.toFixed(0)}ms</span>`;
      sub = `已转换 · ${fmtSize(new Blob([it.md]).size)}`;
    } else if (it.status === 'error') {
      st = '<span class="st error">✗ 失败</span>';
      sub = it.err || '转换失败';
    }

    row.innerHTML =
      `<span class="ico">${ICON[it.ext] || '📄'}</span>
       <div class="meta">
         <div class="nm">${esc(it.name)}</div>
         <div class="sub">${esc(sub)}</div>
       </div>
       ${st}
       <button class="rm" title="移除">✕</button>`;

    row.querySelector('.rm').onclick = () => {
      if (running) return;
      items = items.filter(x => x.id !== it.id);
      render();
    };

    frag.appendChild(row);
  }

  el.rows.replaceChildren(frag);
}

const esc = s => String(s).replace(/[&<>"]/g, c =>
  ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));

// ─────────── 总进度 ───────────
function updateTotal() {
  const pct = total ? Math.round(done / total * 100) : 0;
  el.fill.style.width = `${pct}%`;
  el.pct.textContent = `${pct}%`;
  el.foot.querySelector('.bar').className = 'bar' + (failed ? ' err' : (done && done===total ? ' ok' : ''));
}

function setStat(t) { el.stat.textContent = t; }

/** 转换中的实时状态：已完成 + 进行中 = 当前进度 */
function liveStat() {
  const running_n = items.filter(i => i.status === 'run').length;
  const settled = done + failed;
  return `正在转换 ${settled}/${total}（${running_n} 个进行中）`;
}

// ─────────── Worker 池 ───────────
const POOL_SIZE = Math.min(4, Math.max(2, (navigator.hardwareConcurrency || 4) - 1));
let workers = [];
let idle = [];          // 空闲 worker
const pendingQ = [];    // 等待派发的任务

function initWorkers() {
  for (let i = 0; i < POOL_SIZE; i++) {
    const w = new Worker('./worker.js', { type: 'module' });
    w.onmessage = onWorkerDone;
    w.onerror = (e) => {
      setStat(`内核错误: ${e.message || '未知'}`);
      setKernel('err', '内核错误');
    };
    workers.push(w);
    idle.push(w);
  }
}

function onWorkerDone(e) {
  const { id, ok, md, format, ms, code, message } = e.data;
  const it = items.find(x => x.id === id);

  if (it) {
    if (ok) {
      it.status = 'done'; it.md = md; it.ms = ms; it.format = format;
      done++;
    } else {
      it.status = 'error';
      it.err = describeError(code, message);
      failed++;
    }
  }

  render();
  updateTotal();

  // 进行中时实时刷新状态栏
  if (running) setStat(liveStat());

  // 把 worker 放回池中，并取下一个任务
  const w = idle.pop() || workers[0];
  const next = pendingQ.shift();
  if (next) {
    dispatch(next, w);
  } else {
    idle.push(w);
    if (done + failed === total) finish();
  }
}

function describeError(code, msg) {
  switch (code) {
    case 'needsOcr':     return '需 OCR（anydoc 不含 OCR）';
    case 'encrypted':    return '文件已加密';
    case 'malformed':    return '文件结构损坏';
    case 'resourceLimit':return '超出安全限制';
    case 'unsupported':  return '不支持的格式';
    case 'missingPart':  return '缺少必要内容';
    default:             return msg || '转换失败';
  }
}

/** 把任务派给某个 worker */
function dispatch(it, w) {
  it.status = 'run';
  render();
  if (running) setStat(liveStat());

  it.file.arrayBuffer()
    .then(buf => {
      w.postMessage(
        { id: it.id, name: it.name, ext: it.ext, bytes: buf },
        [buf]
      );
    })
    .catch(err => {
      it.status = 'error';
      it.err = `读取失败: ${err.message}`;
      failed++;
      render();
      updateTotal();
      idle.push(w);
      if (done + failed === total) finish();
      else pump();
    });
}

/** 从队列取任务填补空闲 worker */
function pump() {
  while (idle.length && pendingQ.length) {
    const w = idle.pop();
    const it = pendingQ.shift();
    dispatch(it, w);
  }
  if (!pendingQ.length && !idle.length && done + failed === total && total > 0) {
    finish();
  }
}

// ─────────── 主流程 ───────────
async function run() {
  if (running) return;

  const todo = items.filter(i => i.status === 'wait' || i.status === 'error');
  if (!todo.length) return;

  // 重置错误项
  for (const it of todo) { it.status = 'wait'; it.err = null; }

  running = true;
  finished = false;
  total = items.length;
  done = 0; failed = 0;
  pendingQ.length = 0;
  updateTotal();

  const t0 = performance.now();
  setStat(`正在转换 0/${total}`);
  setKernel('busy', '转换中');
  if (!workers.length) initWorkers();

  // 全部入队，再由 pump() 分发给空闲 worker
  pendingQ.length = 0;
  pendingQ.push(...todo);

  window.__t0 = t0;
  pump();
}

let finished = false;
function finish() {
  if (finished) return;
  finished = true;
  running = false;
  const dt = (performance.now() - (window.__t0 || performance.now())) / 1000;
  summary = `${done} 成功 · ${failed} 失败 · ${dt.toFixed(1)}s`;
  setStat(summary);
  setKernel('ready', '内核就绪');
  updateTotal();
  render();              // 刷新按钮文案（转换中… → 重新保存）
  writeOut();
}

// ─────────── 写盘（走宿主 API）───────────
async function writeOut() {
  if (!outDir) {
    setStat(`${summary} · 未设置输出目录，选「输出到」可自动保存`);
    return;
  }

  const ok = items.filter(i => i.status === 'done');
  setStat(`正在写入 ${ok.length} 个文件到 ${outDir}…`);

  let written = 0, fail = 0;
  for (const it of ok) {
    try {
      const r = await fetch('/api/write', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          dir: outDir,
          path: it.rel + baseOf(it.name) + '.md',
          content: it.md,
        }),
      });
      if (!r.ok) throw new Error(await r.text());
      written++;
    } catch (e) {
      fail++;
    }
  }

  setStat(`${summary} · 已写入 ${written} 个${fail ? ` · ${fail} 个写入失败` : ''}`);
}

// ─────────── 事件绑定 ───────────

// 拖放
['dragenter','dragover'].forEach(ev =>
  el.drop.addEventListener(ev, e => { e.preventDefault(); el.drop.classList.add('hot'); }));
['dragleave','dragend'].forEach(ev =>
  el.drop.addEventListener(ev, e => { e.preventDefault(); el.drop.classList.remove('hot'); }));

el.drop.addEventListener('drop', e => {
  e.preventDefault();
  el.drop.classList.remove('hot');
  const files = [...(e.dataTransfer?.files || [])];
  if (files.length) addFiles(files);
});

// 整页拖放（防止拖到外面浏览器直接打开文件）
window.addEventListener('dragover', e => e.preventDefault());
window.addEventListener('drop', e => e.preventDefault());

el.drop.addEventListener('click', () => el.fPicker.click());
el.btnFiles.addEventListener('click', e => { e.stopPropagation(); el.fPicker.click(); });
el.btnDir.addEventListener('click', e => { e.stopPropagation(); el.dPicker.click(); });

el.fPicker.addEventListener('change', e => {
  addFiles([...e.target.files]);
  e.target.value = '';
});
el.dPicker.addEventListener('change', e => {
  addFiles([...e.target.files]);
  e.target.value = '';
});

// 输出目录
el.btnOut.addEventListener('click', async () => {
  try {
    const r = await fetch('/api/pickdir', { method: 'POST' });
    if (!r.ok) throw new Error();
    const { path } = await r.json();
    outDir = path;
    el.outLabel.textContent = path.split(/[\\/]/).pop() || path;
    el.outLabel.title = path;
    setStat(`输出目录: ${path}`);
  } catch {
    setStat('选择输出目录失败');
  }
});

// 清空
el.btnClear.addEventListener('click', () => {
  if (running) return;
  items = [];
  done = 0; failed = 0; total = 0;
  render(); updateTotal();
  setStat('就绪');
});

// 「全选目录」—— 重新选一个目录并入队列
el.btnAll.addEventListener('click', () => el.dPicker.click());

// 主按钮：按状态切换
const mainBtn = document.createElement('button');
mainBtn.className = 'btn primary';
mainBtn.textContent = '开始转换';
mainBtn.style.marginLeft = '10px';
el.btnAll.parentElement.appendChild(mainBtn);

mainBtn.onclick = async () => {
  if (!running && items.some(i => i.status === 'done')) {
    // 已完成 → 重新写盘
    if (!outDir) {
      setStat('请先点右上角「输出到」选择目录');
      el.btnOut.focus();
      return;
    }
    writeOut();
    return;
  }
  await run();
};

function refreshMainBtn() {
  const anyDone = items.some(i => i.status === 'done');
  mainBtn.textContent = running ? '转换中…' : (anyDone ? '重新保存' : '开始转换');
  mainBtn.disabled = running || items.length === 0;
  el.btnAll.disabled = running;
  el.btnClear.disabled = running;
}

// 状态变化时刷新按钮
const _render = render;
render = function() {
  _render();
  refreshMainBtn();
};

// ─────────── 启动 ───────────
(async () => {
  setKernel('busy', '内核加载中');
  try {
    const r = await fetch('/api/health');
    const h = await r.json();
    setKernel('ready', `内核就绪 · anydoc ${h.version}`);
  } catch {
    setKernel('err', '宿主未就绪');
  }
  render();
  setStat('就绪 · 拖入文档开始');
})();
