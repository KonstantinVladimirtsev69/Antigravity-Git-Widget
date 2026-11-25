import { afterEach, describe, it } from 'node:test';
import assert from 'node:assert/strict';

const originalWindowDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'window');
const originalNavigatorDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'navigator');

let importCounter = 0;

interface PostedMsg {
  data: Record<string, unknown>;
  origin: string;
}

interface MockBridgeEnv {
  posted: PostedMsg[];
  clipboardWrites: string[];
  listeners: Set<(event: MessageEvent) => void>;
  activeTimers: Map<number, { fn: () => void; ms: number }>;
  clearedTimers: number[];
  parentObj: { postMessage: (data: Record<string, unknown>, origin: string) => void };
  emitMessage: (source: unknown, data: unknown) => void;
  triggerNextTimeout: () => void;
}

function setupMockWindow(options: { search: string; selfParent?: boolean; clipboardFail?: boolean }): MockBridgeEnv {
  const posted: PostedMsg[] = [];
  const clipboardWrites: string[] = [];
  const listeners = new Set<(event: MessageEvent) => void>();
  const activeTimers = new Map<number, { fn: () => void; ms: number }>();
  const clearedTimers: number[] = [];
  let nextTimerId = 1;

  const parentObj = {
    postMessage: (data: Record<string, unknown>, origin: string) => {
      posted.push({ data, origin });
    },
  };

  const mockWindow: Record<string, unknown> = {
    location: { search: options.search },
    addEventListener: (type: string, listener: (event: MessageEvent) => void) => {
      if (type === 'message') listeners.add(listener);
    },
    removeEventListener: (type: string, listener: (event: MessageEvent) => void) => {
      if (type === 'message') listeners.delete(listener);
    },
    setTimeout: (fn: () => void, ms: number) => {
      const id = nextTimerId++;
      activeTimers.set(id, { fn, ms });
      return id;
    },
    clearTimeout: (id: number) => {
      clearedTimers.push(id);
      activeTimers.delete(id);
    },
  };

  mockWindow.parent = options.selfParent ? mockWindow : parentObj;
  Object.defineProperty(globalThis, 'window', { value: mockWindow, configurable: true, writable: true });
  Object.defineProperty(globalThis, 'navigator', {
    value: {
      clipboard: {
        writeText: async (text: string) => {
          if (options.clipboardFail) throw new Error('Clipboard denied');
          clipboardWrites.push(text);
        },
      },
    },
    configurable: true,
    writable: true,
  });

  return {
    posted, clipboardWrites, listeners, activeTimers, clearedTimers, parentObj,
    emitMessage: (source: unknown, data: unknown) => {
      for (const listener of [...listeners]) listener({ source, data } as MessageEvent);
    },
    triggerNextTimeout: () => {
      const first = activeTimers.entries().next();
      if (!first.done) {
        const [id, entry] = first.value;
        activeTimers.delete(id);
        entry.fn();
      }
    },
  };
}

async function loadFreshHostBridge() {
  importCounter += 1;
  return import(`../src/utils/hostBridge.ts?case=${importCounter}`);
}

afterEach(() => {
  if (originalWindowDescriptor) Object.defineProperty(globalThis, 'window', originalWindowDescriptor);
  else delete (globalThis as Record<string, unknown>).window;
  if (originalNavigatorDescriptor) Object.defineProperty(globalThis, 'navigator', originalNavigatorDescriptor);
  else delete (globalThis as Record<string, unknown>).navigator;
});

