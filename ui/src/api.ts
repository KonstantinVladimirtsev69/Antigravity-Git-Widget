import type {
  ActionPreview,
  CommitDetails,
  CommitInfo,
  ConflictChatResponse,
  DiffFile,
  DirtyStrategy,
  DualBranchSync,
  GraphResponse,
  GuardedOpKind,
  GuardedOpOutcome,
  IntegrationMode,
  OpResult,
  PrListResponse,
  RepoStatus,
  ReposResponse,
  ScannerConfig,
  StashEntry,
} from './types';

const params = new URLSearchParams(window.location.search);
const SIDE_TOKEN = params.get('token') ?? '';
export const CONVERSATION_ID = params.get('conversationId') ?? '';

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  headers.set('Content-Type', 'application/json');
  if (SIDE_TOKEN) {
    headers.set('X-Sidecar-Token', SIDE_TOKEN);
  }
  const res = await fetch(path, { ...init, headers });
  const text = await res.text();
  if (!res.ok) {
    let msg = text;
    try {
      const parsed = JSON.parse(text) as { error?: string };
      if (parsed.error) msg = parsed.error;
    } catch {
      // raw text
    }
    throw new Error(msg || `HTTP ${res.status}`);
  }
  return JSON.parse(text) as T;
}

export const api = {
  getRepos: () => request<ReposResponse>('./api/repos'),
  saveConfig: (config: ScannerConfig) =>
    request<ReposResponse>('./api/config', { method: 'POST', body: JSON.stringify(config) }),
  saveIntegrationMode: (mode: IntegrationMode) =>
    request<{ mode: IntegrationMode }>('./api/config/integration-mode', {
      method: 'POST',
      body: JSON.stringify({ mode }),
    }),
  resolveWorkspace: (workspaceUris: string[]) =>
    request<{ matched_path: string | null }>('./api/resolve-workspace', {
      method: 'POST',
      body: JSON.stringify({ workspace_uris: workspaceUris }),
    }),
  getStatus: (repo: string) => request<RepoStatus>(`./api/status?repo=${encodeURIComponent(repo)}`),
  runGuarded: (repo: string, kind: GuardedOpKind, target: string | null, strategy: DirtyStrategy) =>
    request<GuardedOpOutcome>('./api/git/guarded', {
      method: 'POST',
      body: JSON.stringify({ repo, kind, target, strategy }),
    }),
  fetchAll: (repo: string, remote?: string) =>
    request<OpResult>('./api/git/fetch', { method: 'POST', body: JSON.stringify({ repo, remote }) }),
  pull: (repo: string, remote?: string) =>
    request<OpResult>('./api/git/pull', { method: 'POST', body: JSON.stringify({ repo, remote }) }),
  push: (repo: string, remote?: string) =>
    request<OpResult>('./api/git/push', { method: 'POST', body: JSON.stringify({ repo, remote }) }),
  publish: (repo: string, remote?: string) =>
    request<OpResult>('./api/git/publish', { method: 'POST', body: JSON.stringify({ repo, remote }) }),
  addRemote: (repo: string, name: string, url: string) =>
    request<OpResult>('./api/git/add-remote', { method: 'POST', body: JSON.stringify({ repo, name, url }) }),
  createGithubRepo: (
    repo: string,
    payload: {
      name: string;
      description: string;
      private: boolean;
      use_ssh: boolean;
      token?: string;
    },
  ) =>
    request<OpResult>('./api/github/create-repo', {
      method: 'POST',
      body: JSON.stringify({ repo, ...payload }),
    }),
  initRepo: (repo: string) =>
    request<OpResult>('./api/git/init-repo', { method: 'POST', body: JSON.stringify({ repo }) }),
  commitAll: (repo: string, message: string, amend = false) =>
    request<OpResult>('./api/git/commit', {
      method: 'POST',
      body: JSON.stringify({ repo, message, amend }),
    }),
  stashSave: (repo: string, message?: string) =>
    request<OpResult>('./api/git/stash-save', { method: 'POST', body: JSON.stringify({ repo, message }) }),
  stashPop: (repo: string) =>
    request<OpResult>('./api/git/stash-pop', { method: 'POST', body: JSON.stringify({ repo }) }),
  listStashes: (repo: string) =>
    request<StashEntry[]>(`./api/git/stashes?repo=${encodeURIComponent(repo)}`),
  stashApply: (repo: string, index: number) =>
    request<OpResult>('./api/git/stash-apply', { method: 'POST', body: JSON.stringify({ repo, index }) }),
  stashPopAt: (repo: string, index: number) =>
    request<OpResult>('./api/git/stash-pop-at', { method: 'POST', body: JSON.stringify({ repo, index }) }),
  stashDrop: (repo: string, index: number) =>
    request<OpResult>('./api/git/stash-drop', { method: 'POST', body: JSON.stringify({ repo, index }) }),
  createBranch: (repo: string, name: string) =>
    request<OpResult>('./api/git/branch-create', { method: 'POST', body: JSON.stringify({ repo, name }) }),
  deleteBranch: (repo: string, name: string, force = false) =>
    request<OpResult>('./api/git/branch-delete', {
      method: 'POST',
      body: JSON.stringify({ repo, name, force }),
    }),
  checkoutRemote: (repo: string, remoteBranch: string) =>
    request<OpResult>('./api/git/branch-checkout-remote', {
      method: 'POST',
      body: JSON.stringify({ repo, remote_branch: remoteBranch }),
    }),
  undoCommit: (repo: string) =>
    request<OpResult>('./api/git/undo-commit', { method: 'POST', body: JSON.stringify({ repo }) }),
  cherryPick: (repo: string, hash: string) =>
    request<OpResult>('./api/git/cherry-pick', { method: 'POST', body: JSON.stringify({ repo, hash }) }),
  stageAll: (repo: string) =>
    request<OpResult>('./api/git/stage-all', { method: 'POST', body: JSON.stringify({ repo }) }),
  unstageAll: (repo: string) =>
    request<OpResult>('./api/git/unstage-all', { method: 'POST', body: JSON.stringify({ repo }) }),
  discardAll: (repo: string) =>
    request<OpResult>('./api/git/discard-all', { method: 'POST', body: JSON.stringify({ repo }) }),
  abortMerge: (repo: string) =>
    request<OpResult>('./api/git/abort-merge', { method: 'POST', body: JSON.stringify({ repo }) }),
  abortRebase: (repo: string) =>
    request<OpResult>('./api/git/abort-rebase', { method: 'POST', body: JSON.stringify({ repo }) }),
  continueRebase: (repo: string) =>
    request<OpResult>('./api/git/continue-rebase', { method: 'POST', body: JSON.stringify({ repo }) }),
  scanSync: (
    repo: string,
    source: string,
    target: string,
    remote?: string,
    doFetch = true,
  ) =>
    request<DualBranchSync>('./api/git/scan-sync', {
      method: 'POST',
      body: JSON.stringify({ repo, source, target, remote, fetch: doFetch }),
    }),
  pullBoth: (repo: string, source: string, target: string, remote?: string) =>
    request<OpResult>('./api/git/pull-both', {
      method: 'POST',
      body: JSON.stringify({ repo, source, target, remote }),
    }),
  previewIntegrate: (
    repo: string,
    source: string,
    target: string,
    mode: IntegrationMode,
  ) =>
    request<ActionPreview>('./api/git/preview-integrate', {
      method: 'POST',
      body: JSON.stringify({ repo, source, target, mode }),
    }),
  integrate: (
    repo: string,
    source: string,
    target: string,
    mode: IntegrationMode,
    strategy: DirtyStrategy,
  ) =>
    request<GuardedOpOutcome>('./api/git/integrate', {
      method: 'POST',
      body: JSON.stringify({ repo, source, target, mode, strategy }),
    }),
  previewMerge: (repo: string, source: string) =>
    request<ActionPreview>('./api/git/preview-merge', { method: 'POST', body: JSON.stringify({ repo, source }) }),
  previewPush: (repo: string) =>
    request<ActionPreview>('./api/git/preview-push', { method: 'POST', body: JSON.stringify({ repo }) }),
  sendConflictToChat: (repo: string, conversationId: string) =>
    request<ConflictChatResponse>('./api/git/send-conflict-to-chat', {
      method: 'POST',
      body: JSON.stringify({ repo, conversation_id: conversationId || null }),
    }),
  getCommits: (repo: string) =>
    request<CommitInfo[]>(`./api/commits?repo=${encodeURIComponent(repo)}`),
  getGraph: (repo: string, all = true, author?: string) =>
    request<GraphResponse>(
      `./api/graph?repo=${encodeURIComponent(repo)}&all=${all}${author ? `&author=${encodeURIComponent(author)}` : ''}`,
    ),
  getCommitDetails: (repo: string, hash: string) =>
    request<CommitDetails>(
      `./api/commit-details?repo=${encodeURIComponent(repo)}&hash=${encodeURIComponent(hash)}`,
    ),
  getPrs: (repo: string) =>
    request<PrListResponse>(`./api/prs?repo=${encodeURIComponent(repo)}`),
  suggestCommit: (repo: string, conversationId: string) =>
    request<ConflictChatResponse>('./api/ai/suggest-commit', {
      method: 'POST',
      body: JSON.stringify({ repo, conversation_id: conversationId || null }),
    }),
  getDiff: (repo: string, mode: 'working' | 'branch' | 'commit' = 'working', commit?: string) =>
    request<DiffFile[]>(
      `./api/diff?repo=${encodeURIComponent(repo)}&mode=${mode}${commit ? `&commit=${encodeURIComponent(commit)}` : ''}`,
    ),
  acceptHunk: (repo: string, file: string, hunkIndex: number) =>
    request<{ ok: boolean }>('./api/diff/accept-hunk', {
      method: 'POST',
      body: JSON.stringify({ repo, file, hunk_index: hunkIndex }),
    }),
  rejectHunk: (repo: string, file: string, hunkIndex: number) =>
    request<{ ok: boolean }>('./api/diff/reject-hunk', {
      method: 'POST',
      body: JSON.stringify({ repo, file, hunk_index: hunkIndex }),
    }),
  stageFile: (repo: string, file: string) =>
    request<{ ok: boolean }>('./api/diff/stage-file', { method: 'POST', body: JSON.stringify({ repo, file }) }),
  unstageFile: (repo: string, file: string) =>
    request<{ ok: boolean }>('./api/diff/unstage-file', { method: 'POST', body: JSON.stringify({ repo, file }) }),
  discardFile: (repo: string, file: string) =>
    request<{ ok: boolean }>('./api/diff/discard-file', { method: 'POST', body: JSON.stringify({ repo, file }) }),
  askAi: (repo: string, file: string, snippet: string, question: string, conversationId: string) =>
    request<ConflictChatResponse>('./api/diff/ask-ai', {
      method: 'POST',
      body: JSON.stringify({ repo, file, snippet, question, conversation_id: conversationId || null }),
    }),
};
