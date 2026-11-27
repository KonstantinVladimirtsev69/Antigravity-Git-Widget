import { afterEach, describe, it } from 'node:test';
import assert from 'node:assert/strict';

import {
  extractRemotePrefix,
  filterRemoteBranchesForRemote,
  isCurrentRemoteRequest,
  isDiverged,
  loadSavedRemote,
  resolveDefaultRemote,
  saveSelectedRemote,
  splitRemoteBranch,
} from '../src/utils/remoteFocus.ts';

const originalWindowDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'window');

function createMockLocalStorage() {
  const store = new Map<string, string>();
  return {
    store,
    storage: {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, val: string) => {
        store.set(key, val);
      },
      removeItem: (key: string) => {
        store.delete(key);
      },
    },
  };
}

function setMockLocalStorage(storage: {
  getItem: (key: string) => string | null;
  setItem: (key: string, val: string) => void;
  removeItem?: (key: string) => void;
}): void {
  Object.defineProperty(globalThis, 'window', {
    value: { localStorage: storage },
    configurable: true,
    writable: true,
  });
}

afterEach(() => {
  if (originalWindowDescriptor) {
    Object.defineProperty(globalThis, 'window', originalWindowDescriptor);
  } else {
    delete (globalThis as Record<string, unknown>).window;
  }
});

describe('loadSavedRemote and saveSelectedRemote', () => {
  it('normalizes Windows paths and isolates separate repositories', () => {
    const { storage } = createMockLocalStorage();
    setMockLocalStorage(storage);

    saveSelectedRemote('C:\\Projects\\MyRepo', 'upstream');
    assert.equal(loadSavedRemote('c:/projects/myrepo/'), 'upstream');
    assert.equal(loadSavedRemote('C:\\Projects\\OtherRepo'), null);

    saveSelectedRemote('C:\\Projects\\OtherRepo', 'team/fork');
    assert.equal(loadSavedRemote('c:/projects/myrepo'), 'upstream');
    assert.equal(loadSavedRemote('c:/projects/otherrepo/'), 'team/fork');
  });

  it('ignores empty values, missing window, and storage errors safely', () => {
    delete (globalThis as Record<string, unknown>).window;
    assert.equal(loadSavedRemote('C:\\Projects\\MyRepo'), null);
    assert.doesNotThrow(() => saveSelectedRemote('C:\\Projects\\MyRepo', 'origin'));

    const { store, storage } = createMockLocalStorage();
    setMockLocalStorage(storage);
    saveSelectedRemote('   ', 'origin');
    saveSelectedRemote('C:\\Projects\\MyRepo', '   ');
    assert.equal(store.size, 0);
    assert.equal(loadSavedRemote('C:\\Projects\\MyRepo'), null);

    setMockLocalStorage({
      getItem: () => {
        throw new Error('SecurityError');
      },
      setItem: () => {
        throw new Error('QuotaExceededError');
      },
    });
    assert.equal(loadSavedRemote('C:\\Projects\\MyRepo'), null);
    assert.doesNotThrow(() => saveSelectedRemote('C:\\Projects\\MyRepo', 'origin'));
  });
});

describe('resolveDefaultRemote', () => {
  it('follows 4-tier priority: savedRemote -> upstream prefix -> origin -> remotes[0] -> empty', () => {
    const remotes = ['second', 'origin', 'team/fork'];

    // Tier 1: valid savedRemote wins over upstream and origin
    assert.equal(
      resolveDefaultRemote({
        savedRemote: 'second',
        upstream: 'origin/main',
        configuredUpstream: 'origin/main',
        remotes,
      }),
      'second',
    );

    // Tier 2: deleted/invalid savedRemote falls through to configuredUpstream / upstream prefix
    assert.equal(
      resolveDefaultRemote({
        savedRemote: 'deleted-remote',
        upstream: null,
        configuredUpstream: 'team/fork/feat/ui',
        remotes,
      }),
      'team/fork',
    );
    assert.equal(
      resolveDefaultRemote({
        savedRemote: null,
        upstream: 'second/main',
        remotes,
      }),
      'second',
    );

    // Tier 3: missing upstream falls through to 'origin'
    assert.equal(
      resolveDefaultRemote({
        savedRemote: 'gone',
        upstream: null,
        configuredUpstream: null,
        remotes: ['second', 'origin'],
      }),
      'origin',
    );

    // Tier 4: missing 'origin' falls through to remotes[0]
    assert.equal(
      resolveDefaultRemote({
        savedRemote: null,
        upstream: null,
        remotes: ['upstream', 'backup'],
      }),
      'upstream',
    );

    // Empty remotes returns ''
    assert.equal(
      resolveDefaultRemote({
        savedRemote: 'origin',
        upstream: 'origin/main',
        remotes: [],
      }),
      '',
    );
  });

  it('is idempotent when re-evaluated with the resolved remote', () => {
    const remotes = ['origin', 'team/fork'];
    const first = resolveDefaultRemote({
      savedRemote: null,
      upstream: 'team/fork/main',
      remotes,
    });
    assert.equal(first, 'team/fork');

    const second = resolveDefaultRemote({
      savedRemote: first,
      upstream: 'origin/main',
      remotes,
    });
    assert.equal(second, first);
  });
});

