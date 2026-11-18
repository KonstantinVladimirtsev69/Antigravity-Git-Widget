import { useCallback, useEffect, useMemo, useState } from 'react';
import { CONVERSATION_ID, api } from './api';
import { BranchPanel } from './BranchPanel';
import { ChangesPanel } from './ChangesPanel';
import { DiffReview } from './DiffReview';
import { GraphPanel } from './GraphPanel';
import { DirtyModal, PreviewModal, RemoteModal, SettingsPanel } from './Modals';
import { Picker } from './Picker';
import { PrPanel } from './PrPanel';
import { StashPanel } from './StashPanel';
import type {
  ActionPreview,
  CommitInfo,
  DiffFile,
  DirtyStrategy,
  GraphResponse,
  GuardedOpKind,
  PrListResponse,
  RepoInfo,
  RepoStatus,
  ScannerConfig,
  StashEntry,
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
  const [commits, setCommits] = useState<CommitInfo[]>([]);
  const [stashes, setStashes] = useState<StashEntry[]>([]);
  const [graphData, setGraphData] = useState<GraphResponse | null>(null);
  const [graphAll, setGraphAll] = useState<boolean>(true);
  const [graphAuthor, setGraphAuthor] = useState<string>('');
  const [prData, setPrData] = useState<PrListResponse | null>(null);
  const [selectedCommit, setSelectedCommit] = useState<string>('');
  const [diffMode, setDiffMode] = useState<'working' | 'branch' | 'commit'>('working');
  const [mergeTarget, setMergeTarget] = useState<string>('');
  const [mergeDir, setMergeDir] = useState<'into_current' | 'current_into_target'>('current_into_target');
  const [selectedRemote, setSelectedRemote] = useState<string>('');
  const [focusFile, setFocusFile] = useState<string | null>(null);
  const [showRemoteForm, setShowRemoteForm] = useState(false);
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
    async (
      repoPath: string,
      mode: 'working' | 'branch' | 'commit' = diffMode,
      commitHash: string = selectedCommit,
      allBranches: boolean = graphAll,
      author: string = graphAuthor,
    ) => {
      if (!repoPath) return;
      try {
        const [st, cm, sl, gr] = await Promise.all([
          api.getStatus(repoPath),
          api.getCommits(repoPath),
          api.listStashes(repoPath),
          api.getGraph(repoPath, allBranches, author || undefined),
        ]);
        const effectiveCommit = commitHash || cm[0]?.hash || '';
        const df = await api.getDiff(
          repoPath,
          mode,
          mode === 'commit' ? effectiveCommit : undefined,
        );
        setStatus(st);
        setCommits(cm);
        setStashes(sl);
        setGraphData(gr);
        if (!commitHash && cm[0]?.hash) setSelectedCommit(cm[0].hash);
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
    [diffMode, selectedCommit, graphAll, graphAuthor],
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
      setNotice('');
      setError('');
      setSelectedCommit('');
      setGraphAuthor('');
      void refreshRepoData(selectedRepo, diffMode, '', graphAll, '');
      void api.getPrs(selectedRepo).then(setPrData).catch(() => setPrData(null));
    }
  }, [selectedRepo]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (!selectedRepo) return;
    const id = window.setInterval(() => {
      if (document.visibilityState === 'visible' && !busy && !dirtyPrompt && !protectedPrompt) {
        void refreshRepoData(selectedRepo, diffMode, selectedCommit);
      }
    }, 3000);
    return () => window.clearInterval(id);
  }, [selectedRepo, diffMode, selectedCommit, busy, dirtyPrompt, protectedPrompt, refreshRepoData]);

  useEffect(() => {
    if (!selectedRepo || !status || status.remotes.length === 0) return;
    const doSilentFetch = () => {
      if (document.visibilityState === 'visible' && !busy) {
        void api
          .fetchAll(selectedRepo, selectedRemote || undefined)
          .then(() => refreshRepoData(selectedRepo, diffMode, selectedCommit))
          .catch(() => {});
      }
    };
    const initial = window.setTimeout(doSilentFetch, 800);
    const id = window.setInterval(doSilentFetch, 45000);
    return () => {
      window.clearTimeout(initial);
      window.clearInterval(id);
    };
  }, [selectedRepo, selectedRemote]); // eslint-disable-line react-hooks/exhaustive-deps

  const repoOptions = useMemo(
    () =>
      repos.map((r) => ({
        value: r.path,
        label: `${r.name}${r.is_worktree ? ' (wt)' : ''}`,
        sub: r.path,
      })),
    [repos],
  );

  const runSimple = async (fn: () => Promise<{ message?: string }>, fallback = 'Done') => {
    setBusy(true);
    setError('');
    setNotice('');
    try {
      const res = await fn();
      setNotice(res.message || fallback);
      await refreshRepoData(selectedRepo);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

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
      if (mergeTarget === 'main' || mergeTarget === 'master') {
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
      if (preview.is_protected) setProtectedPrompt({ action: 'merge', preview });
      else await executeGuarded('merge', mergeTarget, { mode: 'fail_if_dirty' });
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
      if (preview.is_protected) setProtectedPrompt({ action: 'push', preview });
      else await runSimple(() => api.push(selectedRepo, selectedRemote || undefined), 'Pushed');
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
      if (res.sent) setNotice('Conflict context sent to AI chat');
      else {
        await navigator.clipboard.writeText(res.prompt);
        setNotice('Copied conflict prompt to clipboard');
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

      {status && (
        <div className="gw-row">
          <div style={{ flex: 1, minWidth: 0 }}>
            <Picker
              value={selectedRemote}
              placeholder="No remote configured..."
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
      )}

      {status &&
        (status.ahead > 0 || status.behind > 0 || status.is_dirty || status.is_merging || autoMatched) && (
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
        <RemoteModal
          defaultRepoName={
            repos.find((r) => r.path.toLowerCase() === selectedRepo.toLowerCase())?.name ??
            selectedRepo.split(/[\\/]/).pop() ??
            'new-repo'
          }
          hasSavedToken={Boolean(config.github_token)}
          busy={busy}
          onClose={() => setShowRemoteForm(false)}
          onAddExisting={async (name, url) => {
            await runSimple(() => api.addRemote(selectedRepo, name, url));
            setShowRemoteForm(false);
          }}
          onCreateGithub={async (payload) => {
            await runSimple(() => api.createGithubRepo(selectedRepo, payload));
            const res = await api.getRepos();
            setConfig(res.config);
            setShowRemoteForm(false);
          }}
        />
      )}

      {tab === 'diff' && selectedRepo && (
        <DiffReview
          repo={selectedRepo}
          files={diffFiles}
          commits={commits}
          selectedCommit={selectedCommit}
          mode={diffMode}
          focusFile={focusFile}
          busy={busy}
          onModeChange={(m) => {
            setDiffMode(m);
            void refreshRepoData(selectedRepo, m, selectedCommit);
          }}
          onCommitSelect={(hash) => {
            setSelectedCommit(hash);
            void refreshRepoData(selectedRepo, 'commit', hash);
          }}
          onRefresh={() => refreshRepoData(selectedRepo, diffMode, selectedCommit)}
          onNotify={setNotice}
          onError={setError}
        />
      )}

      {tab === 'flow' && status && (
        <>
          <BranchPanel
            status={status}
            selectedRemote={selectedRemote}
            mergeTarget={mergeTarget}
            mergeDir={mergeDir}
            busy={busy}
            onCheckoutLocal={(b) => void executeGuarded('checkout', b, { mode: 'fail_if_dirty' })}
            onCheckoutRemote={(rb) => void runSimple(() => api.checkoutRemote(selectedRepo, rb))}
            onCreateBranch={(name) => runSimple(() => api.createBranch(selectedRepo, name))}
            onDeleteBranch={(name, force) =>
              runSimple(() => api.deleteBranch(selectedRepo, name, force))
            }
            onFetch={() =>
              void runSimple(() => api.fetchAll(selectedRepo, selectedRemote || undefined))
            }
            onPull={() =>
              selectedRemote
                ? void runSimple(() => api.pull(selectedRepo, selectedRemote))
                : void executeGuarded('pull', null, { mode: 'fail_if_dirty' })
            }
            onPush={() => void handlePushClick()}
            onMergeDirToggle={() =>
              setMergeDir((d) =>
                d === 'current_into_target' ? 'into_current' : 'current_into_target',
              )
            }
            onMergeTargetChange={setMergeTarget}
            onMerge={() => void handleMergeClick()}
          />

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
                  if (mergeDir === 'current_into_target') {
                    await runMergeIntoTarget(mergeTarget, status.branch);
                  } else {
                    await executeGuarded('merge', mergeTarget, { mode: 'fail_if_dirty' });
                  }
                } else {
                  await runSimple(() => api.push(selectedRepo, selectedRemote || undefined), 'Pushed');
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

          <ChangesPanel
            staged={status.staged_changes}
            unstaged={status.unstaged_changes}
            busy={busy}
            onCommit={(msg, amend) => runSimple(() => api.commitAll(selectedRepo, msg, amend))}
            onSuggestCommit={async () => {
              await runSimple(async () => {
                const res = await api.suggestCommit(selectedRepo, CONVERSATION_ID);
                if (res.sent) return { message: 'Asked AI chat for commit message' };
                await navigator.clipboard.writeText(res.prompt);
                return { message: 'Copied diff summary to clipboard' };
              });
            }}
            onUndoCommit={() => runSimple(() => api.undoCommit(selectedRepo))}
            onStageFile={(p) =>
              void runSimple(async () => {
                await api.stageFile(selectedRepo, p);
                return { message: `Staged ${p}` };
              })
            }
            onUnstageFile={(p) =>
              void runSimple(async () => {
                await api.unstageFile(selectedRepo, p);
                return { message: `Unstaged ${p}` };
              })
            }
            onDiscardFile={(p) =>
              void runSimple(async () => {
                await api.discardFile(selectedRepo, p);
                return { message: `Discarded ${p}` };
              })
            }
            onStageAll={() => void runSimple(() => api.stageAll(selectedRepo))}
            onUnstageAll={() => void runSimple(() => api.unstageAll(selectedRepo))}
            onDiscardAll={() => void runSimple(() => api.discardAll(selectedRepo))}
            onOpenDiff={(p) => {
              setDiffMode('working');
              setFocusFile(p);
              setTab('diff');
            }}
          />

          <GraphPanel
            repo={selectedRepo}
            commits={graphData?.commits ?? []}
            authors={graphData?.authors ?? []}
            githubUrl={graphData?.github_url ?? null}
            allBranches={graphAll}
            authorFilter={graphAuthor}
            busy={busy}
            onToggleAll={(all) => {
              setGraphAll(all);
              void refreshRepoData(selectedRepo, diffMode, selectedCommit, all, graphAuthor);
            }}
            onAuthorFilter={(author) => {
              setGraphAuthor(author);
              void refreshRepoData(selectedRepo, diffMode, selectedCommit, graphAll, author);
            }}
            onCherryPick={(hash) => void runSimple(() => api.cherryPick(selectedRepo, hash))}
            onInspectCommit={(hash, file) => {
              setSelectedCommit(hash);
              setDiffMode('commit');
              setFocusFile(file ?? null);
              setTab('diff');
              void refreshRepoData(selectedRepo, 'commit', hash, graphAll, graphAuthor);
            }}
            onNotify={setNotice}
          />

          <PrPanel
            prData={prData}
            currentBranch={status.branch}
            baseBranch={mergeTarget || 'main'}
          />

          <StashPanel
            stashes={stashes}
            isDirty={status.is_dirty}
            busy={busy}
            onSave={(m) => void runSimple(() => api.stashSave(selectedRepo, m))}
            onApply={(idx) => void runSimple(() => api.stashApply(selectedRepo, idx))}
            onPop={(idx) => void runSimple(() => api.stashPopAt(selectedRepo, idx))}
            onDrop={(idx) => void runSimple(() => api.stashDrop(selectedRepo, idx))}
          />
        </>
      )}

      {notice && (
        <div className="gw-card gw-muted gw-between" style={{ alignItems: 'flex-start' }}>
          <div style={{ maxHeight: 72, overflowY: 'auto', whiteSpace: 'pre-wrap', flex: 1 }}>
            {notice}
          </div>
          <button
            type="button"
            className="gw-btn gw-btn-xs"
            style={{ marginLeft: 6 }}
            onClick={() => setNotice('')}
          >
            ✕
          </button>
        </div>
      )}
      {error && (
        <div className="gw-card" style={{ borderColor: 'rgba(239, 68, 68, 0.45)', color: '#fca5a5' }}>
          {error}
        </div>
      )}
    </div>
  );
}
