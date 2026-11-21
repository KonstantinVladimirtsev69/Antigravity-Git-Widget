import { useCallback, useEffect, useState } from 'react';
import { CONVERSATION_ID, api } from './api';
import { BranchPanel } from './BranchPanel';
import { ChangesPanel } from './ChangesPanel';
import { ConflictBanner } from './components/ConflictBanner';
import { FlowModals } from './components/FlowModals';
import { HeaderBar } from './components/HeaderBar';
import { NoticeFooter } from './components/NoticeFooter';
import { DiffReview } from './DiffReview';
import { GraphPanel } from './GraphPanel';
import { RemoteModal, SettingsPanel } from './Modals';
import { PrPanel } from './PrPanel';
import { StashPanel } from './StashPanel';
import { useIntegrationFlow } from './hooks/useIntegrationFlow';
import { MODE_STORAGE_KEY, VALID_MODES, useRepoData } from './hooks/useRepoData';

declare global {
  interface Window {
    __gwBridge?: { workspaceUris: string[]; subscribe: (fn: (uris: string[]) => void) => () => void };
  }
}

export default function App() {
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  const [showSettings, setShowSettings] = useState(false);
  const [showRemoteForm, setShowRemoteForm] = useState(false);

  const clearMessages = useCallback(() => {
    setNotice('');
    setError('');
  }, []);

  const data = useRepoData(busy, showSettings || showRemoteForm, setError, clearMessages);
  const flow = useIntegrationFlow({
    selectedRepo: data.selectedRepo,
    status: data.status,
    selectedRemote: data.selectedRemote,
    mergeTarget: data.mergeTarget,
    busy,
    setBusy,
    setNotice,
    setError,
    activeRepoRef: data.activeRepoRef,
    lastFetchAtRef: data.lastFetchAtRef,
    refreshRepoData: data.refreshRepoData,
  });

  useEffect(() => {
    void (async () => {
      try {
        const res = await api.getRepos();
        data.setRepos(res.repos);
        data.setConfig(res.config);
        if (res.config.integration_mode && VALID_MODES.includes(res.config.integration_mode)) {
          flow.setIntegrationMode(res.config.integration_mode);
          try { window.localStorage.setItem(MODE_STORAGE_KEY, res.config.integration_mode); } catch { /* ignore */ }
        }
        const uris = window.__gwBridge?.workspaceUris ?? [];
        if (uris.length > 0) {
          const matched = await api.resolveWorkspace(uris);
          if (matched.matched_path) {
            data.setSelectedRepo(matched.matched_path);
            data.setAutoMatched(true);
            return;
          }
        }
        data.setSelectedRepo((prev) => prev || res.repos[0]?.path || '');
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    })();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (!window.__gwBridge) return;
    return window.__gwBridge.subscribe((uris) => {
      if (uris.length === 0) return;
      void api.resolveWorkspace(uris).then((res) => {
        if (res.matched_path) {
          data.setSelectedRepo(res.matched_path);
          data.setAutoMatched(true);
        }
      });
    });
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const { status, selectedRepo } = data;
  const diffCount =
    data.tab === 'diff' || data.diffMode !== 'working'
      ? data.diffFiles.length
      : (status?.changes.length ?? data.diffFiles.length);

  return (
    <div className="gw-stack">
      <HeaderBar
        selectedRepo={selectedRepo}
        repoOptions={data.repoOptions}
        status={status}
        selectedRemote={data.selectedRemote}
        autoMatched={data.autoMatched}
        tab={data.tab}
        diffCount={diffCount}
        busy={busy}
        onSelectRepo={(val) => { data.setAutoMatched(false); data.setSelectedRepo(val); }}
        onRefresh={() => void data.refreshRepoData(selectedRepo)}
        onToggleSettings={() => setShowSettings((v) => !v)}
        onSelectRemote={data.setSelectedRemote}
        onToggleRemoteForm={() => setShowRemoteForm((v) => !v)}
        onSwitchTab={(t) => { data.setTab(t); if (t === 'diff') void data.refreshRepoData(selectedRepo); }}
      />

      {showSettings && (
        <SettingsPanel
          config={data.config}
          busy={busy}
          onClose={() => setShowSettings(false)}
          onInitRepo={async (path) => {
            if (!(await flow.runSimple(() => api.initRepo(path)))) return false;
            const res = await api.getRepos();
            data.setRepos(res.repos);
            data.setSelectedRepo(path);
            setShowSettings(false);
            return true;
          }}
          onSave={async (cfg) => {
            setBusy(true);
            try {
              const res = await api.saveConfig(cfg);
              data.setRepos(res.repos);
              data.setConfig(res.config);
              setShowSettings(false);
            } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); }
          }}
        />
      )}

      {showRemoteForm && selectedRepo && (
        <RemoteModal
          defaultRepoName={
            data.repos.find((r) => r.path.toLowerCase() === selectedRepo.toLowerCase())?.name ??
            selectedRepo.split(/[\\/]/).pop() ?? 'new-repo'
          }
          hasSavedToken={Boolean(data.config.has_github_token)}
          busy={busy}
          onClose={() => setShowRemoteForm(false)}
          onAuthChanged={async () => {
            data.setConfig((await api.getRepos()).config);
          }}
          onError={setError}
          onAddExisting={async (name, url) => {
            if (await flow.runSimple(() => api.addRemote(selectedRepo, name, url))) setShowRemoteForm(false);
          }}
          onCreateGithub={async (payload) => {
            if (await flow.runSimple(() => api.createGithubRepo(selectedRepo, payload))) {
              data.setConfig((await api.getRepos()).config);
              setShowRemoteForm(false);
            }
          }}
        />
      )}

      {data.tab === 'diff' && selectedRepo && (
        <DiffReview
          repo={selectedRepo}
          files={data.diffFiles}
          commits={data.commits}
          selectedCommit={data.selectedCommit}
          mode={data.diffMode}
          focusFile={data.focusFile}
          busy={busy}
          onModeChange={(m) => { data.setDiffMode(m); void data.refreshRepoData(selectedRepo, { mode: m }); }}
          onCommitSelect={(h) => { data.setSelectedCommit(h); void data.refreshRepoData(selectedRepo, { mode: 'commit', commitHash: h }); }}
          onRefresh={() => data.refreshRepoData(selectedRepo)}
          onNotify={setNotice}
          onError={setError}
        />
      )}

      {data.tab === 'flow' && status && (
        <>
          <BranchPanel
            status={status}
            selectedRemote={data.selectedRemote}
            mergeTarget={data.mergeTarget}
            mergeDir={flow.mergeDir}
            integrationMode={flow.integrationMode}
            dualSync={flow.dualSync}
            scanningSync={flow.scanningSync}
            busy={busy}
            onCheckoutLocal={(b) => void flow.executeGuarded('checkout', b, { mode: 'fail_if_dirty' })}
            onCheckoutRemote={(rb) => void flow.runSimple(() => api.checkoutRemote(selectedRepo, rb))}
            onCreateBranch={async (n) => { await flow.runSimple(() => api.createBranch(selectedRepo, n)); }}
            onDeleteBranch={async (n, f) => { await flow.runSimple(() => api.deleteBranch(selectedRepo, n, f)); }}
            onFetch={() => {
              data.lastFetchAtRef.current = Date.now();
              void flow.runSimple(() => api.fetchAll(selectedRepo, data.selectedRemote || undefined));
            }}
            onPull={() => void flow.executeGuarded('pull', data.selectedRemote || null, { mode: 'fail_if_dirty' })}
            onPullBoth={() => void flow.handlePullBoth()}
            onPush={() => void flow.handlePushClick(() => setShowRemoteForm(true))}
            onMergeDirToggle={() => flow.setMergeDir((d) => (d === 'current_into_target' ? 'into_current' : 'current_into_target'))}
            onMergeTargetChange={data.setMergeTarget}
            onIntegrationModeChange={flow.handleIntegrationModeChange}
            onIntegrate={() => void flow.handleIntegrateClick()}
          />

          <FlowModals
            integrationMode={flow.integrationMode}
            busy={busy}
            unpulledPrompt={flow.unpulledPrompt}
            dirtyPrompt={flow.dirtyPrompt}
            protectedPrompt={flow.protectedPrompt}
            onPullAndIntegrate={() => void flow.confirmPullAndIntegrate()}
            onSkipPullAndIntegrate={() => void flow.skipPullAndIntegrate()}
            onCancelUnpulled={() => flow.setUnpulledPrompt(null)}
            onResolveDirty={flow.resolveDirtyPrompt}
            onCancelDirty={() => flow.setDirtyPrompt(null)}
            onConfirmProtected={() => void flow.confirmProtectedPrompt()}
            onCancelProtected={() => flow.setProtectedPrompt(null)}
          />

          <ConflictBanner
            status={status}
            busy={busy}
            onSendConflict={() => void flow.sendConflict()}
            onContinueRebase={() => void flow.runSimple(() => api.continueRebase(selectedRepo))}
            onAbortRebase={() => void flow.runSimple(() => api.abortRebase(selectedRepo))}
            onAbortMerge={() => void flow.runSimple(() => api.abortMerge(selectedRepo))}
          />

          <ChangesPanel
            staged={status.staged_changes}
            unstaged={status.unstaged_changes}
            busy={busy}
            onCommit={async (msg, amend) => { await flow.runSimple(() => api.commitAll(selectedRepo, msg, amend)); }}
            onSuggestCommit={async () => {
              await flow.runSimple(async () => {
                const res = await api.suggestCommit(selectedRepo, CONVERSATION_ID);
                if (res.sent) return { message: 'Asked AI chat for commit message' };
                await navigator.clipboard.writeText(res.prompt);
                return { message: 'Copied diff summary to clipboard' };
              });
            }}
            onUndoCommit={async () => { await flow.runSimple(() => api.undoCommit(selectedRepo)); }}
            onStageFile={(p) => void flow.runSimple(async () => { await api.stageFile(selectedRepo, p); return { message: `Staged ${p}` }; })}
            onUnstageFile={(p) => void flow.runSimple(async () => { await api.unstageFile(selectedRepo, p); return { message: `Unstaged ${p}` }; })}
            onDiscardFile={(p) => void flow.runSimple(async () => { await api.discardFile(selectedRepo, p); return { message: `Discarded ${p}` }; })}
            onStageAll={() => void flow.runSimple(() => api.stageAll(selectedRepo))}
            onUnstageAll={() => void flow.runSimple(() => api.unstageAll(selectedRepo))}
            onDiscardAll={() => void flow.runSimple(() => api.discardAll(selectedRepo))}
            onOpenDiff={(p) => { data.setDiffMode('working'); data.setFocusFile(p); data.setTab('diff'); void data.refreshRepoData(selectedRepo, { mode: 'working' }); }}
          />

          <GraphPanel
            repo={selectedRepo}
            commits={data.graphData?.commits ?? []}
            authors={data.graphData?.authors ?? []}
            githubUrl={data.graphData?.github_url ?? null}
            allBranches={data.graphAll}
            authorFilter={data.graphAuthor}
            busy={busy}
            onToggleAll={(all) => { data.setGraphAll(all); void data.refreshRepoData(selectedRepo, { allBranches: all }); }}
            onAuthorFilter={(author) => { data.setGraphAuthor(author); void data.refreshRepoData(selectedRepo, { author }); }}
            onCherryPick={(hash) => void flow.runSimple(() => api.cherryPick(selectedRepo, hash))}
            onInspectCommit={(h, f) => {
              data.setSelectedCommit(h); data.setDiffMode('commit'); data.setFocusFile(f ?? null); data.setTab('diff');
              void data.refreshRepoData(selectedRepo, { mode: 'commit', commitHash: h });
            }}
            onNotify={setNotice}
          />

          <PrPanel prData={data.prData} currentBranch={status.branch} baseBranch={data.mergeTarget || 'main'} />

          <StashPanel
            stashes={data.stashes}
            isDirty={status.is_dirty}
            busy={busy}
            onSave={(m) => void flow.runSimple(() => api.stashSave(selectedRepo, m))}
            onApply={(idx) => void flow.runSimple(() => api.stashApply(selectedRepo, idx))}
            onPop={(idx) => void flow.runSimple(() => api.stashPopAt(selectedRepo, idx))}
            onDrop={(idx) => void flow.runSimple(() => api.stashDrop(selectedRepo, idx))}
          />
        </>
      )}

      <NoticeFooter notice={notice} error={error} onClearNotice={() => setNotice('')} />
    </div>
  );
}
