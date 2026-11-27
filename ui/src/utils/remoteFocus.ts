import { normalizeWinPath } from './repoOptions.ts';
import type { RepoStatus } from '../types.ts';

const STORAGE_PREFIX = 'agw.selectedRemote.v1:';

export interface RemoteRequestContext {
  repo: string;
  remote: string;
  seq: number;
}

/**
 * Loads the persisted remote name for the given repository path from localStorage.
 */
export function loadSavedRemote(repoPath: string): string | null {
  const norm = normalizeWinPath(repoPath);
  if (!norm || typeof window === 'undefined') return null;
  try {
    const raw = window.localStorage?.getItem(`${STORAGE_PREFIX}${norm}`);
    if (typeof raw !== 'string') return null;
    const trimmed = raw.trim();
    return trimmed.length > 0 ? trimmed : null;
  } catch {
    return null;
  }
}

/**
 * Persists the selected remote name for the given repository path in localStorage.
 */
export function saveSelectedRemote(repoPath: string, remote: string): void {
  const norm = normalizeWinPath(repoPath);
  const trimmed = remote.trim();
  if (!norm || !trimmed || typeof window === 'undefined') return;
  try {
    window.localStorage?.setItem(`${STORAGE_PREFIX}${norm}`, trimmed);
  } catch {
    // ignore storage quota or restricted context errors
  }
}

/**
 * Splits a remote tracking ref into `{ remote, branch }` using known remotes sorted by descending length.
 */
export function splitRemoteBranch(
  remoteBranch: string,
  remotes: string[],
): { remote: string; branch: string } | null {
  if (!remoteBranch || remotes.length === 0) return null;
  const sorted = [...remotes].filter((r) => r.length > 0).sort((a, b) => b.length - a.length);
  for (const r of sorted) {
    const prefix = `${r}/`;
    if (remoteBranch.startsWith(prefix) && remoteBranch.length > prefix.length) {
      return { remote: r, branch: remoteBranch.slice(prefix.length) };
    }
  }
  return null;
}

/**
 * Extracts the remote name prefix from an upstream ref using known remotes.
 */
export function extractRemotePrefix(
  upstream: string | null | undefined,
  remotes: string[],
): string | null {
  if (!upstream) return null;
  return splitRemoteBranch(upstream, remotes)?.remote ?? null;
}

/**
 * Resolves the default remote in priority order:
 * savedRemote -> configured/effective upstream prefix -> 'origin' -> remotes[0] -> ''.
 */
export function resolveDefaultRemote(opts: {
  savedRemote?: string | null;
  upstream?: string | null;
  configuredUpstream?: string | null;
  remotes: string[];
}): string {
  const { savedRemote, upstream, configuredUpstream, remotes } = opts;
  if (remotes.length === 0) return '';
  if (savedRemote && remotes.includes(savedRemote)) return savedRemote;

  const fromConfigured = extractRemotePrefix(configuredUpstream, remotes);
  if (fromConfigured && remotes.includes(fromConfigured)) return fromConfigured;

  const fromUpstream = extractRemotePrefix(upstream, remotes);
  if (fromUpstream && remotes.includes(fromUpstream)) return fromUpstream;

  if (remotes.includes('origin')) return 'origin';
  return remotes[0] ?? '';
}

/**
 * Filters remote branches strictly to `selectedRemote` and omits branches already checked out locally.
 */
export function filterRemoteBranchesForRemote(
  remoteBranches: string[],
  localBranches: string[],
  selectedRemote: string,
  remotes?: string[],
): string[] {
  if (!selectedRemote) return [];
  const localSet = new Set(localBranches);
  const candidates = remotes && remotes.length > 0 ? remotes : [selectedRemote];
  return remoteBranches.filter((rb) => {
    const parsed = splitRemoteBranch(rb, candidates);
    return parsed !== null && parsed.remote === selectedRemote && !localSet.has(parsed.branch);
  });
}

/**
 * Returns true strictly when an upstream exists and the branch is both ahead and behind.
 */
export function isDiverged(
  status: Pick<RepoStatus, 'upstream' | 'ahead' | 'behind'> | null | undefined,
): boolean {
  return Boolean(status && status.upstream && status.ahead > 0 && status.behind > 0);
}

/**
 * Guards async responses against stale repository, remote, or request sequence changes.
 */
export function isCurrentRemoteRequest(
  expected: RemoteRequestContext,
  current: RemoteRequestContext,
): boolean {
  return (
    expected.seq === current.seq &&
    expected.remote === current.remote &&
    normalizeWinPath(expected.repo) === normalizeWinPath(current.repo)
  );
}
