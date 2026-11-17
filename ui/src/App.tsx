import { useCallback, useEffect, useMemo, useState } from 'react';
import { CONVERSATION_ID, api } from './api';
import { DiffReview } from './DiffReview';
import { DirtyModal, PreviewModal, SettingsPanel } from './Modals';
import { Picker } from './Picker';
import type {
  ActionPreview,
  DiffFile,
  DirtyStrategy,
  GuardedOpKind,
  RepoInfo,
  RepoStatus,
  ScannerConfig,
} from './types';

declare global {
  interface Window {
    __gwBridge?: {
      workspaceUris: string[];
      subscribe: (fn: (uris: string[]) => void) => () => void;
    };
  }
}

export default function App() {
  const [tab, setTab] = useState<'flow' | 'diff'>('flow');
  const [repos, setRepos] = useState<RepoInfo[]>([]);
  const [config, setConfig] = useState<ScannerConfig>({ roots: ['C:\\Projects\\'], max_depth: 3 });
  const [selectedRepo, setSelectedRepo] = useState<string>('');
  const [autoMatched, setAutoMatched] = useState<boolean>(false);
  const [status, setStatus] = useState<RepoStatus | null>(null);
  const [diffFiles, setDiffFiles] = useState<DiffFile[]>([]);
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

  const refreshRepoData = useCallback(async (repoPath: string) => {
    if (!repoPath) return;
    try {
      const [st, df] = await Promise.all([api.getStatus(repoPath), api.getDiff(repoPath)]);
      setStatus(st);
      setDiffFiles(df);
      setMergeSource((prev) => {
        if (prev && st.local_branches.includes(prev) && prev !== st.branch) return prev;
        return st.local_branches.find((b) => b !== st.branch) ?? '';
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void (async () => {
      try {
        const res = await api.getRepos();
        setRepos(res.repos);
        setConfig(res.config);
        const uris = window.__gwBridge?.workspaceUris ?? [];
        if (uris.length > 0) {
          const matched = await api.resolveWorkspace(uris);
          if (matched.matched_path) {
            setSelectedRepo(matched.matched_path);
            setAutoMatched(true);
            return;
          }
        }
        setSelectedRepo((prev) => prev || res.repos[0]?.path || '');
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    })();
  }, []);

  useEffect(() => {
    if (!window.__gwBridge) return;
    return window.__gwBridge.subscribe((uris) => {
      if (uris.length === 0) return;
      void api.resolveWorkspace(uris).then((res) => {
        if (res.matched_path) {
          setSelectedRepo(res.matched_path);
          setAutoMatched(true);
        }
      });
    });
  }, []);

  useEffect(() => {
    if (selectedRepo) {
      void refreshRepoData(selectedRepo);
    }
  }, [selectedRepo, refreshRepoData]);

  const repoOptions = useMemo(
    () =>
      repos.map((r) => ({
        value: r.path,
        label: `${r.name}${r.is_worktree ? ' (wt)' : ''}`,
        sub: r.path,
      })),
    [repos],
  );

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
        if (res.op_result) setNotice(res.op_result.message || `${kind} выполнен`);
        await refreshRepoData(selectedRepo);
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
        await refreshRepoData(selectedRepo);
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
      await refreshRepoData(selectedRepo);
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
        setNotice('Чат недоступен — промпт скопирован в буфер');
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
        <Picker
          value={selectedRepo}
          options={repoOptions}
          searchable
          placeholder="Выбрать репозиторий..."
          onChange={(val) => {
            setAutoMatched(false);
            setSelectedRepo(val);
          }}
        />
        <button className="gw-btn" onClick={() => void refreshRepoData(selectedRepo)} disabled={busy}>
          ↻
        </button>
        <button className="gw-btn" onClick={() => setShowSettings((v) => !v)}>
          ⚙
        </button>
      </div>

      {status && (
        <div className="gw-between">
          <div className="gw-row" style={{ flexWrap: 'wrap' }}>
            <span className="gw-badge gw-badge-ok">{status.branch}</span>
            {status.ahead > 0 && <span className="gw-badge gw-badge-ok">↑{status.ahead}</span>}
            {status.behind > 0 && <span className="gw-badge gw-badge-warn">↓{status.behind}</span>}
            {status.is_dirty && (
              <span className="gw-badge gw-badge-warn">dirty ({status.changes.length})</span>
            )}
            {status.is_merging && <span className="gw-badge gw-badge-danger">CONFLICT</span>}
          </div>
          {autoMatched && <span className="gw-badge">Auto</span>}
        </div>
      )}

      <div className="gw-tabs">
        <button
          type="button"
          className={`gw-tab ${tab === 'flow' ? 'active' : ''}`}
          onClick={() => setTab('flow')}
        >
          Git Flow
        </button>
        <button
          type="button"
          className={`gw-tab ${tab === 'diff' ? 'active' : ''}`}
          onClick={() => setTab('diff')}
        >
          Diff Review {diffFiles.length > 0 ? `(${diffFiles.length})` : ''}
        </button>
      </div>

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

      {tab === 'diff' && selectedRepo && (
        <DiffReview
          repo={selectedRepo}
          files={diffFiles}
          busy={busy}
          onRefresh={() => refreshRepoData(selectedRepo)}
          onNotify={setNotice}
          onError={setError}
        />
      )}

      {tab === 'flow' && status && (
        <>
          <div className="gw-card gw-stack">
            <Picker
              value={status.branch}
              options={status.local_branches.map((b) => ({ value: b, label: `checkout: ${b}` }))}
              disabled={busy}
              onChange={(b) => void executeGuarded('checkout', b, { mode: 'fail_if_dirty' })}
            />

            <div className="gw-row">
              <button
                className="gw-btn"
                style={{ flex: 1 }}
                disabled={busy}
                onClick={() => void runSimple(() => api.fetchAll(selectedRepo))}
              >
                Fetch
              </button>
              <button
                className="gw-btn"
                style={{ flex: 1 }}
                disabled={busy}
                onClick={() => void executeGuarded('pull', null, { mode: 'fail_if_dirty' })}
              >
                Pull
              </button>
              <button
                className="gw-btn gw-btn-primary"
                style={{ flex: 1 }}
                disabled={busy}
                onClick={() => void handlePushClick()}
              >
                Push
              </button>
            </div>

            <div className="gw-row">
              <div style={{ flex: 1, minWidth: 0 }}>
                <Picker
                  value={mergeSource}
                  placeholder="Ветка для merge..."
                  options={status.local_branches
                    .filter((b) => b !== status.branch)
                    .map((b) => ({ value: b, label: `${b} → ${status.branch}` }))}
                  disabled={busy}
                  onChange={setMergeSource}
                />
              </div>
              <button
                className="gw-btn"
                disabled={busy || !mergeSource}
                onClick={() => void handleMergeClick()}
              >
                Merge
              </button>
            </div>
          </div>

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

          {(status.is_merging || status.conflicts.length > 0) && (
            <div className="gw-card gw-stack" style={{ borderColor: 'var(--gw-danger)' }}>
              <strong style={{ color: '#fca5a5' }}>Merge-конфликт</strong>
              <ul className="gw-list">
                {status.conflicts.map((c) => (
                  <li key={c}>{c}</li>
                ))}
              </ul>
              <div className="gw-row">
                <button
                  className="gw-btn gw-btn-primary"
                  disabled={busy}
                  onClick={() => void sendConflict()}
                >
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
        </>
      )}

      {notice && <div className="gw-card gw-muted">{notice}</div>}
      {error && (
        <div className="gw-card" style={{ borderColor: 'var(--gw-danger)', color: '#fca5a5' }}>
          {error}
        </div>
      )}
    </div>
  );
}
