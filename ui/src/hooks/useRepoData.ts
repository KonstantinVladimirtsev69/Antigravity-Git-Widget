import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api } from '../api';
import type {
  CommitInfo,
  DiffFile,
  GraphResponse,
  IntegrationMode,
  PrListResponse,
  RepoInfo,
  RepoStatus,
  ScannerConfig,
  StashEntry,
} from '../types';

export const VALID_MODES: IntegrationMode[] = ['merge', 'merge_no_ff', 'squash', 'rebase', 'ff_only'];
export const MODE_STORAGE_KEY = 'gw.integrationMode';

export interface RefreshOptions {
  mode?: 'working' | 'branch' | 'commit';
  commitHash?: string;
  allBranches?: boolean;
  author?: string;
  lightweight?: boolean;
}

function statusSignature(st: RepoStatus): string {
  return [
    st.branch,
    st.upstream ?? '',
    st.ahead,
    st.behind,
    st.is_dirty,
    st.is_merging,
    st.is_rebasing,
    st.changes.length,
    st.local_branches.join(','),
    st.remote_branches.join(','),
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

  const activeRepoRef = useRef(selectedRepo);
  activeRepoRef.current = selectedRepo;
  const inFlightRef = useRef(false);
  const lastSigRef = useRef<string>('');
  const lastFetchAtRef = useRef<number>(0);

  const applyStatusDefaults = useCallback((st: RepoStatus) => {
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
  }, []);

  const refreshRepoData = useCallback(
    async (repoPath: string, opts?: RefreshOptions) => {
      if (!repoPath) return;
      if (opts?.lightweight && inFlightRef.current) return;
      inFlightRef.current = true;
      const mode = opts?.mode ?? diffMode;
      const commitHash = opts?.commitHash ?? selectedCommit;
      const allBranches = opts?.allBranches ?? graphAll;
      const author = opts?.author ?? graphAuthor;

      try {
        const [st, sl] = await Promise.all([api.getStatus(repoPath), api.listStashes(repoPath)]);
        if (activeRepoRef.current !== repoPath) return;

        const sig = statusSignature(st);
        const sigChanged = sig !== lastSigRef.current;
        lastSigRef.current = sig;

        setStatus(st);
        setStashes(sl);
        applyStatusDefaults(st);

        if (opts?.lightweight && !sigChanged) return;

        const needCommits = tab === 'diff' || mode === 'commit';
        const needDiff = tab === 'diff' || mode !== 'working' || st.changes.length > 0;

        const [gr, cm, pr] = await Promise.all([
          api.getGraph(repoPath, allBranches, author || undefined),
          needCommits ? api.getCommits(repoPath) : Promise.resolve<CommitInfo[] | null>(null),
          st.remotes.length > 0
            ? api.getPrs(repoPath).catch(() => null)
            : Promise.resolve<PrListResponse | null>(null),
        ]);
        if (activeRepoRef.current !== repoPath) return;

        setGraphData(gr);
        setPrData(pr);
        if (cm) {
          setCommits(cm);
          if (!commitHash && cm[0]?.hash) setSelectedCommit(cm[0].hash);
        }

        const effectiveCommit = commitHash || cm?.[0]?.hash || '';
        if (needDiff) {
          const df = await api.getDiff(
            repoPath,
            mode,
            mode === 'commit' ? effectiveCommit : undefined,
          );
          if (activeRepoRef.current !== repoPath) return;
          setDiffFiles(df);
        } else {
          setDiffFiles([]);
        }
      } catch (e) {
        if (activeRepoRef.current === repoPath) {
          onError(e instanceof Error ? e.message : String(e));
        }
      } finally {
        inFlightRef.current = false;
      }
    },
    [diffMode, selectedCommit, graphAll, graphAuthor, tab, applyStatusDefaults, onError],
  );

  useEffect(() => {
    if (!selectedRepo) return;
    onClearMessages();
    setSelectedCommit('');
    setGraphAuthor('');
    lastSigRef.current = '';
    void refreshRepoData(selectedRepo, { commitHash: '', author: '' });
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
    const doSilentFetch = () => {
      if (document.visibilityState !== 'visible' || busy || inFlightRef.current) return;
      lastFetchAtRef.current = Date.now();
      void api
        .fetchAll(repo, selectedRemote || undefined)
        .then(() => {
          if (activeRepoRef.current === repo) {
            return refreshRepoData(repo, { lightweight: true });
          }
        })
        .catch(() => {});
    };
    const initial = window.setTimeout(doSilentFetch, 1500);
    const id = window.setInterval(doSilentFetch, 60000);
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

  return {
    tab,
    setTab,
    repos,
    setRepos,
    config,
    setConfig,
    selectedRepo,
    setSelectedRepo,
    autoMatched,
    setAutoMatched,
    status,
    diffFiles,
    commits,
    stashes,
    graphData,
    graphAll,
    setGraphAll,
    graphAuthor,
    setGraphAuthor,
    prData,
    selectedCommit,
    setSelectedCommit,
    diffMode,
    setDiffMode,
    selectedRemote,
    setSelectedRemote,
    mergeTarget,
    setMergeTarget,
    focusFile,
    setFocusFile,
    repoOptions,
    activeRepoRef,
    lastFetchAtRef,
    refreshRepoData,
  };
}
