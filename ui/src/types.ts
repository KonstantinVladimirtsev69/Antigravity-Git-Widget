export interface RepoInfo {
  name: string;
  path: string;
  is_worktree: boolean;
}

export interface ScannerConfig {
  roots: string[];
  max_depth: number;
}

export interface ReposResponse {
  repos: RepoInfo[];
  config: ScannerConfig;
}

export interface FileChange {
  path: string;
  code: string;
  is_conflicted: boolean;
}

export interface RepoStatus {
  branch: string;
  upstream: string | null;
  ahead: number;
  behind: number;
  is_dirty: boolean;
  is_merging: boolean;
  changes: FileChange[];
  conflicts: string[];
  local_branches: string[];
  remote_branches: string[];
}

export interface OpResult {
  success: boolean;
  message: string;
  has_conflicts: boolean;
}

export interface GuardedOpOutcome {
  blocked_dirty: boolean;
  dirty_files: string[];
  op_result: OpResult | null;
  stash_restored: boolean;
}

export interface ActionPreview {
  source: string;
  target: string;
  is_protected: boolean;
  commit_count: number;
  commits: string[];
}

export interface ConflictChatResponse {
  sent: boolean;
  prompt: string;
  error?: string | null;
}

export type GuardedOpKind = 'checkout' | 'pull' | 'merge';

export type DirtyStrategy =
  | { mode: 'fail_if_dirty' }
  | { mode: 'stash_and_pop' }
  | { mode: 'quick_commit'; message: string };