describe('hostBridge', () => {
  it('detects T3 Code host only when query param host=t3code is present', async () => {
    setupMockWindow({ search: '?host=t3code' });
    const t3 = await loadFreshHostBridge();
    assert.equal(t3.IS_T3_HOST, true);

    setupMockWindow({ search: '?host=antigravity' });
    const ag = await loadFreshHostBridge();
    assert.equal(ag.IS_T3_HOST, false);
  });

  it('rejects empty path, non-T3 host, and self-parent windows without posting', async () => {
    const t3Env = setupMockWindow({ search: '?host=t3code' });
    const t3 = await loadFreshHostBridge();
    assert.deepEqual(await t3.addProjectToT3Host('   '), {
      status: 'error',
      message: 'Empty repository path',
    });
    assert.equal(t3Env.posted.length, 0);
    assert.equal(t3Env.listeners.size, 0);

    const agEnv = setupMockWindow({ search: '' });
    const ag = await loadFreshHostBridge();
    assert.deepEqual(await ag.addProjectToT3Host('C:\\Projects\\repo'), {
      status: 'error',
      message: 'Not inside T3 Code host',
    });
    assert.equal(agEnv.posted.length, 0);

    const selfEnv = setupMockWindow({ search: '?host=t3code', selfParent: true });
    const selfBridge = await loadFreshHostBridge();
    assert.deepEqual(await selfBridge.addProjectToT3Host('C:\\Projects\\repo'), {
      status: 'error',
      message: 'Not inside T3 Code host',
    });
    assert.equal(selfEnv.posted.length, 0);
  });

  it('filters messages by event.source, type, and requestId and cleans up listener and timer on match', async () => {
    const env = setupMockWindow({ search: '?host=t3code' });
    const bridge = await loadFreshHostBridge();

    let settled = false;
    const promise = bridge.addProjectToT3Host('  C:\\Projects\\cloned-app  ').then((res) => {
      settled = true;
      return res;
    });

    assert.equal(env.posted.length, 1);
    const posted = env.posted[0];
    assert.equal(posted.origin, '*');
    assert.equal(posted.data.type, 'git-widget-add-project');
    assert.equal(posted.data.path, 'C:\\Projects\\cloned-app');
    assert.equal(typeof posted.data.requestId, 'string');
    const reqId = posted.data.requestId as string;
    assert.ok(reqId.length > 0);
    assert.equal(env.listeners.size, 1);
    assert.equal(env.activeTimers.size, 1);

    // Spoofed source, wrong type, and wrong requestId must all be ignored
    env.emitMessage({ fake: true }, { type: 'git-widget-add-project-result', requestId: reqId, status: 'added', message: 'Spoofed' });
    env.emitMessage(env.parentObj, { type: 'other-message', requestId: reqId, status: 'added' });
    env.emitMessage(env.parentObj, { type: 'git-widget-add-project-result', requestId: 'wrong-id', status: 'added' });

    await Promise.resolve();
    assert.equal(settled, false);
    assert.equal(env.listeners.size, 1);

    // Valid response resolves and cleans up listener + timer
    env.emitMessage(env.parentObj, {
      type: 'git-widget-add-project-result',
      requestId: reqId,
      status: 'already_exists',
      message: 'Project already in workspace',
    });

    const result = await promise;
    assert.deepEqual(result, { status: 'already_exists', message: 'Project already in workspace' });
    assert.equal(env.listeners.size, 0);
    assert.equal(env.activeTimers.size, 0);
    assert.equal(env.clearedTimers.length, 1);
  });

  it('coerces unknown response status values to error', async () => {
    const env = setupMockWindow({ search: '?host=t3code' });
    const bridge = await loadFreshHostBridge();

    const promise = bridge.addProjectToT3Host('C:\\Projects\\app');
    const reqId = env.posted[0].data.requestId;
    env.emitMessage(env.parentObj, {
      type: 'git-widget-add-project-result',
      requestId: reqId,
      status: 'unexpected_status',
      message: 123,
    });

    assert.deepEqual(await promise, { status: 'error', message: '' });
    assert.equal(env.listeners.size, 0);
  });

  it('falls back to clipboard after 5000ms timeout and removes listener', async () => {
    const env = setupMockWindow({ search: '?host=t3code' });
    const bridge = await loadFreshHostBridge();

    const promise = bridge.addProjectToT3Host('  C:\\Projects\\timeout-repo  ');
    const firstTimer = env.activeTimers.values().next().value;
    assert.equal(firstTimer?.ms, 5000);

    env.triggerNextTimeout();
    const result = await promise;

    assert.deepEqual(result, {
      status: 'fallback',
      message: 'Path copied to clipboard — add project manually in T3 Code',
    });
    assert.deepEqual(env.clipboardWrites, ['C:\\Projects\\timeout-repo']);
    assert.equal(env.listeners.size, 0);
    assert.equal(env.activeTimers.size, 0);
  });

  it('deliverPromptToHost posts to T3 parent or copies to clipboard outside T3', async () => {
    const t3Env = setupMockWindow({ search: '?host=t3code' });
    const t3 = await loadFreshHostBridge();
    assert.equal(await t3.deliverPromptToHost('  Review PR  ', false), 'Prompt sent to T3 Code composer');
    assert.deepEqual(t3Env.posted, [{ data: { type: 'git-widget-ask-ai', prompt: 'Review PR' }, origin: '*' }]);

    const agEnv = setupMockWindow({ search: '' });
    const ag = await loadFreshHostBridge();
    assert.equal(
      await ag.deliverPromptToHost('Review PR', false),
      'Copied to clipboard (direct chat send unavailable)',
    );
    assert.deepEqual(agEnv.clipboardWrites, ['Review PR']);
  });
});
