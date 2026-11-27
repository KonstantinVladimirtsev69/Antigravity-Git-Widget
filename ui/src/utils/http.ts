export class ApiError extends Error {
  readonly status: number;
  readonly body?: unknown;

  constructor(status: number, message: string, body?: unknown) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.body = body;
  }
}

const params = new URLSearchParams(window.location.search);
const SIDE_TOKEN = params.get('token') ?? '';
export const CONVERSATION_ID = params.get('conversationId') ?? '';

export async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  headers.set('Content-Type', 'application/json');
  if (SIDE_TOKEN) {
    headers.set('X-Sidecar-Token', SIDE_TOKEN);
  }
  const res = await fetch(path, { ...init, headers });
  const text = await res.text();
  if (!res.ok) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch {
      // raw text
    }
    const msg = (parsed as { error?: string } | undefined)?.error || text || `HTTP ${res.status}`;
    throw new ApiError(res.status, msg, parsed);
  }
  return JSON.parse(text) as T;
}
