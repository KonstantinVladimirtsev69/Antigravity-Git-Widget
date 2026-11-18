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
  const [diffMode, setDiffMode] = useState<'working' | 'branch'>('working');
  const [mergeTarget, setMergeTarget] = useState<string>('');
  const [mergeDir, setMergeDir] = useState<'into_current' | 'current_into_target'>('current_into_target');
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

  const refreshRepoData = useCallback(
    async (repoPath: string, mode: 'working' | 'branch' = diffMode) => {
      if (!repoPath) return;
      try {
        const [st, df] = await Promise.all([
          api.getStatus(repoPath),
          api.getDiff(repoPath, mode),
        ]);
        setStatus(st);
        setDiffFiles(df);
        setMergeTarget((prev) => {
          if (prev && st.local_branches.includes(prev) && prev !== st.branch) return prev;
          if (st.local_branches.includes('main') && st.branch !== 'main') return 'main';
          if (st.local_branches.includes('developer') && st.branch !== 'developer') return 'developer';
          return st.local_branches.find((b) => b !== st.branch) ?? '';
        });
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    [diffMode],
  );

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

  const runMergeIntoTarget = async (targetBranch: string, sourceBranch: string) => {
    setBusy(true);
    setError('');
    setNotice('');
    try {
      const ck = await api.runGuarded(selectedRepo, 'checkout', targetBranch, {
        mode: 'fail_if_dirty',
      });
      if (ck.blocked_dirty) {
        setDirtyPrompt({ kind: 'checkout', target: targetBranch, files: ck.dirty_files });
        return;
      }
      const mg = await api.runGuarded(selectedRepo, 'merge', sourceBranch, {
        mode: 'fail_if_dirty',
      });
      setNotice(mg.op_result?.message || `Merged ${sourceBranch} → ${targetBranch}`);
      await refreshRepoData(selectedRepo);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleMergeClick = async () => {
    if (!selectedRepo || !mergeTarget || !status) return;
    if (mergeDir === 'current_into_target') {
      const isProtected = mergeTarget === 'main' || mergeTarget === 'master';
      if (isProtected) {
        setProtectedPrompt({
          action: 'merge',
          preview: {
            source: status.branch,
            target: mergeTarget,
            is_protected: true,
            commit_count: 0,
            commits: [`Ветка ${status.branch} будет влита в ${mergeTarget} (checkout ${mergeTarget} → merge ${status.branch})`],
          },
        });
      } else {
        await runMergeIntoTarget(mergeTarget, status.branch);
      }
      return;
    }

    setBusy(true);
    setError('');
    try {
      const preview = await api.previewMerge(selectedRepo, mergeTarget);
      if (preview.is_protected) {
        setProtectedPrompt({ action: 'merge', preview });
      } else {
        await executeGuarded('merge', mergeTarget, { mode: 'fail_if_dirty' });
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

      {status && (status.ahead > 0 || status.behind > 0 || status.is_dirty || status.is_merging || autoMatched) && (
        <div className="gw-between">
          <div className="gw-row" style={{ flexWrap: 'wrap' }}>
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
          mode={diffMode}
          busy={busy}
          onModeChange={(m) => {
            setDiffMode(m);
            void refreshRepoData(selectedRepo, m);
          }}
          onRefresh={() => refreshRepoData(selectedRepo, diffMode)}
          onNotify={setNotice}
          onError={setError}
        />
      )}

      {tab === 'flow' && status && (
        <>
          <div className="gw-card gw-stack">
            <div className="gw-row">
              <span className="gw-muted" style={{ minWidth: 68 }}>Ветка:</span>
              <div style={{ flex: 1, minWidth: 0 }}>
                <Picker
                  value={status.branch}
                  options={status.local_branches.map((b) => ({
                    value: b,
                    label: b === status.branch ? `${b} (текущая)` : `Переключиться → ${b}`,
                  }))}
                  disabled={busy}
                  onChange={(b) => {
                    if (b !== status.branch) {
                      void executeGuarded('checkout', b, { mode: 'fail_if_dirty' });
                    }
                  }}
                />
              </div>
            </div>

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
              <button
                type="button"
                className="gw-btn"
                title="Сменить направление merge"
                onClick={() =>
                  setMergeDir((d) =>
                    d === 'current_into_target' ? 'into_current' : 'current_into_target',
                  )
                }
              >
                {mergeDir === 'current_into_target' ? 'Влить →' : '← Взять'}
              </button>
              <div style={{ flex: 1, minWidth: 0 }}>
                <Picker
                  value={mergeTarget}
                  placeholder="Выбрать ветку..."
                  options={status.local_branches
                    .filter((b) => b !== status.branch)
                    .map((b) => ({
                      value: b,
                      label:
                        mergeDir === 'current_into_target'
                          ? `${status.branch} → ${b}`
                          : `${b} → ${status.branch}`,
                    }))}
                  disabled={busy}
                  onChange={setMergeTarget}
                />
              </div>
              <button
                className="gw-btn"
                disabled={busy || !mergeTarget}
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
                  if (mergeDir === 'current_into_target' && status) {
                    await runMergeIntoTarget(mergeTarget, status.branch);
                  } else {
                    await executeGuarded('merge', mergeTarget, { mode: 'fail_if_dirty' });
                  }
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
