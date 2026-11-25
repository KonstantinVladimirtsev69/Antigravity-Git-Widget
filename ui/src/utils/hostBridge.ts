import type { AddProjectResult } from '../types';

export const IS_T3_HOST =
  new URLSearchParams(window.location.search).get('host') === 't3code';

const ADD_PROJECT_TIMEOUT_MS = 5000;
const VALID_STATUSES = new Set<AddProjectResult['status']>([
  'added',
  'already_exists',
  'fallback',
  'error',
]);

function newRequestId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

export async function deliverPromptToHost(
  prompt: string,
  alreadySent: boolean,
  sentLabel = 'Sent to AI chat',
): Promise<string> {
  if (alreadySent) return sentLabel;
  const trimmed = prompt.trim();
  if (!trimmed) return 'Empty prompt ignored';
  if (IS_T3_HOST && window.parent && window.parent !== window) {
    window.parent.postMessage({ type: 'git-widget-ask-ai', prompt: trimmed }, '*');
    return 'Prompt sent to T3 Code composer';
  }
  await navigator.clipboard.writeText(trimmed);
  return 'Copied to clipboard (direct chat send unavailable)';
}

export function addProjectToT3Host(path: string): Promise<AddProjectResult> {
  const trimmed = path.trim();
  if (!trimmed) {
    return Promise.resolve({ status: 'error', message: 'Empty repository path' });
  }
  if (!IS_T3_HOST || !window.parent || window.parent === window) {
    return Promise.resolve({ status: 'error', message: 'Not inside T3 Code host' });
  }

  const requestId = newRequestId();
  return new Promise<AddProjectResult>((resolve) => {
    let settled = false;
    const cleanup = () => {
      window.removeEventListener('message', onMessage);
      window.clearTimeout(timer);
    };

    const onMessage = (event: MessageEvent) => {
      if (event.source !== window.parent) return;
      const data = event.data as {
        type?: unknown;
        requestId?: unknown;
        status?: unknown;
        message?: unknown;
      } | null;
      if (!data || data.type !== 'git-widget-add-project-result' || data.requestId !== requestId) {
        return;
      }
      if (settled) return;
      settled = true;
      cleanup();
      const rawStatus = typeof data.status === 'string' ? data.status : 'error';
      const status = VALID_STATUSES.has(rawStatus as AddProjectResult['status'])
        ? (rawStatus as AddProjectResult['status'])
        : 'error';
      const message = typeof data.message === 'string' ? data.message : '';
      resolve({ status, message });
    };

    const timer = window.setTimeout(() => {
      if (settled) return;
      settled = true;
      cleanup();
      void navigator.clipboard?.writeText(trimmed).catch(() => {});
      resolve({
        status: 'fallback',
        message: 'Path copied to clipboard — add project manually in T3 Code',
      });
    }, ADD_PROJECT_TIMEOUT_MS);

    window.addEventListener('message', onMessage);
    window.parent.postMessage({ type: 'git-widget-add-project', requestId, path: trimmed }, '*');
  });
}
