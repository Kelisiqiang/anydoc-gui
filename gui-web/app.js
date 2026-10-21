// AnyDoc GUI —— 解析内核使用官方 firecrawl/anydoc (WASM, MIT)
// 文件选择 / 拖放 → WASM 本地转换 → 保存为 .md
// 全程本地，不上传任何数据。

let anydoc = null;
let queue = [];

const $ = id => document.getElementById(id);
const el = {
  drop: $('drop'), pick: $('pick'), file: $('file'), queue: $('queue'),
  run: $('run'), clear: $('clear'), save: $('save'), log: $('log'),
  badge: $('badge'), rt: $('rt'), mkdir: $('mkdir'), copy: $('copy'),
};

const EXT = ['doc','docx','docm','ppt','pptx','pptm','xls','xlsx','xlsm',
             'odt','ods','odp','rtf','epub','csv','pdf'];
const ICON = { doc:'📄', docx:'📘', docm:'📘', ppt:'📙', pptx:'📙', pptm:'📙',
               xls:'📗', xlsx:'📗', xlsm:'📗', odt:'📄', ods:'📊', odp:'🎞️',
               rtf:'📃', epub:'📚', csv:'📋', pdf:'📕' };

function log(msg, cls = '') {
  const d = document.createElement('div');
  if (cls) d.className = cls;
  d.textContent = msg;
  el.log.appendChild(d);
  el.log.scrollTop = el.log.scrollHeight;
}

function ext(name) {
  const i = name.lastIndexOf('.');
  return i < 0 ? '' : name.slice(i + 1).toLowerCase();
}

// ---------- 内核加载 ----------
async function boot() {
  try {
    log('加载 anydoc 内核 (WebAssembly)…', 'acc');
    const mod = await import('./anydoc_wasm.js');
    await mod.default();
    anydoc = mod;
    el.badge.textContent = '内核就绪 · anydoc v0.2.4';
    el.badge.classList.add('ready');
    log('✓ 内核就绪，支持 14 种格式', 'ok');
  } catch (e) {
    el.badge.textContent = '内核加载失败';
    el.badge.classList.add('err');
    log('✗ ' + e, 'err');
  }
}

// ---------- 队列 ----------
function add(files) {
  for (const f of files) {
    const e = ext(f.name);
    if (!EXT.includes(e)) {
      log(`✗ 跳过不支持的格式: ${f.name}`, 'err');
      continue;
    }
    queue.push({ file: f, name: f.name, ext: e, status: 'wait', msg: '' });
  }
  render();
}

function render() {
  el.queue.innerHTML = '';
  queue.forEach((it, i) => {
    const d = document.createElement('div');
    d.className = 'qi';
    const txt = it.status === 'ok' ? `✓ ${it.msg}`
               : it.status === 'err' ? `✗ ${it.msg}`
               : it.status === 'run' ? '转换中…' : '待转换';
    d.innerHTML =
      `<span class="ico">${ICON[it.ext] || '📄'}</span>
       <div class="meta">
         <div class="nm">${esc(it.name)}</div>
         <div class="sub">${(it.file.size/1024).toFixed(1)} KB</div>
       </div>
       <span class="st ${it.status}">${txt}</span>
       <button class="x" title="移除">✕</button>`;
    d.querySelector('.x').onclick = () => { queue.splice(i,1); render(); };
    el.queue.appendChild(d);
  });
  el.run.disabled = queue.filter(q => q.status !== 'ok').length === 0;
}

const esc = s => s.replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));