describe('splitRemoteBranch, extractRemotePrefix, and filterRemoteBranchesForRemote', () => {
  it('resolves slash remote names by longest matching prefix', () => {
    const remotes = ['origin', 'team', 'team/fork'];

    assert.deepEqual(splitRemoteBranch('team/fork/feat/ui', remotes), {
      remote: 'team/fork',
      branch: 'feat/ui',
    });
    assert.deepEqual(splitRemoteBranch('team/other', remotes), {
      remote: 'team',
      branch: 'other',
    });
    assert.equal(splitRemoteBranch('unknown/main', remotes), null);
    assert.equal(extractRemotePrefix('team/fork/main', remotes), 'team/fork');
    assert.equal(extractRemotePrefix(null, remotes), null);
  });

  it('filters remote branches strictly to selectedRemote and excludes existing local branches', () => {
    const remoteBranches = ['origin/main', 'team/fork/main', 'team/fork/feat/ui', 'team/other'];
    const remotes = ['origin', 'team', 'team/fork'];

    assert.deepEqual(
      filterRemoteBranchesForRemote(remoteBranches, [], 'team/fork', remotes),
      ['team/fork/main', 'team/fork/feat/ui'],
    );
    assert.deepEqual(
      filterRemoteBranchesForRemote(remoteBranches, ['main'], 'team/fork', remotes),
      ['team/fork/feat/ui'],
    );
    assert.deepEqual(
      filterRemoteBranchesForRemote(remoteBranches, [], 'team', remotes),
      ['team/other'],
    );
    assert.deepEqual(
      filterRemoteBranchesForRemote(remoteBranches, [], 'empty-remote', [...remotes, 'empty-remote']),
      [],
    );
  });
});

describe('isDiverged', () => {
  it('returns true only when upstream is present and both ahead > 0 and behind > 0', () => {
    assert.equal(isDiverged({ upstream: 'second/main', ahead: 2, behind: 1 }), true);
    assert.equal(isDiverged({ upstream: 'origin/main', ahead: 0, behind: 0 }), false);
    assert.equal(isDiverged({ upstream: 'origin/main', ahead: 3, behind: 0 }), false);
    assert.equal(isDiverged({ upstream: 'origin/main', ahead: 0, behind: 4 }), false);
    assert.equal(isDiverged({ upstream: null, ahead: 2, behind: 2 }), false);
    assert.equal(isDiverged(null), false);
    assert.equal(isDiverged(undefined), false);
  });
});

describe('isCurrentRemoteRequest', () => {
  it('rejects stale responses when repo, remote, or requestSeq changed', () => {
    const current = { repo: 'c:/projects/repo/', remote: 'second', seq: 5 };

    assert.equal(
      isCurrentRemoteRequest({ repo: 'C:\\Projects\\Repo', remote: 'second', seq: 5 }, current),
      true,
    );
    assert.equal(
      isCurrentRemoteRequest({ repo: 'C:\\Projects\\Repo', remote: 'origin', seq: 5 }, current),
      false,
    );
    assert.equal(
      isCurrentRemoteRequest({ repo: 'C:\\Projects\\Repo', remote: 'second', seq: 4 }, current),
      false,
    );
    assert.equal(
      isCurrentRemoteRequest({ repo: 'C:\\Projects\\Other', remote: 'second', seq: 5 }, current),
      false,
    );
  });
});
