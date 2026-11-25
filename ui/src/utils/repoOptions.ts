import type { PickerOption } from '../Picker';
import type { RepoInfo } from '../types';

const ADHOC_STORAGE_KEY = 'gw.adHocRepos';

export function normalizeWinPath(p: string): string {
  return p.trim().replace(/\//g, '\\').replace(/\\+$/, '').toLowerCase();
}

export function loadAdHocRepos(): RepoInfo[] {
  try {
    const raw = window.sessionStorage.getItem(ADHOC_STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    const result: RepoInfo[] = [];
    for (const item of parsed) {
      if (
        typeof item === 'object' &&
        item !== null &&
        typeof (item as RepoInfo).path === 'string' &&
        typeof (item as RepoInfo).name === 'string' &&
        (item as RepoInfo).path.trim().length > 0
      ) {
        result.push({
          name: (item as RepoInfo).name,
          path: (item as RepoInfo).path,
          is_worktree: Boolean((item as RepoInfo).is_worktree),
        });
      }
    }
    return result;
  } catch {
    return [];
  }
}

export function saveAdHocRepos(items: RepoInfo[]): void {
  try {
    window.sessionStorage.setItem(ADHOC_STORAGE_KEY, JSON.stringify(items));
  } catch {
    // ignore storage quota / restricted context errors
  }
}

export function mergeRepoOptions(repos: RepoInfo[], adHocRepos: RepoInfo[]): PickerOption[] {
  const seen = new Set<string>();
  const merged: RepoInfo[] = [];
  for (const repo of repos) {
    const norm = normalizeWinPath(repo.path);
    if (!norm || seen.has(norm)) continue;
    seen.add(norm);
    merged.push(repo);
  }
  for (const repo of adHocRepos) {
    const norm = normalizeWinPath(repo.path);
    if (!norm || seen.has(norm)) continue;
    seen.add(norm);
    merged.push(repo);
  }
  return merged.map((r) => ({
    value: r.path,
    label: `${r.name}${r.is_worktree ? ' (wt)' : ''}`,
    sub: r.path,
  }));
}
