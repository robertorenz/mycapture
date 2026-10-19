// Shared helpers for every myCapture window.
const TAURI = window.__TAURI__;
const invoke = (cmd, args, opts) => TAURI.core.invoke(cmd, args, opts);
const listen = (ev, cb) => TAURI.event.listen(ev, cb);
const currentWindow = () => TAURI.window.getCurrentWindow();

// ---------- language ----------
// English text is the key. Settings stores the choice here too, so every window can translate
// before it paints; "auto" follows the system language.
const LANG = (() => {
  let pref = 'auto';
  try { pref = localStorage.getItem('mycapture.lang') || 'auto'; } catch {}
  const l = pref === 'auto' ? (navigator.language || 'en').slice(0, 2).toLowerCase() : pref;
  return l === 'es' ? 'es' : 'en';
})();
document.documentElement.lang = LANG;
const STRINGS_ES = {
  'OK': 'Aceptar',
  'Cancel': 'Cancelar',
  'Delete': 'Eliminar',
  'Close': 'Cerrar',
  'Share': 'Compartir',
  'Share link': 'Compartir enlace',
  'Share to…': 'Compartir en…',
  'Upload failed': 'No se pudo subir',
  'Upload to {0} and copy the link': 'Subir a {0} y copiar el enlace',
  'Choose where to share': 'Elige dónde compartir',
  'Default for the Share button': 'Predeterminado del botón Compartir',
  'Make this the default': 'Usar como predeterminado',
  'No sharing destination is set up yet.': 'Todavía no hay ningún destino para compartir.',
  'Set up sharing…': 'Configurar uso compartido…',
  'Link copied to the clipboard': 'Enlace copiado al portapapeles',
  'Dropbox': 'Dropbox',
  'OneDrive': 'OneDrive',
  'Google Drive': 'Google Drive',
  'myCapture server': 'Servidor myCapture',
  'S3 storage': 'Almacenamiento S3',
  'Custom upload': 'Subida personalizada',
};
/** Add a page's own Spanish strings. */
function addStrings(map) { Object.assign(STRINGS_ES, map); }
/** Translate English UI text; `{0}`, `{1}`… are filled from `args`. */
function tr(s, ...args) {
  let out = (LANG === 'es' && STRINGS_ES[s]) || s;
  args.forEach((a, i) => { out = out.split(`{${i}}`).join(a); });
  return out;
}
/** Translate the static text, titles and placeholders already in the page. */
function translatePage(root = document.body) {
  if (LANG === 'en') return;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: n => n.parentElement?.closest('script, style, textarea, [data-no-tr]') ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT,
  });
  for (let n; (n = walker.nextNode());) {
    const key = n.nodeValue.trim();
    if (key && STRINGS_ES[key]) n.nodeValue = n.nodeValue.replace(key, STRINGS_ES[key]);
  }
  root.querySelectorAll('[title], [placeholder], [aria-label]').forEach(el => {
    for (const a of ['title', 'placeholder', 'aria-label']) {
      const v = el.getAttribute(a);
      if (v && STRINGS_ES[v]) el.setAttribute(a, STRINGS_ES[v]);
    }
  });
  // Text mixed with links or code: the element's whole (trusted, built-in) HTML is translated by key.
  root.querySelectorAll('[data-i18n]').forEach(el => { const v = STRINGS_ES[el.dataset.i18n]; if (v) el.innerHTML = v; });
  if (STRINGS_ES[document.title]) document.title = STRINGS_ES[document.title];
}
// Windows that are safe to rebuild switch language as soon as it changes in Settings.
if (/\/(moon|history)\.html$/.test(location.pathname)) listen('language-changed', () => location.reload());

const CAP_BASE = /Windows/.test(navigator.userAgent) ? 'http://mycap.localhost' : 'mycap://localhost';
const fileUrl = (name, bust = true) => `${CAP_BASE}/file/${encodeURIComponent(name)}${bust ? `?t=${Date.now()}` : ''}`;

