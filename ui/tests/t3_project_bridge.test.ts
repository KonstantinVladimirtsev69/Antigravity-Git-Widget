import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import vm from 'node:vm';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..');

interface RecordedCall {
  url: string;
  options: {
    method?: string;
    credentials?: string;
    headers?: Record<string, string>;
    body?: string;
  };
}

function mountBridge({
  desktopHttpBase,
  bearerToken,
  existingProjects = [],
}: {
  desktopHttpBase?: string;
  bearerToken?: string;
  existingProjects?: Array<{ id: string; workspaceRoot: string; archivedAt?: string | null; deletedAt?: string | null }>;
}) {
  const src = fs.readFileSync(
    path.join(ROOT, 'crates', 'git-widget-sidecar', 'assets', 't3-project-bridge.js'),
    'utf8',
  );
  const listeners: Record<string, Array<(ev: unknown) => void>> = {};
  const postedToIframe: unknown[] = [];
  const toasts: string[] = [];
  const fetchCalls: RecordedCall[] = [];
  const iframeWin = {
    postMessage: (msg: unknown) => {
      postedToIframe.push(msg);
    },
  };

  const sandbox: Record<string, unknown> = {
    IFRAME_ORIGIN: 'http://127.0.0.1:58115',
    iframeEl: { contentWindow: iframeWin },
    showToast: (t: string) => {
      toasts.push(t);
    },
    URL,
    crypto: { randomUUID: () => '11111111-1111-4111-8111-111111111111' },
    navigator: { clipboard: { writeText: async () => {} } },
    document: { querySelector: () => null },
    Event: class {},
    window: {
      addEventListener: (ev: string, fn: (e: unknown) => void) => {
        (listeners[ev] ||= []).push(fn);
      },
      desktopBridge: desktopHttpBase
        ? {
            getLocalEnvironmentBootstraps: () => [
              { id: 'primary', httpBaseUrl: desktopHttpBase },
            ],
            getLocalEnvironmentBearerToken: async () => bearerToken ?? '',
          }
        : undefined,
    },
    fetch: async (url: string, options: RecordedCall['options'] = {}) => {
      fetchCalls.push({ url, options });
      if (desktopHttpBase && url.startsWith('/')) {
        throw new TypeError(`Relative fetch failed on t3code://app for ${url}`);
      }
      if (url.endsWith('/api/projects')) {
        return {
          ok: true,
          status: 200,
          json: async () => ({ projects: existingProjects }),
        };
      }
      if (url.endsWith('/api/projects/mutate')) {
        return {
          ok: true,
          status: 200,
          json: async () => ({ id: 'proj-1' }),
          text: async () => '{}',
        };
      }
      return { ok: false, status: 404, text: async () => 'Not found' };
    },
  };

  vm.runInNewContext(src, sandbox);
  return {
    sendAddProject: async (repoPath: string, requestId = 'req-1') => {
      for (const fn of listeners['message'] ?? []) {
        await fn({
          origin: 'http://127.0.0.1:58115',
          source: iframeWin,
          data: { type: 'git-widget-add-project', requestId, path: repoPath },
        });
      }
      await new Promise((r) => setTimeout(r, 20));
    },
    fetchCalls,
    postedToIframe,
    toasts,
  };
}

test('Add to T3 Code uses desktopBridge httpBaseUrl and Bearer token on T3 Code Desktop', async () => {
  const ctx = mountBridge({
    desktopHttpBase: 'http://127.0.0.1:41234/',
    bearerToken: 'secret-desktop-bearer',
  });
  await ctx.sendAddProject('C:\\Projects\\my-app');

  assert.equal(ctx.fetchCalls.length, 2);
  assert.equal(ctx.fetchCalls[0]?.url, 'http://127.0.0.1:41234/api/projects');
  assert.equal(ctx.fetchCalls[0]?.options.headers?.Authorization, 'Bearer secret-desktop-bearer');
  assert.equal(ctx.fetchCalls[0]?.options.credentials, 'include');
  assert.equal(ctx.fetchCalls[1]?.url, 'http://127.0.0.1:41234/api/projects/mutate');
  assert.equal(ctx.fetchCalls[1]?.options.headers?.Authorization, 'Bearer secret-desktop-bearer');

  const reply = ctx.postedToIframe.find(
    (m) => (m as { type?: string }).type === 'git-widget-add-project-result',
  ) as { status?: string } | undefined;
  assert.equal(reply?.status, 'added');
});

test('Add to T3 Code ignores soft-deleted projects and uses relative same-origin on web', async () => {
  const ctx = mountBridge({
    existingProjects: [
      { id: 'old-del', workspaceRoot: 'C:\\Projects\\my-app', deletedAt: '2026-11-01T00:00:00Z' },
    ],
  });
  await ctx.sendAddProject('C:\\Projects\\my-app');

  assert.equal(ctx.fetchCalls.length, 2);
  assert.equal(ctx.fetchCalls[0]?.url, '/api/projects');
  assert.equal(ctx.fetchCalls[0]?.options.credentials, 'same-origin');
  assert.equal(ctx.fetchCalls[1]?.url, '/api/projects/mutate');
});
