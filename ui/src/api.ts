import type {
  ActionPreview,
  ConflictChatResponse,
  DirtyStrategy,
  GuardedOpKind,
  GuardedOpOutcome,
  OpResult,
  RepoStatus,
  ReposResponse,
  ScannerConfig,
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
      // keep raw text
    }
    throw new Error(msg || `HTTP ${res.status}`);
  }
  return JSON.parse(text) as T;
}

export const api = {
  getRepos: () => request<ReposResponse>('./api/repos'),
  saveConfig: (config: ScannerConfig) =>
    request<ReposResponse>('./api/config', {
      method: 'POST',
      body: JSON.stringify(config),
    }),
  resolveWorkspace: (workspaceUris: string[]) =>
    request<{ matched_path: string | null }>('./api/resolve-workspace', {
      method: 'POST',
      body: JSON.stringify({ workspace_uris: workspaceUris }),
    }),
  getStatus: (repo: string) =>
    request<RepoStatus>(`./api/status?repo=${encodeURIComponent(repo)}`),
  runGuarded: (
    repo: string,
    kind: GuardedOpKind,
    target: string | null,
    strategy: DirtyStrategy,
  ) =>
    request<GuardedOpOutcome>('./api/git/guarded', {
      method: 'POST',
      body: JSON.stringify({ repo, kind, target, strategy }),
    }),
  fetchAll: (repo: string) =>
    request<OpResult>('./api/git/fetch', {
      method: 'POST',
      body: JSON.stringify({ repo }),
    }),
  push: (repo: string) =>
    request<OpResult>('./api/git/push', {
      method: 'POST',
      body: JSON.stringify({ repo }),
    }),
  commitAll: (repo: string, message: string) =>
    request<OpResult>('./api/git/commit', {
      method: 'POST',
      body: JSON.stringify({ repo, message }),
    }),
  stashSave: (repo: string, message?: string) =>
    request<OpResult>('./api/git/stash-save', {
      method: 'POST',
      body: JSON.stringify({ repo, message }),
    }),
  stashPop: (repo: string) =>
    request<OpResult>('./api/git/stash-pop', {
      method: 'POST',
      body: JSON.stringify({ repo }),
    }),
  abortMerge: (repo: string) =>
    request<OpResult>('./api/git/abort-merge', {
      method: 'POST',
      body: JSON.stringify({ repo }),
    }),
  previewMerge: (repo: string, source: string) =>
    request<ActionPreview>('./api/git/preview-merge', {
      method: 'POST',
      body: JSON.stringify({ repo, source }),
    }),
  previewPush: (repo: string) =>
    request<ActionPreview>('./api/git/preview-push', {
      method: 'POST',
      body: JSON.stringify({ repo }),
    }),
  sendConflictToChat: (repo: string, conversationId: string) =>
    request<ConflictChatResponse>('./api/git/send-conflict-to-chat', {
      method: 'POST',
      body: JSON.stringify({
        repo,
        conversation_id: conversationId || null,
      }),
    }),
};