const ICONS = {
  camera: '<path d="M4 8h3l2-3h6l2 3h3v11H4z"/><circle cx="12" cy="13" r="3.5"/>',
  video: '<rect x="3" y="6" width="13" height="12" rx="2"/><path d="M16 10l5-3v10l-5-3z"/>',
  crosshair: '<circle cx="12" cy="12" r="8"/><path d="M12 2v5M12 17v5M2 12h5M17 12h5"/>',
  history: '<path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5M12 7v5l3 2"/>',
  more: '<circle cx="5" cy="12" r="1.3"/><circle cx="12" cy="12" r="1.3"/><circle cx="19" cy="12" r="1.3"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1"/>',
  webcam: '<circle cx="12" cy="10" r="7"/><circle cx="12" cy="10" r="2.5"/><path d="M8 21h8M12 17v4"/>',
  eyeoff: '<path d="M3 3l18 18M10.6 10.6a2 2 0 0 0 2.8 2.8M9.9 5.1A10 10 0 0 1 12 5c6 0 9.5 7 9.5 7a17 17 0 0 1-3.2 4M6.6 6.6C3.9 8.4 2.5 12 2.5 12S6 19 12 19a9.6 9.6 0 0 0 5.4-1.6"/>',
  power: '<path d="M12 3v9M6.3 6.3a8 8 0 1 0 11.4 0"/>',
  back: '<path d="M15 18l-6-6 6-6"/>',
  copy: '<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V6a2 2 0 0 1 2-2h9"/>',
  link: '<path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1"/><path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"/>',
  file: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>',
  save: '<path d="M12 3v12M7 10l5 5 5-5M5 21h14"/>',
  trash: '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/>',
  folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  x: '<path d="M6 6l12 12M18 6L6 18"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  alert: '<circle cx="12" cy="12" r="9"/><path d="M12 7.5v5.5M12 16.5v.1"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5.5M12 7.5v.1"/>',
  pause: '<rect x="6.5" y="5" width="3.5" height="14" rx="1"/><rect x="14" y="5" width="3.5" height="14" rx="1"/>',
  play: '<path d="M7 5l12 7-12 7z"/>',
  stop: '<rect x="6" y="6" width="12" height="12" rx="2"/>',
  redo: '<path d="M4 12a8 8 0 0 1 14-5.3L20 9"/><path d="M20 4v5h-5"/>',
  undo: '<path d="M20 12a8 8 0 0 0-14-5.3L4 9"/><path d="M4 4v5h5"/>',
  arrow: '<path d="M5 19L19 5M10 5h9v9"/>',
  rect: '<rect x="4" y="5" width="16" height="14" rx="1.5"/>',
  ellipse: '<ellipse cx="12" cy="12" rx="9" ry="7"/>',
  pen: '<path d="M4 20c4-1 5-7 8-9s5 1 8-6"/>',
  highlight: '<path d="M4 20h7M14 4l6 6-9 9H5v-6z"/>',
  text: '<path d="M5 6V4h14v2M12 4v16M9 20h6"/>',
  blur: '<rect x="4" y="4" width="5" height="5"/><rect x="15" y="4" width="5" height="5"/><rect x="9.5" y="9.5" width="5" height="5"/><rect x="4" y="15" width="5" height="5"/><rect x="15" y="15" width="5" height="5"/>',
  pointer: '<path d="M6 3l13 8.5-5.6 1.4L16.6 20l-2.6 1.2-3.2-7.1L6 18z"/>',
  callout: '<path d="M4 5h16v10h-9l-4 4v-4H4z"/><path d="M8 9h8M8 12h5"/>',
  step: '<circle cx="12" cy="12" r="8"/><path d="M11 9l2-1v8"/>',
  mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/>',
  speaker: '<path d="M4 9h4l5-4v14l-5-4H4z"/><path d="M16.5 8.5a5 5 0 0 1 0 7M19 6a8.5 8.5 0 0 1 0 12"/>',
  grip: '<circle cx="9" cy="7" r="1"/><circle cx="15" cy="7" r="1"/><circle cx="9" cy="12" r="1"/><circle cx="15" cy="12" r="1"/><circle cx="9" cy="17" r="1"/><circle cx="15" cy="17" r="1"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  minus: '<path d="M5 12h14"/>',
  shape: '<rect x="4" y="4" width="16" height="16" rx="5"/>',
  share: '<path d="M4 12v7a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-7M12 3v12M7 8l5-5 5 5"/>',
  image: '<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="10" r="2"/><path d="M21 16l-5-5-9 9"/>',
  chevron: '<path d="M7 10l5 5 5-5"/>',
  scroll: '<rect x="6" y="3" width="12" height="18" rx="2"/><path d="M12 7v7M9 11l3 3 3-3"/>',
  star: '<path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z"/>',
  cloud: '<path d="M7 18a4.5 4.5 0 0 1-.6-9 6 6 0 0 1 11.4 1.6A3.8 3.8 0 0 1 17.5 18z"/>',
};
const icon = (name, cls = 'i') => `<svg class="${cls}" viewBox="0 0 24 24">${ICONS[name] || ''}</svg>`;
function hydrateIcons(root = document) {
  root.querySelectorAll('[data-icon]').forEach(el => {
    el.insertAdjacentHTML('afterbegin', icon(el.dataset.icon));
    el.removeAttribute('data-icon');
  });
}

