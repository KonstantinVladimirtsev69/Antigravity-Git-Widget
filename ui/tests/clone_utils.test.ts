import { afterEach, describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { extractFolderName, previewCloneUrl } from '../src/utils/cloneUrl.ts';
import {
  loadAdHocRepos,
  mergeRepoOptions,
  normalizeWinPath,
  saveAdHocRepos,
} from '../src/utils/repoOptions.ts';

const originalWindowDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'window');

function setMockSessionStorage(storage: {
  getItem: (key: string) => string | null;
  setItem: (key: string, val: string) => void;
}): void {
  Object.defineProperty(globalThis, 'window', {
    value: { sessionStorage: storage },
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

describe('previewCloneUrl', () => {
  it('expands owner/repo shorthand and trims whitespace and .git suffix', () => {
    assert.equal(previewCloneUrl('octocat/hello-world'), 'https://github.com/octocat/hello-world.git');
    assert.equal(previewCloneUrl('  octocat/hello-world.git  '), 'https://github.com/octocat/hello-world.git');
  });

  it('does not expand full URLs, SCP paths, or empty values', () => {
    assert.equal(previewCloneUrl(''), '');
    assert.equal(previewCloneUrl('   '), '');
    assert.equal(previewCloneUrl('  https://gitlab.com/org/repo.git  '), 'https://gitlab.com/org/repo.git');
    assert.equal(previewCloneUrl('ssh://git@host/org/repo.git'), 'ssh://git@host/org/repo.git');
    assert.equal(previewCloneUrl('git@github.com:org/repo.git'), 'git@github.com:org/repo.git');
  });

  it('rejects flag-injection, traversal, nested paths, and invalid characters from shorthand expansion', () => {
    assert.equal(previewCloneUrl('-evil/repo'), '-evil/repo');
    assert.equal(previewCloneUrl('owner/-repo'), 'owner/-repo');
    assert.equal(previewCloneUrl('../repo'), '../repo');
    assert.equal(previewCloneUrl('owner/..'), 'owner/..');
    assert.equal(previewCloneUrl('owner/.git'), 'owner/.git');
    assert.equal(previewCloneUrl('a/b/c'), 'a/b/c');
    assert.equal(previewCloneUrl('owner/bad:name'), 'owner/bad:name');
    assert.equal(previewCloneUrl('owner/has space'), 'owner/has space');
  });
});

describe('extractFolderName', () => {
  it('extracts folder name from HTTPS, SSH, SCP, file URLs, and owner/repo shorthand', () => {
    assert.equal(extractFolderName('https://host/org/my-app.git/'), 'my-app');
    assert.equal(extractFolderName('https://host/org/my-app.GIT///'), 'my-app');
    assert.equal(extractFolderName('https://user:secret@gitlab.com/team/sub/repo.git?ref=main#top'), 'repo');
    assert.equal(extractFolderName('owner/repo'), 'repo');
    assert.equal(extractFolderName('git@github.com:org/scp-repo.git'), 'scp-repo');
    assert.equal(extractFolderName('git@host:direct-repo.git'), 'direct-repo');
    assert.equal(extractFolderName('file:///C:/Projects/local-repo.git/'), 'local-repo');
    assert.equal(extractFolderName('C:\\Projects\\local-dir\\'), 'local-dir');
  });

  it('decodes UTF-8 and %20 sequences and falls back safely on malformed escapes', () => {
    assert.equal(
      extractFolderName('https://example.com/org/my%20app%20%D0%BF%D1%80%D0%BE%D0%B5%D0%BA%D1%82.git'),
      'my app проект',
    );
    assert.equal(extractFolderName('https://example.com/org/bad%ZZ%20name.git'), 'bad%ZZ name');
  });

  it('returns empty string for empty input, slashes only, or host-only URLs', () => {
    assert.equal(extractFolderName(''), '');
    assert.equal(extractFolderName('///'), '');
    assert.equal(extractFolderName('https://github.com'), '');
  });
});

describe('repoOptions helpers', () => {
  it('normalizeWinPath trims, converts forward slashes, strips trailing slashes, and lowercases', () => {
    assert.equal(normalizeWinPath('  C:/Projects/My-Repo//  '), 'c:\\projects\\my-repo');
    assert.equal(normalizeWinPath('D:\\Work\\App\\\\\\'), 'd:\\work\\app');
  });

  it('loadAdHocRepos and saveAdHocRepos round-trip valid entries and filter malformed items', () => {
    const store = new Map<string, string>();
    setMockSessionStorage({
      getItem: (key) => store.get(key) ?? null,
      setItem: (key, val) => {
        store.set(key, val);
      },
    });

    saveAdHocRepos([{ name: 'demo', path: 'C:\\Projects\\demo', is_worktree: false }]);
    assert.deepEqual(loadAdHocRepos(), [{ name: 'demo', path: 'C:\\Projects\\demo', is_worktree: false }]);

    store.set(
      'gw.adHocRepos',
      JSON.stringify([
        null,
        { name: 'no-path', path: '   ' },
        { path: 'C:\\NoName' },
        { name: 'wt-repo', path: 'C:\\Work\\wt-repo', is_worktree: 1 },
      ]),
    );
    assert.deepEqual(loadAdHocRepos(), [{ name: 'wt-repo', path: 'C:\\Work\\wt-repo', is_worktree: true }]);
  });

  it('loadAdHocRepos and saveAdHocRepos handle corrupt JSON and storage exceptions safely', () => {
    setMockSessionStorage({
      getItem: () => '{not-valid-json',
      setItem: () => {
        throw new Error('QuotaExceededError');
      },
    });
    assert.deepEqual(loadAdHocRepos(), []);
    assert.doesNotThrow(() => saveAdHocRepos([{ name: 'a', path: 'C:\\a', is_worktree: false }]));

    setMockSessionStorage({
      getItem: () => JSON.stringify({ unexpected: 'object' }),
      setItem: () => {},
    });
    assert.deepEqual(loadAdHocRepos(), []);
  });

  it('mergeRepoOptions deduplicates case-insensitively across slashes and formats worktree labels', () => {
    const options = mergeRepoOptions(
      [
        { name: 'MainRepo', path: 'C:\\Projects\\MainRepo', is_worktree: false },
        { name: 'DupScanner', path: 'c:/projects/mainrepo/', is_worktree: false },
      ],
      [
        { name: 'DuplicateAdHoc', path: 'C:/Projects/MAINREPO', is_worktree: false },
        { name: 'ClonedWt', path: 'D:\\Clones\\ClonedWt', is_worktree: true },
        { name: 'EmptyPath', path: '   ', is_worktree: false },
      ],
    );

    assert.deepEqual(options, [
      { value: 'C:\\Projects\\MainRepo', label: 'MainRepo', sub: 'C:\\Projects\\MainRepo' },
      { value: 'D:\\Clones\\ClonedWt', label: 'ClonedWt (wt)', sub: 'D:\\Clones\\ClonedWt' },
    ]);
  });
});
