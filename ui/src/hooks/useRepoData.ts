import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api } from '../api';
import { isCurrentRemoteRequest, loadSavedRemote, resolveDefaultRemote, saveSelectedRemote } from '../utils/remoteFocus';
import { loadAdHocRepos, mergeRepoOptions, normalizeWinPath, saveAdHocRepos } from '../utils/repoOptions';
import type {
  CommitInfo, DiffFile, GraphResponse, IntegrationMode,
  PrListResponse, RepoInfo, RepoStatus, ScannerConfig, StashEntry,
} from '../types';

export const VALID_MODES: IntegrationMode[] = ['merge', 'merge_no_ff', 'squash', 'rebase', 'ff_only'];
export const MODE_STORAGE_KEY = 'gw.integrationMode';

export interface RefreshOptions {
  mode?: 'working' | 'branch' | 'commit';
  commitHash?: string;
  allBranches?: boolean;
  author?: string;
  remote?: string;
  lightweight?: boolean;
}

function statusSignature(st: RepoStatus): string {
  return [
    st.branch, st.upstream ?? '', st.configured_upstream ?? '',
    st.ahead, st.behind, st.is_dirty, st.is_merging, st.is_rebasing,
    st.changes.length, st.remotes.join(','), st.local_branches.join(','), st.remote_branches.join(','),
  ].join('|');
}