const ESC_MAP = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ESC_MAP[c]);

// Modal popups (never window.alert).
const Modal = {
  open({ title, body, kind = 'info', buttons }) {
    return new Promise(resolve => {
      const back = document.createElement('div');
      back.className = 'modal-back';
      const badge = { err: 'alert', info: 'info', ok: 'check' }[kind] || 'info';
      back.innerHTML = `<div class="modal" role="dialog" aria-modal="true">
        <h3><span class="badge ${kind}">${icon(badge)}</span>${esc(title)}</h3>
        <div class="body">${esc(body)}</div>
        <div class="actions"></div></div>`;
      const actions = back.querySelector('.actions');
      const close = v => { back.remove(); document.removeEventListener('keydown', onKey, true); resolve(v); };
      buttons.forEach(b => {
        const el = document.createElement('button');
        el.className = `btn ${b.cls || ''}`;
        el.textContent = b.label;
        el.onclick = () => close(b.value);
        actions.appendChild(el);
      });
      const onKey = e => {
        if (e.key === 'Escape') { e.stopPropagation(); close(buttons[0].value); }
        if (e.key === 'Enter') { e.stopPropagation(); close(buttons[buttons.length - 1].value); }
      };
      document.addEventListener('keydown', onKey, true);
      back.addEventListener('mousedown', e => { if (e.target === back) close(buttons[0].value); });
      document.body.appendChild(back);
      actions.lastElementChild.focus();
    });
  },
  alert(title, body, kind = 'info') {
    return this.open({ title, body, kind, buttons: [{ label: tr('OK'), cls: 'primary', value: true }] });
  },
  error(title, body) { return this.alert(title, String(body), 'err'); },
  confirm(title, body, okLabel = tr('OK'), danger = false) {
    return this.open({ title, body, kind: danger ? 'err' : 'info', buttons: [{ label: tr('Cancel'), value: false }, { label: okLabel, cls: 'primary', value: true }] });
  },
};

function toast(msg, kind = 'ok') {
  let host = document.querySelector('.toasts');
  if (!host) { host = document.createElement('div'); host.className = 'toasts'; document.body.appendChild(host); }
  const t = document.createElement('div');
  t.className = `toast ${kind}`;
  t.innerHTML = `${icon(kind === 'ok' ? 'check' : 'alert')}<span>${esc(msg)}</span>`;
  host.appendChild(t);
  setTimeout(() => { t.style.transition = 'opacity .3s'; t.style.opacity = '0'; setTimeout(() => t.remove(), 300); }, 2200);
}

function fmtDuration(sec) {
  sec = Math.max(0, Math.floor(sec));
  return `${Math.floor(sec / 60)}:${String(sec % 60).padStart(2, '0')}`;
}

document.addEventListener('contextmenu', e => {
  if (!e.target.closest('input, textarea, [contenteditable]')) e.preventDefault();
});

