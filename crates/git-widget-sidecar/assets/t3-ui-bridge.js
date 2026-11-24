// Client-side UI bridge for T3 Code (Desktop + Tailscale Mobile Web).
// Native Git button, right-panel menu item, resizable docked/push drawer,
// React Fiber workspace sync, and "💬 AI" composer bridge.
(function initGitWidgetT3Bridge() {
  if (window.__gwT3BridgeMounted) return;
  window.__gwT3BridgeMounted = true;

  const BASE_URL = window.__GW_T3_BASE_URL || '/git-widget';
  const IFRAME_ORIGIN = new URL(BASE_URL, window.location.origin).origin;
  const WIDTH_KEY = 'gw.t3DrawerWidth';
  const CSS = __GW_BRIDGE_CSS__;
  const GIT_ICON_SVG =
    '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="2.5"/><circle cx="6" cy="18" r="2.5"/><circle cx="18" cy="8" r="2.5"/><path d="M6 8.5v7"/><path d="M18 10.5c0 4-6 3.5-12 5"/></svg>';

  const threadWorkspaces = new Map();
  let lastSentWorkspace = undefined;
  let drawerOpen = false;
  let suppressToggleClose = false;
  let drawerWidth = (() => {
    try {
      const saved = Number(localStorage.getItem(WIDTH_KEY));
      if (saved >= 300 && saved <= 900) return saved;
    } catch (_) {}
    return 420;
  })();
  let iframeEl = null;
  let drawerEl = null;
  let fallbackBtn = null;

  function applyWidthVar(w) {
    const clamped = Math.max(300, Math.min(Math.min(900, window.innerWidth - 320), Math.round(w)));
    drawerWidth = clamped;
    document.documentElement.style.setProperty('--gw-drawer-w', `${clamped}px`);
    return clamped;
  }

  function showToast(text) {
    const el = document.createElement('div');
    el.className = 'gw-t3-toast';
    el.textContent = text;
    document.body.appendChild(el);
    setTimeout(() => el.remove(), 3200);
  }

  function collectThemeVars() {
    const cs = getComputedStyle(document.documentElement);
    const vars = {};
    for (const k of ['--background', '--foreground', '--border', '--card', '--muted', '--accent']) {
      const v = cs.getPropertyValue(k).trim();
      if (v) vars[k] = v;
    }
    if (!vars['--background']) {
      const isDark = document.documentElement.classList.contains('dark');
      vars['--background'] = isDark ? '#131316' : '#ffffff';
      vars['--foreground'] = isDark ? '#e4e4e7' : '#18181b';
    }
    return vars;
  }

  function postTheme() {
    if (!iframeEl || !iframeEl.contentWindow) return;
    try {
      iframeEl.contentWindow.postMessage(
        { type: 'theme-change', payload: { cssVariables: collectThemeVars() } },
        IFRAME_ORIGIN
      );
    } catch (_) {}
  }

  function looksLikeAbsPath(v) {
    return typeof v === 'string' && v.length > 2 && (/^[A-Za-z]:[\\/]/.test(v) || v.startsWith('/'));
  }

  function findWorkspaceInFiber(rootEl) {
    if (!rootEl) return null;
    const key = Object.keys(rootEl).find((k) => k.startsWith('__reactFiber$'));
    if (!key) return null;
    const queue = [rootEl[key]];
    let visited = 0, fallbackRoot = null;
    while (queue.length > 0 && visited < 900) {
      const f = queue.shift();
      if (!f) continue;
      visited++;
      const p = f.memoizedProps;
      if (p && typeof p === 'object') {
        if (looksLikeAbsPath(p.worktreePath)) return p.worktreePath;
        if (p.thread && looksLikeAbsPath(p.thread.worktreePath)) return p.thread.worktreePath;
        if (looksLikeAbsPath(p.workspaceRoot)) fallbackRoot = fallbackRoot || p.workspaceRoot;
        if (p.project && looksLikeAbsPath(p.project.workspaceRoot)) fallbackRoot = fallbackRoot || p.project.workspaceRoot;
      }
      if (f.child) queue.push(f.child);
      if (f.sibling) queue.push(f.sibling);
    }
    return fallbackRoot;
  }

  function resolveActiveWorkspace() {
    const conv = document.querySelector('[data-workspace-area="conversation"]') || document.getElementById('root');
    const fromFiber = findWorkspaceInFiber(conv) || findWorkspaceInFiber(document.getElementById('root'));
    if (fromFiber) return fromFiber;
    const m = (window.location.pathname + window.location.hash).match(/\/threads?\/([^/?#]+)/i);
    if (m && m[1] && threadWorkspaces.has(decodeURIComponent(m[1]))) return threadWorkspaces.get(decodeURIComponent(m[1]));
    const attrEl = document.querySelector('[data-workspace-root], [data-cwd]');
    return attrEl ? attrEl.getAttribute('data-workspace-root') || attrEl.getAttribute('data-cwd') : null;
  }

  function syncWorkspace(force) {
    if (!iframeEl || !iframeEl.contentWindow) return;
    const ws = resolveActiveWorkspace();
    if (!force && ws === lastSentWorkspace) return;
    lastSentWorkspace = ws;
    try { iframeEl.contentWindow.postMessage({ type: 'workspace-change', payload: { workspaceUris: ws ? [ws] : [] } }, IFRAME_ORIGIN); } catch (_) {}
  }

  function isT3RightPanelOpen() {
    const surface = document.querySelector('[data-right-panel-surface-content]');
    if (surface && surface.getBoundingClientRect().width > 80) return true;
    const toggle = document.querySelector('[data-right-panel-toggle]');
    if (toggle && (toggle.getAttribute('aria-pressed') === 'true' || toggle.getAttribute('data-state') === 'open')) {
      return true;
    }
    return Boolean(findOpenPanelAnchor());
  }

  function closeT3RightPanelIfOpen() {
    if (!isT3RightPanelOpen()) return;
    const toggle = document.querySelector('[data-right-panel-toggle]');
    if (!toggle) return;
    suppressToggleClose = true;
    try { toggle.click(); } catch (_) {}
    setTimeout(() => { suppressToggleClose = false; }, 120);
  }

  function positionDrawer() {
    if (!drawerEl || !drawerOpen) {
      document.documentElement.classList.remove('gw-t3-pushed');
      return;
    }
    const w = applyWidthVar(drawerWidth);
    const canPush = window.innerWidth >= 960;
    document.documentElement.classList.toggle('gw-t3-pushed', canPush);
    Object.assign(drawerEl.style, {
      top: '40px', left: 'auto', right: '0',
      width: canPush ? `${w}px` : `min(${w}px, 92vw)`, height: 'calc(100vh - 40px)',
    });
  }

  function bindResizer(resizerEl) {
    resizerEl.addEventListener('mousedown', (ev) => {
      ev.preventDefault();
      resizerEl.classList.add('dragging');
      document.documentElement.classList.add('gw-t3-resizing');
      if (iframeEl) iframeEl.style.pointerEvents = 'none';

      const onMove = (m) => {
        const w = applyWidthVar(window.innerWidth - m.clientX);
        const canPush = window.innerWidth >= 960;
        document.documentElement.classList.toggle('gw-t3-pushed', canPush);
        Object.assign(drawerEl.style, { left: 'auto', right: '0', width: `${w}px` });
      };
      const onUp = () => {
        resizerEl.classList.remove('dragging');
        document.documentElement.classList.remove('gw-t3-resizing');
        if (iframeEl) iframeEl.style.pointerEvents = '';
        try { localStorage.setItem(WIDTH_KEY, String(drawerWidth)); } catch (_) {}
        window.removeEventListener('mousemove', onMove);
        window.removeEventListener('mouseup', onUp);
      };
      window.addEventListener('mousemove', onMove);
      window.addEventListener('mouseup', onUp);
    });
  }

  function ensureNativeButton() {
    const toggle = document.querySelector('[data-right-panel-toggle]');
    let btn = document.getElementById('gw-t3-native-btn');
    if (toggle && toggle.parentElement) {
      if (!toggle.dataset.gwBound) {
        toggle.dataset.gwBound = '1';
        toggle.addEventListener('click', () => {
          if (!suppressToggleClose && drawerOpen) setDrawerOpen(false);
        }, true);
      }
      if (!btn) {
        btn = document.createElement('button');
        btn.type = 'button';
        btn.id = 'gw-t3-native-btn';
        btn.title = 'Git Widget (Alt+G)';
        btn.innerHTML = `${GIT_ICON_SVG}<span>Git</span>`;
        btn.addEventListener('click', (e) => { e.stopPropagation(); setDrawerOpen(!drawerOpen); });
      }
      if (btn.parentElement !== toggle.parentElement) toggle.parentElement.insertBefore(btn, toggle);
      btn.classList.toggle('active', drawerOpen);
      if (fallbackBtn) fallbackBtn.style.display = 'none';
    } else if (fallbackBtn) {
      fallbackBtn.style.display = 'inline-flex';
    }
  }

  function findOpenPanelAnchor() {
    const buttons = Array.from(document.querySelectorAll('button'));
    for (const b of buttons) {
      if (b.id === 'gw-t3-panel-item' || b.offsetParent === null) continue;
      const txt = (b.textContent || '').trim();
      if (!/^(Боковой чат|Side chat|Устройство|Device|Пул-реквест|Pull request)/i.test(txt)) continue;
      const parent = b.parentElement;
      if (!parent) continue;
      const siblings = Array.from(parent.querySelectorAll(':scope > button'));
      const hasTermOrDiff = siblings.some((s) => /(Терминал|Terminal|Дифф|Diff|Файлы|Files)/i.test(s.textContent || ''));
      if (siblings.length >= 3 && hasTermOrDiff) return b;
    }
    return null;
  }

  function ensurePanelMenuItem() {
    if (document.getElementById('gw-t3-panel-item')) return;
    const anchor = findOpenPanelAnchor();
    if (!anchor || !anchor.parentElement) return;
    const item = document.createElement('button');
    item.type = 'button';
    item.id = 'gw-t3-panel-item';
    item.className = anchor.className;
    item.innerHTML = `<span class="gw-t3-pi-left">${GIT_ICON_SVG}<span>Git Widget</span></span><span class="gw-t3-pi-kbd">Alt+G</span>`;
    item.addEventListener('click', (e) => { e.stopPropagation(); setDrawerOpen(true); });
    anchor.parentElement.appendChild(item);
  }

  function findComposer() {
    for (const sel of [
      '[data-testid*="composer"] [contenteditable="true"]',
      '[contenteditable="true"][role="textbox"]',
      'textarea[placeholder*="Ask"], textarea[placeholder*="Message"], textarea[placeholder*="Спросите"]',
      'form textarea:not([disabled])',
      'textarea:not([disabled])',
    ]) {
      const el = document.querySelector(sel);
      if (el && el.offsetParent !== null) return el;
    }
    return null;
  }

  function insertIntoComposer(prompt) {
    const el = findComposer();
    if (!el) return false;
    try {
      el.focus();
      if (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT') {
        const desc = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value');
        const nextVal = el.value ? `${el.value}\n\n${prompt}` : prompt;
        if (desc && desc.set) desc.set.call(el, nextVal); else el.value = nextVal;
        el.dispatchEvent(new Event('input', { bubbles: true }));
        return true;
      }
      if (el.isContentEditable) {
        if (!document.execCommand || !document.execCommand('insertText', false, prompt)) {
          el.textContent = el.textContent ? `${el.textContent}\n\n${prompt}` : prompt;
          el.dispatchEvent(new Event('input', { bubbles: true }));
        }
        return true;
      }
    } catch (_) {}
    return false;
  }

  window.addEventListener('message', (event) => {
    if (event.origin !== IFRAME_ORIGIN) return;
    const data = event.data;
    if (!data || data.type !== 'git-widget-ask-ai') return;
    const prompt = typeof data.prompt === 'string' ? data.prompt.trim() : '';
    if (!prompt) return;
    if (insertIntoComposer(prompt)) { showToast('Промпт вставлен в композер T3 Code'); return; }
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(prompt)
        .then(() => showToast('Композер недоступен — промпт скопирован в буфер обмена'))
        .catch(() => showToast('Не удалось вставить или скопировать промпт'));
    } else showToast('Композер недоступен');
  });

  function setDrawerOpen(open) {
    drawerOpen = open;
    if (open) closeT3RightPanelIfOpen();
    if (!drawerEl) return;
    drawerEl.classList.toggle('open', open);
    positionDrawer();
    ensureNativeButton();
    if (open) {
      if (!iframeEl.src) iframeEl.src = `${BASE_URL}/?host=t3code`;
      postTheme();
      syncWorkspace(true);
    }
  }

  function mountUi() {
    if (!document.body) return;
    const style = document.createElement('style');
    style.textContent = CSS;
    document.head.appendChild(style);

    fallbackBtn = document.createElement('button');
    fallbackBtn.type = 'button'; fallbackBtn.id = 'gw-t3-fallback-btn';
    fallbackBtn.innerHTML = `${GIT_ICON_SVG}<span>Git</span>`; fallbackBtn.title = 'Git Widget (Alt+G)';
    fallbackBtn.addEventListener('click', () => setDrawerOpen(!drawerOpen));

    drawerEl = document.createElement('aside'); drawerEl.id = 'gw-t3-drawer';
    const resizer = document.createElement('div'); resizer.id = 'gw-t3-resizer'; resizer.title = 'Изменить ширину';
    bindResizer(resizer);

    const hdr = document.createElement('div'); hdr.id = 'gw-t3-drawer-hdr';
    const title = document.createElement('span'); title.textContent = 'Git Widget';
    const closeBtn = document.createElement('button');
    closeBtn.type = 'button'; closeBtn.id = 'gw-t3-drawer-close';
    closeBtn.textContent = '×'; closeBtn.title = 'Закрыть Git Widget (Esc)';
    closeBtn.addEventListener('click', () => setDrawerOpen(false));
    hdr.append(title, closeBtn);

    iframeEl = document.createElement('iframe'); iframeEl.title = 'Git Widget';
    iframeEl.addEventListener('load', () => { postTheme(); syncWorkspace(true); });
    drawerEl.append(resizer, hdr, iframeEl); document.body.append(fallbackBtn, drawerEl);

    applyWidthVar(drawerWidth);
    ensureNativeButton();
    ensurePanelMenuItem();
    window.addEventListener('resize', positionDrawer);
    window.addEventListener('keydown', (e) => {
      if (e.altKey && (e.key === 'g' || e.key === 'G')) { e.preventDefault(); setDrawerOpen(!drawerOpen); }
      else if (e.key === 'Escape' && drawerOpen) setDrawerOpen(false);
    });
    new MutationObserver(() => postTheme()).observe(document.documentElement, {
      attributes: true, attributeFilter: ['class', 'style'],
    });
    setInterval(() => {
      ensureNativeButton();
      ensurePanelMenuItem();
      if (drawerOpen) { positionDrawer(); syncWorkspace(false); }
    }, 800);
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', mountUi);
  else mountUi();
})();
