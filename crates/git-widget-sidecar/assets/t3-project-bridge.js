  function newUuid() {
    if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
      return crypto.randomUUID();
    }
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
      const r = (Math.random() * 16) | 0;
      const v = c === 'x' ? r : (r & 0x3) | 0x8;
      return v.toString(16);
    });
  }

  function folderBasename(p) {
    return String(p || '').replace(/[\\/]+$/, '').split(/[\\/]/).pop() || String(p || '');
  }

  function normWinPath(p) {
    return String(p || '').replace(/\//g, '\\').replace(/\\+$/, '').toLowerCase();
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

  async function resolveT3HttpContext() {
    const bridge = window.desktopBridge;
    if (!bridge || typeof bridge.getLocalEnvironmentBootstraps !== 'function') {
      return { baseUrl: '', bearer: null, credentials: 'same-origin' };
    }
    let baseUrl = '';
    try {
      const list = bridge.getLocalEnvironmentBootstraps();
      if (Array.isArray(list)) {
        const entry =
          list.find((b) => b && b.id === 'primary' && typeof b.httpBaseUrl === 'string' && b.httpBaseUrl) ||
          list.find((b) => b && typeof b.httpBaseUrl === 'string' && b.httpBaseUrl);
        if (entry) baseUrl = entry.httpBaseUrl.replace(/\/+$/, '');
      }
    } catch (_) {}
    let bearer = null;
    if (baseUrl && typeof bridge.getLocalEnvironmentBearerToken === 'function') {
      try {
        const token = await bridge.getLocalEnvironmentBearerToken();
        if (typeof token === 'string' && token.trim()) bearer = token.trim();
      } catch (_) {}
    }
    return { baseUrl, bearer, credentials: baseUrl ? 'omit' : 'same-origin' };
  }

  function t3ApiFetch(ctx, apiPath, init) {
    const opts = init ? { ...init } : {};
    const headers = opts.headers ? { ...opts.headers } : {};
    if (ctx.bearer) headers.Authorization = `Bearer ${ctx.bearer}`;
    if (Object.keys(headers).length > 0) opts.headers = headers;
    opts.credentials = ctx.credentials;
    return fetch(`${ctx.baseUrl}${apiPath}`, opts);
  }

  async function handleAddProjectToT3(payload, origin) {
    const path = payload && typeof payload.path === 'string' ? payload.path.trim() : '';
    const requestId = (payload && payload.requestId) || '';
    const reply = (status, message) => {
      if (iframeEl && iframeEl.contentWindow) {
        iframeEl.contentWindow.postMessage(
          { type: 'git-widget-add-project-result', requestId, status, message, path },
          origin
        );
      }
    };
    if (!path) return reply('error', 'Empty repository path');
    try {
      const ctx = await resolveT3HttpContext();
      const res = await t3ApiFetch(ctx, '/api/projects');
      if (res.status === 404 || res.status === 405) throw new Error('API_UNSUPPORTED');
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const snap = await res.json();
      const list = Array.isArray(snap.projects) ? snap.projects : [];
      const targetNorm = normWinPath(path);
      const existing = list.find((p) => !p.deletedAt && normWinPath(p.workspaceRoot) === targetNorm);

      if (existing) {
        if (existing.archivedAt) {
          const mut = await t3ApiFetch(ctx, '/api/projects/mutate', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              type: 'project.update',
              commandId: newUuid(),
              projectId: existing.id,
              archived: false,
            }),
          });
          if (!mut.ok) {
            if (mut.status === 404 || mut.status === 405 || mut.status === 400) {
              throw new Error('API_UNSUPPORTED');
            }
            throw new Error(`HTTP ${mut.status}`);
          }
        }
        showToast('Project already exists in T3 Code');
        return reply('already_exists', 'Project already registered in T3 Code');
      }

      const createRes = await t3ApiFetch(ctx, '/api/projects/mutate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          type: 'project.create',
          commandId: newUuid(),
          projectId: newUuid(),
          title: folderBasename(path),
          workspaceRoot: path,
        }),
      });
      if (!createRes.ok) {
        const txt = await createRes.text().catch(() => '');
        if (txt.includes('ProjectWorkspaceConflictError')) {
          showToast('Project already exists in T3 Code');
          return reply('already_exists', 'Project already registered in T3 Code');
        }
        if (createRes.status === 404 || createRes.status === 405 || createRes.status === 400) {
          throw new Error('API_UNSUPPORTED');
        }
        throw new Error(`HTTP ${createRes.status}`);
      }
      showToast('Project added to T3 Code');
      return reply('added', 'Project added to T3 Code');
    } catch (err) {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(path).catch(() => {});
      }
      const reason = err && err.message && err.message !== 'API_UNSUPPORTED' ? ` (${err.message})` : '';
      showToast('Path copied to clipboard — add project manually in T3 Code');
      return reply('fallback', `Path copied to clipboard — add project manually in T3 Code${reason}`);
    }
  }

  window.addEventListener('message', (event) => {
    if (event.origin !== IFRAME_ORIGIN) return;
    if (iframeEl && iframeEl.contentWindow && event.source !== iframeEl.contentWindow) return;
    const data = event.data;
    if (!data || typeof data.type !== 'string') return;
    if (data.type === 'git-widget-add-project') {
      handleAddProjectToT3(data, event.origin);
      return;
    }
    if (data.type !== 'git-widget-ask-ai') return;
    const prompt = typeof data.prompt === 'string' ? data.prompt.trim() : '';
    if (!prompt) return;
    if (insertIntoComposer(prompt)) { showToast('Prompt inserted into T3 Code composer'); return; }
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(prompt)
        .then(() => showToast('Composer unavailable — prompt copied to clipboard'))
        .catch(() => showToast('Failed to insert or copy prompt'));
    } else showToast('Composer unavailable');
  });