// Split "Share" button: the main half uploads to the default destination, the arrow lists every
// destination that is set up (with a star to change the default) and a shortcut to Settings.
const DEST_ICON = { dropbox: 'cloud', onedrive: 'cloud', gdrive: 'cloud', server: 'link', s3: 'folder', http: 'link' };
function closeShareMenus() { document.querySelectorAll('.menu').forEach(m => m.remove()); }
function shareButton({ id, label = tr('Share link'), showDest = false, onShared, onError = err => Modal.error(tr('Upload failed'), err) }) {
  const wrap = document.createElement('div');
  wrap.className = 'split';
  wrap.innerHTML = `<button class="btn split-main">${icon('share')}<span>${esc(label)}</span><small class="split-dest"></small></button>`
    + `<button class="btn split-more" title="${tr('Share to…')}" aria-haspopup="menu">${icon('chevron')}</button>`;
  const [main, more] = wrap.children;
  const targets = async () => (await invoke('share_targets')) || [];
  const busy = on => { main.disabled = more.disabled = on; };
  async function refresh(list) {
    const d = (list || await targets()).find(t => t.default);
    main.title = d ? tr('Upload to {0} and copy the link', tr(d.label)) : tr('Choose where to share');
    if (showDest) wrap.querySelector('.split-dest').textContent = d ? `· ${tr(d.label)}` : '';
  }
  async function go(provider) {
    closeShareMenus();
    busy(true);
    try { onShared?.(await invoke('share', { id: typeof id === 'function' ? id() : id, provider }), provider); }
    catch (err) { onError(err); }
    finally { busy(false); }
  }
  function openMenu(list) {
    closeShareMenus();
    const m = document.createElement('div');
    m.className = 'menu';
    m.setAttribute('role', 'menu');
    m.innerHTML = (list.length
      ? list.map(t => `<div class="menu-row">
          <button class="menu-item" role="menuitem" data-p="${esc(t.id)}">${icon(DEST_ICON[t.id] || 'share')}<span class="mi-text"><b>${esc(tr(t.label))}</b>${t.detail ? `<small>${esc(t.detail)}</small>` : ''}</span></button>
          <button class="menu-star${t.default ? ' on' : ''}" data-star="${esc(t.id)}" title="${t.default ? tr('Default for the Share button') : tr('Make this the default')}">${icon('star')}</button>
        </div>`).join('')
      : `<div class="menu-empty">${tr('No sharing destination is set up yet.')}</div>`)
      + `<div class="menu-sep"></div><button class="menu-item" role="menuitem" data-setup>${icon('settings')}<span class="mi-text"><b>${tr('Set up sharing…')}</b></span></button>`;
    document.body.appendChild(m);
    const r = wrap.getBoundingClientRect(), mr = m.getBoundingClientRect();
    const top = r.bottom + 6 + mr.height <= innerHeight - 8 ? r.bottom + 6 : Math.max(8, r.top - 6 - mr.height);
    m.style.top = `${top}px`;
    m.style.left = `${Math.max(8, Math.min(r.right - mr.width, innerWidth - mr.width - 8))}px`;
    m.addEventListener('click', async e => {
      const star = e.target.closest('[data-star]'), item = e.target.closest('.menu-item');
      if (star) {
        await invoke('set_share_default', { provider: star.dataset.star });
        const fresh = await targets();
        refresh(fresh);
        openMenu(fresh);
      } else if (item?.dataset.p) go(item.dataset.p);
      else if (item && 'setup' in item.dataset) { closeShareMenus(); invoke('open_settings', { section: 'sharing' }); }
    });
    m.querySelector('.menu-item')?.focus();
  }
  main.onclick = async () => { const list = await targets(); list.some(t => t.default) ? go(null) : openMenu(list); };
  more.onclick = async e => {
    e.stopPropagation();
    if (document.querySelector('.menu')) return closeShareMenus();
    openMenu(await targets());
  };
  wrap.addEventListener('mouseenter', () => refresh(), { once: !showDest });
  if (showDest) refresh();
  return wrap;
}
addEventListener('mousedown', e => { if (!e.target.closest('.menu, .split-more')) closeShareMenus(); });
addEventListener('keydown', e => { if (e.key === 'Escape' && document.querySelector('.menu')) { e.stopPropagation(); closeShareMenus(); } }, true);
addEventListener('blur', closeShareMenus);
addEventListener('resize', closeShareMenus);
