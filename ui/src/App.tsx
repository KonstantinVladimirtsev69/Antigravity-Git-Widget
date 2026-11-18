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
  const [selectedRemote, setSelectedRemote] = useState<string>('');
  const [focusFile, setFocusFile] = useState<string | null>(null);
  const [showRemoteForm, setShowRemoteForm] = useState(false);
  const [remoteName, setRemoteName] = useState('origin');
  const [remoteUrl, setRemoteUrl] = useState('');
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
        setSelectedRemote((prev) => {
          if (prev && st.remotes.includes(prev)) return prev;
          if (st.remotes.includes('origin')) return 'origin';
          return st.remotes[0] ?? '';
        });
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
        if (res.op_result) setNotice(res.op_result.message || `${kind} completed`);
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
            commits: [`Checkout ${mergeTarget} and merge ${status.branch}`],
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
    if (!selectedRepo || !status) return;
    if (status.remotes.length === 0) {
      setShowRemoteForm(true);
      setError('No git remote configured. Add remote below.');
      return;
    }
    if (!status.upstream) {
      await runSimple(() => api.publish(selectedRepo, selectedRemote || status.remotes[0]));
      return;
    }
    setBusy(true);
    setError('');
    try {
      const preview = await api.previewPush(selectedRepo);
      if (preview.is_protected) {
        setProtectedPrompt({ action: 'push', preview });
      } else {
        const res = await api.push(selectedRepo, selectedRemote || undefined);
        setNotice(res.message || 'Pushed');
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
      setNotice(res.message || 'Done');
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
        setNotice('Conflict context sent to AI chat');
      } else {
        await navigator.clipboard.writeText(res.prompt);
        setNotice('Copied conflict prompt to clipboard');
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const hasRemote = (status?.remotes.length ?? 0) > 0;
  const isPublished = Boolean(status?.upstream);

  return (
    <div className="gw-stack">
      <div className="gw-row">
        <Picker
          value={selectedRepo}
          options={repoOptions}
          searchable
          placeholder="Select repository..."
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

      {status &&
        (status.ahead > 0 || status.behind > 0 || status.is_dirty || status.is_merging || autoMatched || !hasRemote) && (
          <div className="gw-between">
            <div className="gw-row" style={{ flexWrap: 'wrap' }}>
              {status.ahead > 0 && <span className="gw-badge gw-badge-ok">↑{status.ahead}</span>}
              {status.behind > 0 && <span className="gw-badge gw-badge-warn">↓{status.behind}</span>}
              {status.is_dirty && (
                <span className="gw-badge gw-badge-warn">dirty ({status.changes.length})</span>
              )}
              {status.is_merging && <span className="gw-badge gw-badge-danger">CONFLICT</span>}
              {!hasRemote && (
                <button
                  type="button"
                  className="gw-btn gw-btn-xs"
                  onClick={() => setShowRemoteForm((v) => !v)}
                >
                  + Add Remote
                </button>
              )}
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
          onInitRepo={async (path) => {
            await runSimple(() => api.initRepo(path));
            const res = await api.getRepos();
            setRepos(res.repos);
            setSelectedRepo(path);
            setShowSettings(false);
          }}
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

      {showRemoteForm && selectedRepo && (
        <div className="gw-card gw-stack">
          <div className="gw-between">
            <strong style={{ fontSize: 11.5 }}>Add Remote</strong>
            <button className="gw-btn gw-btn-xs" onClick={() => setShowRemoteForm(false)}>
              ✕
            </button>
          </div>
          <div className="gw-row">
            <input
              className="gw-input"
              style={{ width: 84 }}
              value={remoteName}
              onChange={(e) => setRemoteName(e.target.value)}
              placeholder="origin"
            />
            <input
              className="gw-input"
              value={remoteUrl}
              onChange={(e) => setRemoteUrl(e.target.value)}
              placeholder="https://github.com/user/repo.git"
            />
          </div>
          <button
            className="gw-btn gw-btn-primary"
            disabled={busy || !remoteName.trim() || !remoteUrl.trim()}
            onClick={async () => {
              await runSimple(() => api.addRemote(selectedRepo, remoteName, remoteUrl));
              setRemoteUrl('');
              setShowRemoteForm(false);
            }}
          >
            Save Remote
          </button>
        </div>
      )}

      {tab === 'diff' && selectedRepo && (
        <DiffReview
          repo={selectedRepo}
          files={diffFiles}
          mode={diffMode}
          focusFile={focusFile}
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
            <Picker
              value={status.branch}
              options={status.local_branches.map((b) => ({ value: b, label: b }))}
              disabled={busy}
              onChange={(b) => {
                if (b !== status.branch) {
                  void executeGuarded('checkout', b, { mode: 'fail_if_dirty' });
                }
              }}
            />

            <div className="gw-row">
              <div style={{ flex: 1, minWidth: 0 }}>
                <Picker
                  value={selectedRemote}
                  placeholder="No remote..."
                  options={status.remotes.map((r) => ({ value: r, label: `remote: ${r}` }))}
                  disabled={busy || status.remotes.length === 0}
                  onChange={setSelectedRemote}
                />
              </div>
              <button
                type="button"
                className="gw-btn"
                title="Add remote"
                onClick={() => setShowRemoteForm((v) => !v)}
              >
                +
              </button>
            </div>

            <div className="gw-row">
              <button
                className="gw-btn"
                style={{ flex: 1 }}
                disabled={busy || !hasRemote}
                onClick={() =>
                  void runSimple(() => api.fetchAll(selectedRepo, selectedRemote || undefined))
                }
              >
                Fetch
              </button>
              <button
                className="gw-btn"
                style={{ flex: 1 }}
                disabled={busy || !hasRemote}
                onClick={() =>
                  selectedRemote
                    ? void runSimple(() => api.pull(selectedRepo, selectedRemote))
                    : void executeGuarded('pull', null, { mode: 'fail_if_dirty' })
                }
              >
                Pull
              </button>
              <button
                className="gw-btn gw-btn-primary"
                style={{ flex: 1 }}
                disabled={busy}
                onClick={() => void handlePushClick()}
              >
                {!hasRemote ? 'Set Remote' : isPublished ? 'Push' : 'Publish Branch'}
              </button>
            </div>

            <div className="gw-row">
              <button
                type="button"
                className="gw-btn"
                title="Toggle merge direction"
                onClick={() =>
                  setMergeDir((d) =>
                    d === 'current_into_target' ? 'into_current' : 'current_into_target',
                  )
                }
              >
                {mergeDir === 'current_into_target' ? '→' : '←'}
              </button>
              <div style={{ flex: 1, minWidth: 0 }}>
                <Picker
                  value={mergeTarget}
                  placeholder="Target branch..."
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
                  await runSimple(() => api.push(selectedRepo, selectedRemote || undefined));
                }
              }}
              onCancel={() => setProtectedPrompt(null)}
            />
          )}

          {(status.is_merging || status.conflicts.length > 0) && (
            <div className="gw-card gw-stack" style={{ borderColor: 'rgba(239, 68, 68, 0.45)' }}>
              <strong style={{ color: '#fca5a5' }}>Merge Conflict</strong>
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
                  Send to AI Chat
                </button>
                <button
                  className="gw-btn gw-btn-danger"
                  disabled={busy}
                  onClick={() => void runSimple(() => api.abortMerge(selectedRepo))}
                >
                  Abort Merge
                </button>
              </div>
            </div>
          )}

          <div className="gw-card gw-stack">
            {status.changes.length > 0 && (
              <div className="gw-stack" style={{ gap: 2, maxHeight: 110, overflowY: 'auto' }}>
                {status.changes.map((c) => (
                  <button
                    key={`${c.code}:${c.path}`}
                    type="button"
                    className="gw-picker-item gw-between"
                    style={{ padding: '3px 5px' }}
                    onClick={() => {
                      setDiffMode('working');
                      setFocusFile(c.path);
                      setTab('diff');
                    }}
                  >
                    <span
                      style={{
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {c.path}
                    </span>
                    <span className="gw-badge" style={{ marginLeft: 6 }}>
                      {c.code}
                    </span>
                  </button>
                ))}
              </div>
            )}
            <div className="gw-row">
              <input
                className="gw-input"
                placeholder="Quick commit (all)..."
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
        <div className="gw-card" style={{ borderColor: 'rgba(239, 68, 68, 0.45)', color: '#fca5a5' }}>
          {error}
        </div>
      )}
    </div>
  );
}
