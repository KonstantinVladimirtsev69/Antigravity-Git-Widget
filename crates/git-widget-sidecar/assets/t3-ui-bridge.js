// Client-side UI bridge for T3 Code (Desktop + Tailscale Mobile Web).
// Integrates native Git button next to [data-right-panel-toggle], docks drawer into
// [data-right-panel-surface-content] (below Windows titleBarOverlay), syncs active thread
// workspace via React Fiber + network observers, and routes "💬 AI" to T3 Code composer.
(function initGitWidgetT3Bridge() {
  if (window.__gwT3BridgeMounted) return;
  window.__gwT3BridgeMounted = true;

  const BASE_URL = window.__GW_T3_BASE_URL || '/git-widget';
  const IFRAME_ORIGIN = new URL(BASE_URL, window.location.origin).origin;
  const threadWorkspaces = new Map();
  let lastSentWorkspace = undefined;
  let drawerOpen = false;
  let iframeEl = null;
  let drawerEl = null;
  let fallbackBtn = null;

  const GIT_ICON_SVG =
    '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="2.5"/><circle cx="6" cy="18" r="2.5"/><circle cx="18" cy="8" r="2.5"/><path d="M6 8.5v7"/><path d="M18 10.5c0 4-6 3.5-12 5"/></svg>';

  const CSS = `
    #gw-t3-native-btn {
      display: inline-flex; align-items: center; justify-content: center; gap: 4px;
      height: 28px; min-width: 28px; padding: 0 7px; border-radius: 6px; border: 1px solid transparent;
      background: transparent; color: var(--muted-foreground, var(--foreground, #a1a1aa));
      font-size: 12px; font-weight: 500; cursor: pointer; -webkit-app-region: no-drag;
      transition: background 120ms, color 120ms;
    }
    #gw-t3-native-btn:hover { background: var(--accent, rgba(255,255,255,0.08)); color: var(--foreground, #f4f4f5); }
    #gw-t3-native-btn.active { background: var(--accent, rgba(255,255,255,0.14)); color: var(--foreground, #fff); border-color: var(--border, rgba(255,255,255,0.16)); }
    #gw-t3-fallback-btn {
      position: fixed; bottom: 14px; right: 16px; z-index: 9998; height: 30px; padding: 0 11px;
      display: inline-flex; align-items: center; gap: 5px; border-radius: 8px;
      border: 1px solid var(--border, rgba(128,128,128,0.4));
      background: var(--card, rgba(24,24,27,0.92)); color: var(--foreground, #f4f4f5);
      font-size: 12px; font-weight: 600; cursor: pointer; -webkit-app-region: no-drag;
    }
    #gw-t3-drawer {
      position: fixed; top: 40px; right: 0; width: min(440px, 92vw); height: calc(100vh - 40px);
      z-index: 9999; display: none; flex-direction: column; background: var(--background, #131316);
      border-left: 1px solid var(--border, rgba(128,128,128,0.35));
      border-top: 1px solid var(--border, rgba(128,128,128,0.2));
      box-shadow: -8px 0 28px rgba(0,0,0,0.45); -webkit-app-region: no-drag;
    }
    #gw-t3-drawer.open { display: flex; }
    #gw-t3-drawer-hdr {
      display: flex; align-items: center; justify-content: space-between; padding: 6px 10px;
      border-bottom: 1px solid var(--border, rgba(128,128,128,0.25));
      font-size: 12px; font-weight: 600; color: var(--foreground, #f4f4f5);
    }
    #gw-t3-drawer-close {
      background: transparent; border: none; color: inherit; font-size: 16px; cursor: pointer; padding: 2px 6px; border-radius: 4px;
    }
    #gw-t3-drawer-close:hover { background: var(--accent, rgba(255,255,255,0.1)); }
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

  function positionDrawer() {
    if (!drawerEl || !drawerOpen) return;
    const panel = document.querySelector('[data-right-panel-surface-content]');
    if (panel) {
      const r = panel.getBoundingClientRect();
      if (r.width >= 260 && r.height >= 240) {
        const top = Math.max(40, Math.round(r.top));
        Object.assign(drawerEl.style, {
          top: `${top}px`, left: `${Math.round(r.left)}px`, right: 'auto',
          width: `${Math.round(r.width)}px`, height: `${Math.max(240, Math.round(window.innerHeight - top))}px`,
        });
        return;
      }
    }
    Object.assign(drawerEl.style, { top: '40px', left: 'auto', right: '0', width: 'min(440px, 92vw)', height: 'calc(100vh - 40px)' });
  }

  function ensureNativeButton() {
    const toggle = document.querySelector('[data-right-panel-toggle]');
    let btn = document.getElementById('gw-t3-native-btn');
    if (toggle && toggle.parentElement) {
      if (!btn) {
        btn = document.createElement('button');
        btn.type = 'button';
        btn.id = 'gw-t3-native-btn';
        btn.title = 'Git Widget (Alt+G)';
        btn.innerHTML = `${GIT_ICON_SVG}<span>Git</span>`;
        btn.addEventListener('click', (e) => { e.stopPropagation(); setDrawerOpen(!drawerOpen); });
      }
      if (btn.parentElement !== toggle.parentElement) {
        toggle.parentElement.insertBefore(btn, toggle);
      }
      btn.classList.toggle('active', drawerOpen);
      if (fallbackBtn) fallbackBtn.style.display = 'none';
    } else if (fallbackBtn) {
      fallbackBtn.style.display = 'inline-flex';
    }
  }

  function findComposer() {
    const selectors = [
      '[data-testid*="composer"] [contenteditable="true"]',
      '[contenteditable="true"][role="textbox"]',
      'textarea[placeholder*="Ask"], textarea[placeholder*="Message"], textarea[placeholder*="Спросите"]',
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
    ensureNativeButton();
    if (open) {
      positionDrawer();
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
    fallbackBtn.type = 'button';
    fallbackBtn.id = 'gw-t3-fallback-btn';
    fallbackBtn.innerHTML = `${GIT_ICON_SVG}<span>Git</span>`;
    fallbackBtn.title = 'Git Widget (Alt+G)';
    fallbackBtn.addEventListener('click', () => setDrawerOpen(!drawerOpen));

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
    closeBtn.title = 'Закрыть Git Widget (Esc)';
    closeBtn.addEventListener('click', () => setDrawerOpen(false));
    hdr.append(title, closeBtn);

    iframeEl = document.createElement('iframe');
    iframeEl.title = 'Git Widget';
    iframeEl.addEventListener('load', () => { postTheme(); syncWorkspace(true); });
    drawerEl.append(hdr, iframeEl);
    document.body.append(fallbackBtn, drawerEl);

    ensureNativeButton();
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
      if (drawerOpen) { positionDrawer(); syncWorkspace(false); }
    }, 800);
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', mountUi);
  else mountUi();
})();
