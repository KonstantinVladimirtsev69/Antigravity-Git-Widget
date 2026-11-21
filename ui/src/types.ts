export interface RepoInfo {
  name: string;
  path: string;
  is_worktree: boolean;
}

export type IntegrationMode = 'merge' | 'merge_no_ff' | 'squash' | 'rebase' | 'ff_only';

export interface BranchTracking {
  upstream: string | null;
  ahead: number;
  behind: number;
  gone: boolean;
}

export interface BranchSyncState {
  branch: string;
  upstream: string | null;
  ahead: number;
  behind: number;
  gone: boolean;
}

export interface DualBranchSync {
  source: BranchSyncState;
  target: BranchSyncState;
  any_behind: boolean;
  fetch_error: string | null;
}

export interface ScannerConfig {
  roots: string[];
  max_depth: number;
  github_token?: string | null;
  has_github_token?: boolean;
  integration_mode?: IntegrationMode;
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
  is_rebasing: boolean;
  changes: FileChange[];
  staged_changes: FileChange[];
  unstaged_changes: FileChange[];
  conflicts: string[];
  local_branches: string[];
  remote_branches: string[];
  remotes: string[];
  branch_tracking: Record<string, BranchTracking>;
}

export interface StashEntry {
  index: number;
  reference: string;
  message: string;
  date_rel: string;
}

export interface RefDecoration {
  name: string;
  kind: 'head' | 'local' | 'remote' | 'tag';
}

export interface GraphCommit {
  hash: string;
  short_hash: string;
  parents: string[];
  subject: string;
  author_name: string;
  author_email: string;
  date_rel: string;
  date_exact: string;
  refs: RefDecoration[];
  is_head: boolean;
}

export interface AuthorStat {
  name: string;
  count: number;
  last_date: string;
}

export interface GraphResponse {
  commits: GraphCommit[];
  authors: AuthorStat[];
  github_url: string | null;
}

export interface CommitFileStat {
  path: string;
  status: string;
  additions: number;
  deletions: number;
}

export interface CommitDetails {
  hash: string;
  files: CommitFileStat[];
  total_additions: number;
  total_deletions: number;
}

export interface PullRequestInfo {
  number: number;
  title: string;
  author: string;
  head_ref: string;
  base_ref: string;
  url: string;
  updated_at: string;
}

export interface PrListResponse {
  gh_available: boolean;
  github_url: string | null;
  prs: PullRequestInfo[];
}

export interface DiffHunk {
  index: number;
  header: string;
  lines: string[];
  raw_patch: string;
}

export interface DiffFile {
  path: string;
  status: string;
  staged: boolean;
  additions: number;
  deletions: number;
  hunks: DiffHunk[];
}

export interface CommitInfo {
  hash: string;
  short_hash: string;
  subject: string;
  author: string;
  date_rel: string;
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

export type GuardedOpKind = 'checkout' | 'pull' | 'merge' | 'integrate';

export type DirtyStrategy =
  | { mode: 'fail_if_dirty' }
  | { mode: 'stash_and_pop' }
  | { mode: 'quick_commit'; message: string };

export interface GithubDeviceFlowStart {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
}

export interface GithubDevicePollRes {
  status: 'pending' | 'authorized' | 'expired' | 'denied' | 'error';
  interval?: number | null;
  message?: string | null;
}
