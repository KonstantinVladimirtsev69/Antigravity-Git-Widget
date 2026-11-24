// Client-side UI bridge for T3 Code (Desktop + Tailscale Mobile Web).
// Mounts Git drawer toggle + uniform side drawer, syncs active thread workspace & theme,
// and routes "💬 AI" prompts from Git Widget into T3 Code's chat composer.
(function initGitWidgetT3Bridge() {
  if (window.__gwT3BridgeMounted) return;
  window.__gwT3BridgeMounted = true;

  const BASE_URL = window.__GW_T3_BASE_URL || '/git-widget';
  const IFRAME_ORIGIN = new URL(BASE_URL, window.location.origin).origin;
  const threadWorkspaces = new Map();
  const projectWorkspaces = new Map();
  let lastSentWorkspace = undefined;
  let drawerOpen = false;
  let iframeEl = null;
  let drawerEl = null;

  const CSS = `
    #gw-t3-toggle-btn {
      position: fixed; bottom: 14px; right: 16px; z-index: 9998; height: 30px; padding: 0 12px;
      border-radius: 8px; border: 1px solid var(--border, rgba(128,128,128,0.4));
      background: var(--card, rgba(24,24,27,0.92)); color: var(--foreground, #f4f4f5);
      font-size: 12px; font-weight: 600; cursor: pointer; backdrop-filter: blur(6px);
      box-shadow: 0 4px 14px rgba(0,0,0,0.35); -webkit-app-region: no-drag;
    }
    #gw-t3-drawer {
      position: fixed; top: 0; right: 0; width: min(440px, 92vw); height: 100vh; z-index: 9999;
      display: flex; flex-direction: column; background: var(--background, #131316);
      border-left: 1px solid var(--border, rgba(128,128,128,0.35));
      box-shadow: -8px 0 28px rgba(0,0,0,0.45); transform: translateX(100%);
      transition: transform 160ms ease-out; -webkit-app-region: no-drag;
    }
    #gw-t3-drawer.open { transform: translateX(0); }
    #gw-t3-drawer-hdr {
      display: flex; align-items: center; justify-content: space-between; padding: 6px 10px;
      border-bottom: 1px solid var(--border, rgba(128,128,128,0.25));
      font-size: 12px; font-weight: 600; color: var(--foreground, #f4f4f5);
    }
    #gw-t3-drawer-close {
      background: transparent; border: none; color: inherit; font-size: 16px; cursor: pointer; padding: 0 4px;
    }
    #gw-t3-drawer iframe { flex: 1; width: 100%; border: none; background: transparent; }
    .gw-t3-toast {
      position: fixed; bottom: 16px; right: 16px; z-index: 10002; padding: 8px 12px;
      border-radius: 8px; background: var(--card, #18181b); color: var(--foreground, #f4f4f5);
      border: 1px solid var(--border, #3f3f46); font-size: 12px; box-shadow: 0 6px 20px rgba(0,0,0,0.35);
    }
  `;

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

  function extractThreadIdFromUrl() {
    const m = window.location.pathname.match(/\/threads?\/([^/?#]+)/i);
    if (m && m[1]) return decodeURIComponent(m[1]);
    const params = new URLSearchParams(window.location.search);
    return params.get('threadId') || params.get('thread') || null;
  }

  function indexPayload(obj, depth) {
    if (!obj || typeof obj !== 'object' || depth > 5) return;
    if (Array.isArray(obj)) {
      for (let i = 0; i < obj.length && i < 200; i++) indexPayload(obj[i], depth + 1);
      return;
    }
    const id = obj.id || obj.threadId;
    const projId = obj.projectId;
    const ws = obj.worktreePath || obj.workspaceRoot || obj.cwd || obj.repoPath;
    if (typeof ws === 'string' && ws.length > 2) {
      if (typeof id === 'string') threadWorkspaces.set(id, ws);
      if (typeof obj.id === 'string' && obj.workspaceRoot) projectWorkspaces.set(obj.id, obj.workspaceRoot);
    }
    if (typeof id === 'string' && typeof projId === 'string' && !threadWorkspaces.has(id)) {
      const pws = projectWorkspaces.get(projId);
      if (pws) threadWorkspaces.set(id, pws);
    }
    for (const k of Object.keys(obj)) {
      const v = obj[k];
      if (v && typeof v === 'object') indexPayload(v, depth + 1);
    }
  }

  function resolveActiveWorkspace() {
    const tid = extractThreadIdFromUrl();
    if (!tid) return null;
    if (threadWorkspaces.has(tid)) return threadWorkspaces.get(tid);
    const el = document.querySelector('[data-workspace-root], [data-cwd]');
    return el ? el.getAttribute('data-workspace-root') || el.getAttribute('data-cwd') : null;
  }

  function syncWorkspace(force) {
    if (!iframeEl || !iframeEl.contentWindow) return;
    const ws = resolveActiveWorkspace();
    if (!force && ws === lastSentWorkspace) return;
    lastSentWorkspace = ws;
    try {
      iframeEl.contentWindow.postMessage(
        { type: 'workspace-change', payload: { workspaceUris: ws ? [ws] : [] } },
        IFRAME_ORIGIN
      );
    } catch (_) {}
  }

  const origFetch = window.fetch;
  if (typeof origFetch === 'function') {
    window.fetch = function patchedFetch() {
      const p = origFetch.apply(this, arguments);
      p.then((res) => {
        try {
          if ((res.headers.get('content-type') || '').includes('application/json')) {
            res.clone().json().then((d) => { indexPayload(d, 0); syncWorkspace(false); }).catch(() => {});
          }
        } catch (_) {}
      }).catch(() => {});
      return p;
    };
  }

  const OrigWS = window.WebSocket;
  if (typeof OrigWS === 'function') {
    window.WebSocket = function PatchedWebSocket(url, protocols) {
      const ws = protocols !== undefined ? new OrigWS(url, protocols) : new OrigWS(url);
      ws.addEventListener('message', (ev) => {
        try {
          if (typeof ev.data === 'string' && ev.data.length < 2_000_000 && ev.data[0] === '{') {
            indexPayload(JSON.parse(ev.data), 0);
            syncWorkspace(false);
          }
        } catch (_) {}
      });
      return ws;
    };
    window.WebSocket.prototype = OrigWS.prototype;
    Object.assign(window.WebSocket, OrigWS);
  }

  function findComposer() {
    const selectors = [
      '[data-testid*="composer"] [contenteditable="true"]',
      '[contenteditable="true"][role="textbox"]',
      'textarea[placeholder*="Ask"], textarea[placeholder*="Message"], textarea[placeholder*="Prompt"]',
      'form textarea:not([disabled])',
      'textarea:not([disabled])',
    ];
    for (const sel of selectors) {
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
        if (desc && desc.set) desc.set.call(el, nextVal);
        else el.value = nextVal;
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
    if (insertIntoComposer(prompt)) {
      showToast('Промпт вставлен в композер T3 Code');
      return;
    }
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard
        .writeText(prompt)
        .then(() => showToast('Композер недоступен — промпт скопирован в буфер обмена'))
        .catch(() => showToast('Не удалось вставить или скопировать промпт'));
    } else {
      showToast('Композер недоступен');
    }
  });

  function setDrawerOpen(open) {
    drawerOpen = open;
    if (!drawerEl) return;
    drawerEl.classList.toggle('open', open);
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

    const toggleBtn = document.createElement('button');
    toggleBtn.type = 'button';
    toggleBtn.id = 'gw-t3-toggle-btn';
    toggleBtn.textContent = 'Git';
    toggleBtn.title = 'Git Widget (Alt+G)';
    toggleBtn.addEventListener('click', () => setDrawerOpen(!drawerOpen));

    drawerEl = document.createElement('aside');
    drawerEl.id = 'gw-t3-drawer';
    const hdr = document.createElement('div');
    hdr.id = 'gw-t3-drawer-hdr';
    const title = document.createElement('span');
    title.textContent = 'Git Widget';
    const closeBtn = document.createElement('button');
    closeBtn.type = 'button';
    closeBtn.id = 'gw-t3-drawer-close';
    closeBtn.textContent = '×';
    closeBtn.addEventListener('click', () => setDrawerOpen(false));
    hdr.append(title, closeBtn);

    iframeEl = document.createElement('iframe');
    iframeEl.title = 'Git Widget';
    iframeEl.addEventListener('load', () => { postTheme(); syncWorkspace(true); });
    drawerEl.append(hdr, iframeEl);
    document.body.append(toggleBtn, drawerEl);

    window.addEventListener('keydown', (e) => {
      if (e.altKey && (e.key === 'g' || e.key === 'G')) { e.preventDefault(); setDrawerOpen(!drawerOpen); }
      else if (e.key === 'Escape' && drawerOpen) setDrawerOpen(false);
    });
    new MutationObserver(() => postTheme()).observe(document.documentElement, {
      attributes: true, attributeFilter: ['class', 'style'],
    });
    setInterval(() => { if (drawerOpen) syncWorkspace(false); }, 1000);
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', mountUi);
  else mountUi();
})();
