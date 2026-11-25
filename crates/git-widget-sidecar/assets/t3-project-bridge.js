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
    const notifyWorkspace = () => {
      if (iframeEl && iframeEl.contentWindow) {
        try {
          iframeEl.contentWindow.postMessage(
            { type: 'workspace-change', payload: { workspaceUris: [path] } },
            IFRAME_ORIGIN
          );
        } catch (_) {}
      }
    };
    if (!path) return reply('error', 'Empty repository path');
    try {
      const res = await fetch('/api/projects', { credentials: 'same-origin' });
      if (res.status === 404 || res.status === 405) throw new Error('API_UNSUPPORTED');
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const snap = await res.json();
      const list = Array.isArray(snap.projects) ? snap.projects : [];
      const targetNorm = normWinPath(path);
      const existing = list.find((p) => normWinPath(p.workspaceRoot) === targetNorm);

      if (existing) {
        if (existing.archivedAt) {
          const mut = await fetch('/api/projects/mutate', {
            method: 'POST',
            credentials: 'same-origin',
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
        notifyWorkspace();
        showToast('Проект уже есть в T3 Code');
        return reply('already_exists', 'Project already registered in T3 Code');
      }

      const createRes = await fetch('/api/projects/mutate', {
        method: 'POST',
        credentials: 'same-origin',
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
          notifyWorkspace();
          showToast('Проект уже есть в T3 Code');
          return reply('already_exists', 'Project already registered in T3 Code');
        }
        if (createRes.status === 404 || createRes.status === 405 || createRes.status === 400) {
          throw new Error('API_UNSUPPORTED');
        }
        throw new Error(`HTTP ${createRes.status}`);
      }
      notifyWorkspace();
      showToast('Проект добавлен в T3 Code');
      return reply('added', 'Project added to T3 Code');
    } catch (_) {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(path).catch(() => {});
      }
      showToast('Путь скопирован — добавьте папку в T3 Code вручную');
      return reply('fallback', 'Path copied to clipboard — add project manually in T3 Code');
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
    if (insertIntoComposer(prompt)) { showToast('Промпт вставлен в композер T3 Code'); return; }
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(prompt)
        .then(() => showToast('Композер недоступен — промпт скопирован в буфер обмена'))
        .catch(() => showToast('Не удалось вставить или скопировать промпт'));
    } else showToast('Композер недоступен');
  });