export function useRepoData(
  busy: boolean,
  modalOpen: boolean,
  onError: (msg: string) => void,
  onClearMessages: () => void,
) {
  const [tab, setTab] = useState<'flow' | 'diff'>('flow');
  const [repos, setRepos] = useState<RepoInfo[]>([]);
  const [adHocRepos, setAdHocRepos] = useState<RepoInfo[]>(() => loadAdHocRepos());
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
  const [selectedRemote, setSelectedRemote] = useState<string>('');
  const [mergeTarget, setMergeTarget] = useState<string>('');
  const [focusFile, setFocusFile] = useState<string | null>(null);
  const [fetchWarning, setFetchWarning] = useState<string | null>(null);

  const activeRepoRef = useRef(selectedRepo);
  activeRepoRef.current = selectedRepo;
  const activeRemoteRef = useRef(selectedRemote);
  activeRemoteRef.current = selectedRemote;
  const requestSeqRef = useRef(0);
  const inFlightRef = useRef(false);
  const lastSigRef = useRef<string>('');
  const lastFetchAtRef = useRef<number>(0);

  const clearFetchWarning = useCallback(() => setFetchWarning(null), []);
  const checkCurrent = useCallback(
    (repo: string, remote: string, seq: number) =>
      isCurrentRemoteRequest(
        { repo, remote, seq },
        { repo: activeRepoRef.current, remote: activeRemoteRef.current, seq: requestSeqRef.current },
      ),
    [],
  );

  const applyMergeTargetDefault = useCallback((st: RepoStatus) => {
    setMergeTarget((prev) => {
      if (prev && st.local_branches.includes(prev) && prev !== st.branch) return prev;
      if (st.local_branches.includes('main') && st.branch !== 'main') return 'main';
      if (st.local_branches.includes('developer') && st.branch !== 'developer') return 'developer';
      return st.local_branches.find((b) => b !== st.branch) ?? '';
    });
  }, []);

  const loadSecondaryData = useCallback(
    async (
      repoPath: string,
      st: RepoStatus,
      remote: string,
      opts: { mode: 'working' | 'branch' | 'commit'; commitHash: string; allBranches: boolean; author: string },
      isCurrent: () => boolean,
    ) => {
      const needCommits = tab === 'diff' || opts.mode === 'commit';
      const needDiff = tab === 'diff' || opts.mode !== 'working' || st.changes.length > 0;
      const [gr, cm, pr] = await Promise.all([
        api.getGraph(repoPath, opts.allBranches, opts.author || undefined, remote || undefined),
        needCommits ? api.getCommits(repoPath) : Promise.resolve<CommitInfo[] | null>(null),
        st.remotes.length > 0 ? api.getPrs(repoPath, remote || undefined).catch(() => null) : Promise.resolve<PrListResponse | null>(null),
      ]);
      if (!isCurrent()) return;
      setGraphData(gr);
      setPrData(pr);
      if (cm) {
        setCommits(cm);
        if (!opts.commitHash && cm[0]?.hash) setSelectedCommit(cm[0].hash);
      }
      const effectiveCommit = opts.commitHash || cm?.[0]?.hash || '';
      if (needDiff) {
        const df = await api.getDiff(repoPath, opts.mode, opts.mode === 'commit' ? effectiveCommit : undefined);
        if (!isCurrent()) return;
        setDiffFiles(df);
      } else {
        setDiffFiles([]);
      }
    },
    [tab],
  );

  const refreshRepoData = useCallback(
    async (repoPath: string, opts?: RefreshOptions) => {
      if (!repoPath || (opts?.lightweight && inFlightRef.current)) return;
      inFlightRef.current = true;
      const seq = opts?.lightweight ? requestSeqRef.current : ++requestSeqRef.current;
      const initialRemote = opts?.remote !== undefined ? opts.remote : activeRemoteRef.current;
      const mode = opts?.mode ?? diffMode;
      const commitHash = opts?.commitHash ?? selectedCommit;
      const allBranches = opts?.allBranches ?? graphAll;
      const author = opts?.author ?? graphAuthor;

      let targetRemote = initialRemote;
      const isCurrent = () => checkCurrent(repoPath, targetRemote, seq);

      try {
        const [initialStatus, sl] = await Promise.all([
          api.getStatus(repoPath, initialRemote || undefined),
          api.listStashes(repoPath),
        ]);
        if (!isCurrent()) return;

        let st = initialStatus;
        const resolvedRemote = resolveDefaultRemote({
          savedRemote: initialRemote || loadSavedRemote(repoPath),
          upstream: st.upstream,
          configuredUpstream: st.configured_upstream,
          remotes: st.remotes,
        });
        if (resolvedRemote !== initialRemote) {
          if (resolvedRemote !== '') {
            st = await api.getStatus(repoPath, resolvedRemote);
            if (!isCurrent()) return;
          }
          targetRemote = resolvedRemote;
          activeRemoteRef.current = resolvedRemote;
          setSelectedRemote(resolvedRemote);
        } else if (resolvedRemote !== selectedRemote) {
          setSelectedRemote(resolvedRemote);
        }
        if (resolvedRemote) saveSelectedRemote(repoPath, resolvedRemote);

        const sig = statusSignature(st);
        const sigChanged = sig !== lastSigRef.current;
        lastSigRef.current = sig;
        setStatus(st);
        setStashes(sl);
        applyMergeTargetDefault(st);

        if (opts?.lightweight && !sigChanged) return;
        await loadSecondaryData(repoPath, st, resolvedRemote, { mode, commitHash, allBranches, author }, isCurrent);
      } catch (e) {
        if (isCurrent()) onError(e instanceof Error ? e.message : String(e));
      } finally {
        inFlightRef.current = false;
      }
    },
    [diffMode, selectedCommit, graphAll, graphAuthor, selectedRemote, checkCurrent, applyMergeTargetDefault, loadSecondaryData, onError],
  );

  const switchRemote = useCallback(
    async (nextRemote: string) => {
      const repo = activeRepoRef.current;
      if (!repo || !nextRemote) return;
      const seq = ++requestSeqRef.current;
      activeRemoteRef.current = nextRemote;
      setSelectedRemote(nextRemote);
      saveSelectedRemote(repo, nextRemote);
      setFetchWarning(null);
      lastSigRef.current = '';
      const isCurrent = () => checkCurrent(repo, nextRemote, seq);

      try {
        const res = await api.switchRemote(repo, nextRemote);
        lastFetchAtRef.current = Date.now();
        if (!isCurrent()) return;
        setFetchWarning(res.fetch_warning ?? null);
        lastSigRef.current = statusSignature(res.status);
        setStatus(res.status);
        applyMergeTargetDefault(res.status);
        await loadSecondaryData(
          repo,
          res.status,
          nextRemote,
          { mode: diffMode, commitHash: selectedCommit, allBranches: graphAll, author: graphAuthor },
          isCurrent,
        );
      } catch (e) {
        if (isCurrent()) onError(e instanceof Error ? e.message : String(e));
      }
    },
    [diffMode, selectedCommit, graphAll, graphAuthor, checkCurrent, applyMergeTargetDefault, loadSecondaryData, onError],
  );

  useEffect(() => {
    if (!selectedRepo) {
      setStatus(null);
      setSelectedRemote('');
      activeRemoteRef.current = '';
      setFetchWarning(null);
      return;
    }
    onClearMessages();
    setFetchWarning(null);
    setSelectedCommit('');
    setGraphAuthor('');
    lastSigRef.current = '';
    const saved = loadSavedRemote(selectedRepo) ?? '';
    activeRemoteRef.current = saved;
    setSelectedRemote(saved);
    void refreshRepoData(selectedRepo, { commitHash: '', author: '', remote: saved });
  }, [selectedRepo]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (!selectedRepo) return;
    const id = window.setInterval(() => {
      if (document.visibilityState === 'visible' && !busy && !modalOpen) {
        void refreshRepoData(selectedRepo, { lightweight: true });
      }
    }, 8000);
    return () => window.clearInterval(id);
  }, [selectedRepo, busy, modalOpen, refreshRepoData]);

  useEffect(() => {
    if (!selectedRepo || !status || status.remotes.length === 0) return;
    const repo = selectedRepo;
    const remote = selectedRemote;
    const doSilentFetch = () => {
      if (document.visibilityState !== 'visible' || busy || inFlightRef.current) return;
      if (Date.now() - lastFetchAtRef.current < 15000) return;
      lastFetchAtRef.current = Date.now();
      const seq = requestSeqRef.current;
      void api.fetchAll(repo, activeRemoteRef.current || undefined).then(() => {
        if (checkCurrent(repo, remote, seq)) {
          return refreshRepoData(repo, { lightweight: true, remote: activeRemoteRef.current });
        }
      }).catch(() => {});
    };
    const initial = window.setTimeout(doSilentFetch, 1500);
    const id = window.setInterval(doSilentFetch, 60000);
    return () => { window.clearTimeout(initial); window.clearInterval(id); };
  }, [selectedRepo, selectedRemote, status?.remotes.length]); // eslint-disable-line react-hooks/exhaustive-deps

  const addAdHocRepo = useCallback((repoPath: string) => {
    const trimmed = repoPath.trim().replace(/[\\/]+$/, '');
    const norm = normalizeWinPath(trimmed);
    if (!norm) return;
    const name = trimmed.split(/[\\/]/).pop() || trimmed;
    setAdHocRepos((prev) => {
      if (prev.some((r) => normalizeWinPath(r.path) === norm)) return prev;
      const next = [...prev, { name, path: trimmed, is_worktree: false }];
      saveAdHocRepos(next);
      return next;
    });
  }, []);

  const repoOptions = useMemo(() => mergeRepoOptions(repos, adHocRepos), [repos, adHocRepos]);

  return {
    tab, setTab, repos, setRepos, addAdHocRepo, config, setConfig,
    selectedRepo, setSelectedRepo, autoMatched, setAutoMatched,
    status, diffFiles, commits, stashes, graphData,
    graphAll, setGraphAll, graphAuthor, setGraphAuthor,
    prData, selectedCommit, setSelectedCommit, diffMode, setDiffMode,
    selectedRemote, setSelectedRemote, switchRemote, fetchWarning, clearFetchWarning,
    mergeTarget, setMergeTarget, focusFile, setFocusFile,
    repoOptions, activeRepoRef, lastFetchAtRef, refreshRepoData,
  };
}