// ---------- 转换 ----------
async function run() {
  const todo = queue.filter(q => q.status !== 'ok');
  if (!todo.length) return;

  const t0 = performance.now();
  let ok = 0, bad = 0;

  for (const it of todo) {
    it.status = 'run'; render();
    try {
      const buf = new Uint8Array(await it.file.arrayBuffer());
      const fmt = anydoc.formatFromBytes(buf) || anydoc.formatFromExtension(it.ext);
      const md = anydoc.toMarkdownBytes(buf, fmt);
      it.md = md;
      it.status = 'ok';
      it.msg = `${(md.length/1024).toFixed(1)} KB · ${fmt.toUpperCase()}`;
      ok++;
      log(`✓ ${it.name} → ${it.msg}`);
    } catch (e) {
      it.status = 'err';
      it.msg = e.code || '转换失败';
      bad++;
      const hint = {
        needsOcr: '扫描件/PDF 含图片页，需要 OCR（anydoc 不含 OCR）',
        encrypted: '文件已加密或受密码保护',
        resourceLimit: '文件超出安全限制（解压/嵌套过深）',
        malformed: '文件结构损坏，无法提取内容',
        unsupported: '不支持的格式',
      }[e.code] || e.code || e.message || String(e);
      log(`✗ ${it.name} — ${hint}`, 'err');
    }
    render();
    await new Promise(r => setTimeout(r, 0));
  }

  const dt = performance.now() - t0;
  el.rt.textContent = `${ok} 成功 · ${bad} 失败 · ${dt.toFixed(0)}ms`;
  log(`— 完成：${ok} 成功 / ${bad} 失败，耗时 ${dt.toFixed(0)}ms`, 'acc');
}

// ---------- 保存 ----------
function baseName(n) { return n.replace(/\.[^.]+$/, ''); }

async function saveAll() {
  const done = queue.filter(q => q.status === 'ok' && q.md);
  if (!done.length) return;

  const dir = await pickDir();
  if (!dir) return;

  let saved = 0;
  for (const it of done) {
    try {
      let target = dir;
      if (el.mkdir.checked) {
        target = `${dir}/${baseName(it.name)}`;
        await mkDir(target);
      }
      await writeText(`${target}/${baseName(it.name)}.md`, it.md);
      saved++;
    } catch (e) {
      log(`✗ 保存失败 ${it.name}: ${e.message || e}`, 'err');
    }
  }
  log(`✓ 已保存 ${saved}/${done.length} 个文件到 ${dir}`, 'ok');
}

async function saveOne(it) {
  const p = await pickSave(`${baseName(it.name)}.md`);
  if (!p) return;
  await writeText(p, it.md);
  log(`✓ 已保存 ${p}`, 'ok');
}

// File System Access API 封装（Chromium 内核可用）
async function pickDir() {
  if (window.showDirectoryPicker) {
    try { return await (await window.showDirectoryPicker()).handle; } catch { return null; }
  }
  return null;
}
function pickSave(name) {
  if (window.showSaveFilePicker) {
    return window.showSaveFilePicker({ suggestedName: name })
      .then(h => h.createWritable()).then(async w => {
        await w.write('.'); // 占位，实际由调用方重写
        await w.close(); return name;
      }).catch(() => null);
  }
  return Promise.resolve(null);
}
async function mkDir() { /* 由宿主 API 实现 */ }
async function writeText() { /* 由宿主 API 实现 */ }

// ---------- 拖放 ----------
['dragenter','dragover'].forEach(ev =>
  el.drop.addEventListener(ev, e => { e.preventDefault(); el.drop.classList.add('hot'); }));
['dragleave','drop'].forEach(ev =>
  el.drop.addEventListener(ev, e => { e.preventDefault(); el.drop.classList.remove('hot'); }));

el.drop.addEventListener('drop', e => {
  e.preventDefault();
  const fs = [...(e.dataTransfer?.files || [])];
  if (fs.length) add(fs);
});
el.drop.addEventListener('click', () => el.file.click());
el.pick.addEventListener('click', e => { e.stopPropagation(); el.file.click(); });
el.file.addEventListener('change', e => { add([...e.target.files]); e.target.value = ''; });

el.run.onclick = run;
el.clear.onclick = () => { queue = []; render(); el.rt.textContent = ''; };
el.save.onclick = saveAll;

boot();