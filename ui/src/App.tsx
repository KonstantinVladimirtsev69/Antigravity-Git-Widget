import { useCallback, useEffect, useMemo, useState } from 'react';
import { CONVERSATION_ID, api } from './api';
import { DirtyModal, PreviewModal, SettingsPanel } from './Modals';
import type {
  ActionPreview,
  DirtyStrategy,
  GuardedOpKind,
  RepoInfo,
  RepoStatus,
  ScannerConfig,
} from './types';

export default function App() {
  const [repos, setRepos] = useState<RepoInfo[]>([]);
  const [config, setConfig] = useState<ScannerConfig>({ roots: ['C:\\Projects\\'], max_depth: 3 });
  const [selectedRepo, setSelectedRepo] = useState<string>('');
  const [repoFilter, setRepoFilter] = useState('');
  const [status, setStatus] = useState<RepoStatus | null>(null);
  const [mergeSource, setMergeSource] = useState<string>('');
  const [commitMsg, setCommitMsg] = useState('');
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string>('');
  const [error, setError] = useState<string>('');
  const [showSettings, setShowSettings] = useState(false);

  const [dirtyPrompt, setDirtyPrompt] = useState<{
    kind: GuardedOpKind;
    target: string | null;
    files: string[];
  } | null>(null);

  const [protectedPrompt, setProtectedPrompt] = useState<{
    action: 'merge' | 'push';
    preview: ActionPreview;
  } | null>(null);

  const loadStatus = useCallback(async (repoPath: string) => {
    if (!repoPath) return;
    try {
      const st = await api.getStatus(repoPath);
      setStatus(st);
      setMergeSource((prev) => {
        if (prev && st.local_branches.includes(prev) && prev !== st.branch) return prev;
        return st.local_branches.find((b) => b !== st.branch) ?? '';
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const loadRepos = useCallback(async () => {
    try {
      const res = await api.getRepos();
      setRepos(res.repos);
      setConfig(res.config);
      setSelectedRepo((prev) => prev || res.repos[0]?.path || '');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void loadRepos();
  }, [loadRepos]);

  useEffect(() => {
    if (selectedRepo) {
      void loadStatus(selectedRepo);
    }
  }, [selectedRepo, loadStatus]);

  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      if (event.data?.type === 'theme-change') {
        const vars = (event.data.payload?.cssVariables ?? {}) as Record<string, string>;
        for (const [k, v] of Object.entries(vars)) {
          document.documentElement.style.setProperty(k, v);
        }
      } else if (event.data?.type === 'workspace-change') {
        const uris = (event.data.payload?.workspaceUris ?? []) as string[];
        if (uris.length > 0) {
          void api.resolveWorkspace(uris).then((res) => {
            if (res.matched_path) setSelectedRepo(res.matched_path);
          });
        }
      }
    };
    window.addEventListener('message', onMessage);
    return () => window.removeEventListener('message', onMessage);
  }, []);

  const filteredRepos = useMemo(() => {
    const q = repoFilter.trim().toLowerCase();
    if (!q) return repos;
    return repos.filter((r) => r.name.toLowerCase().includes(q) || r.path.toLowerCase().includes(q));
  }, [repos, repoFilter]);

  const executeGuarded = async (
    kind: GuardedOpKind,
    target: string | null,
    strategy: DirtyStrategy,
  ) => {
    if (!selectedRepo) return;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      const res = await api.runGuarded(selectedRepo, kind, target, strategy);
      if (res.blocked_dirty) {
        setDirtyPrompt({ kind, target, files: res.dirty_files });
      } else {
        setDirtyPrompt(null);
        if (res.op_result) {
          setNotice(res.op_result.message || `${kind} выполнен`);
        }
        await loadStatus(selectedRepo);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleMergeClick = async () => {
    if (!selectedRepo || !mergeSource) return;
    setBusy(true);
    setError('');
    try {
      const preview = await api.previewMerge(selectedRepo, mergeSource);
      if (preview.is_protected) {
        setProtectedPrompt({ action: 'merge', preview });
      } else {
        await executeGuarded('merge', mergeSource, { mode: 'fail_if_dirty' });
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const handlePushClick = async () => {
    if (!selectedRepo) return;
    setBusy(true);
    setError('');
    try {
      const preview = await api.previewPush(selectedRepo);
      if (preview.is_protected) {
        setProtectedPrompt({ action: 'push', preview });
      } else {
        const res = await api.push(selectedRepo);
        setNotice(res.message || 'Push выполнен');
        await loadStatus(selectedRepo);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const runSimple = async (fn: () => Promise<{ message: string }>) => {
    setBusy(true);
    setError('');
    setNotice('');
    try {
      const res = await fn();
      setNotice(res.message || 'Готово');
      await loadStatus(selectedRepo);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const sendConflict = async () => {
    if (!selectedRepo) return;
    setBusy(true);
    setError('');
    try {
      const res = await api.sendConflictToChat(selectedRepo, CONVERSATION_ID);
      if (res.sent) {
        setNotice('Контекст конфликта отправлен в открытый чат ИИ');
      } else {
        await navigator.clipboard.writeText(res.prompt);
        setNotice('Чат недоступен напрямую — промпт скопирован в буфер обмена');
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="gw-stack">
      <div className="gw-row">
        <input
          className="gw-input"
          placeholder="Фильтр проекта..."
          value={repoFilter}
          onChange={(e) => setRepoFilter(e.target.value)}
        />
        <button className="gw-btn" onClick={() => void loadStatus(selectedRepo)} disabled={busy}>
          ↻
        </button>
        <button className="gw-btn" onClick={() => setShowSettings((v) => !v)}>
          ⚙
        </button>
      </div>

      <select
        className="gw-select"
        value={selectedRepo}
        onChange={(e) => setSelectedRepo(e.target.value)}
      >
        {filteredRepos.map((r) => (
          <option key={r.path} value={r.path}>
            {r.name} {r.is_worktree ? '(wt)' : ''} — {r.path}
          </option>
        ))}
      </select>

      {showSettings && (
        <SettingsPanel
          config={config}
          busy={busy}
          onClose={() => setShowSettings(false)}
          onSave={async (cfg) => {
            setBusy(true);
            try {
              const res = await api.saveConfig(cfg);
              setRepos(res.repos);
              setConfig(res.config);
              setShowSettings(false);
            } finally {
              setBusy(false);
            }
          }}
        />
      )}

      {status && (
        <div className="gw-card gw-stack">
          <div className="gw-row" style={{ flexWrap: 'wrap' }}>
            <strong>{status.branch}</strong>
            {status.ahead > 0 && <span className="gw-badge gw-badge-ok">↑{status.ahead}</span>}
            {status.behind > 0 && <span className="gw-badge gw-badge-warn">↓{status.behind}</span>}
            {status.is_dirty && (
              <span className="gw-badge gw-badge-warn">dirty ({status.changes.length})</span>
            )}
            {status.is_merging && <span className="gw-badge gw-badge-danger">CONFLICT</span>}
          </div>

          <div className="gw-row">
            <select
              className="gw-select"
              value={status.branch}
              disabled={busy}
              onChange={(e) =>
                void executeGuarded('checkout', e.target.value, { mode: 'fail_if_dirty' })
              }
            >
              {status.local_branches.map((b) => (
                <option key={b} value={b}>
                  checkout: {b}
                </option>
              ))}
            </select>
          </div>

          <div className="gw-row">
            <button
              className="gw-btn"
              disabled={busy}
              onClick={() => void runSimple(() => api.fetchAll(selectedRepo))}
            >
              Fetch
            </button>
            <button
              className="gw-btn"
              disabled={busy}
              onClick={() => void executeGuarded('pull', null, { mode: 'fail_if_dirty' })}
            >
              Pull
            </button>
            <button
              className="gw-btn gw-btn-primary"
              disabled={busy}
              onClick={() => void handlePushClick()}
            >
              Push
            </button>
          </div>

          <div className="gw-row">
            <select
              className="gw-select"
              value={mergeSource}
              onChange={(e) => setMergeSource(e.target.value)}
            >
              {status.local_branches
                .filter((b) => b !== status.branch)
                .map((b) => (
                  <option key={b} value={b}>
                    {b} → {status.branch}
                  </option>
                ))}
            </select>
            <button
              className="gw-btn"
              disabled={busy || !mergeSource}
              onClick={() => void handleMergeClick()}
            >
              Merge
            </button>
          </div>
        </div>
      )}

      {dirtyPrompt && (
        <DirtyModal
          kind={dirtyPrompt.kind}
          target={dirtyPrompt.target}
          files={dirtyPrompt.files}
          busy={busy}
          onStashPop={() =>
            void executeGuarded(dirtyPrompt.kind, dirtyPrompt.target, { mode: 'stash_and_pop' })
          }
          onQuickCommit={(message) =>
            void executeGuarded(dirtyPrompt.kind, dirtyPrompt.target, {
              mode: 'quick_commit',
              message,
            })
          }
          onCancel={() => setDirtyPrompt(null)}
        />
      )}

      {protectedPrompt && (
        <PreviewModal
          actionLabel={protectedPrompt.action === 'merge' ? 'Merge' : 'Push'}
          preview={protectedPrompt.preview}
          busy={busy}
          onConfirm={async () => {
            const act = protectedPrompt.action;
            setProtectedPrompt(null);
            if (act === 'merge') {
              await executeGuarded('merge', mergeSource, { mode: 'fail_if_dirty' });
            } else {
              await runSimple(() => api.push(selectedRepo));
            }
          }}
          onCancel={() => setProtectedPrompt(null)}
        />
      )}

      {status && (status.is_merging || status.conflicts.length > 0) && (
        <div className="gw-card gw-stack" style={{ borderColor: 'var(--gw-danger)' }}>
          <strong style={{ color: '#fecaca' }}>Merge-конфликт</strong>
          <ul className="gw-list">
            {status.conflicts.map((c) => (
              <li key={c}>{c}</li>
            ))}
          </ul>
          <div className="gw-row">
            <button className="gw-btn gw-btn-primary" disabled={busy} onClick={() => void sendConflict()}>
              Передать ИИ в чат
            </button>
            <button
              className="gw-btn gw-btn-danger"
              disabled={busy}
              onClick={() => void runSimple(() => api.abortMerge(selectedRepo))}
            >
              Abort merge
            </button>
          </div>
        </div>
      )}

      {status && (
        <div className="gw-card gw-stack">
          <div className="gw-row">
            <input
              className="gw-input"
              placeholder="Быстрый commit (all)..."
              value={commitMsg}
              onChange={(e) => setCommitMsg(e.target.value)}
            />
            <button
              className="gw-btn"
              disabled={busy || !commitMsg.trim()}
              onClick={async () => {
                const msg = commitMsg.trim();
                setCommitMsg('');
                await runSimple(() => api.commitAll(selectedRepo, msg));
              }}
            >
              Commit
            </button>
          </div>
          <div className="gw-row">
            <button
              className="gw-btn"
              disabled={busy || !status.is_dirty}
              onClick={() => void runSimple(() => api.stashSave(selectedRepo))}
            >
              Stash
            </button>
            <button
              className="gw-btn"
              disabled={busy}
              onClick={() => void runSimple(() => api.stashPop(selectedRepo))}
            >
              Stash Pop
            </button>
          </div>
        </div>
      )}

      {notice && <div className="gw-card gw-muted">{notice}</div>}
      {error && (
        <div className="gw-card" style={{ borderColor: 'var(--gw-danger)', color: '#fecaca' }}>
          {error}
        </div>
      )}
    </div>
  );
}
