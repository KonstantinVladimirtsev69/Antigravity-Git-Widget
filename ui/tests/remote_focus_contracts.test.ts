import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { afterEach, describe, it } from 'node:test';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..');
const require = createRequire(import.meta.url);
const ts = require(path.join(ROOT, 'ui', 'node_modules', 'typescript', 'lib', 'typescript.js')) as typeof import('typescript');

const origWindowDesc = Object.getOwnPropertyDescriptor(globalThis, 'window');
const origFetchDesc = Object.getOwnPropertyDescriptor(globalThis, 'fetch');

function loadTranspiledApi(): typeof import('../src/api').api {
  const httpSrc = fs.readFileSync(path.join(ROOT, 'ui', 'src', 'utils', 'http.ts'), 'utf8');
  const apiSrc = fs.readFileSync(path.join(ROOT, 'ui', 'src', 'api.ts'), 'utf8');

  const compileCjs = (source: string, fileName: string) =>
    ts.transpileModule(source, {
      fileName,
      compilerOptions: {
        target: ts.ScriptTarget.ES2022,
        module: ts.ModuleKind.CommonJS,
        esModuleInterop: true,
      },
    }).outputText;

  const evalCjs = (code: string, localRequire: (id: string) => unknown) => {
    const mod = { exports: {} as Record<string, unknown> };
    const fn = new Function('exports', 'require', 'module', code);
    fn(mod.exports, localRequire, mod);
    return mod.exports;
  };

  const httpExports = evalCjs(compileCjs(httpSrc, 'http.ts'), require);
  const apiExports = evalCjs(compileCjs(apiSrc, 'api.ts'), (id: string) => {
    if (id === './utils/http') return httpExports;
    return require(id);
  });
  return apiExports.api as typeof import('../src/api').api;
}

afterEach(() => {
  if (origWindowDesc) Object.defineProperty(globalThis, 'window', origWindowDesc);
  else delete (globalThis as Record<string, unknown>).window;
  if (origFetchDesc) Object.defineProperty(globalThis, 'fetch', origFetchDesc);
  else delete (globalThis as Record<string, unknown>).fetch;
});

describe('api.ts multi-remote request serialization (T-13-03)', () => {
  it('encodes remote query params including slashes and forwards remote in POST bodies', async () => {
    const calls: Array<{ url: string; method: string; token: string | null; body?: string }> = [];

    Object.defineProperty(globalThis, 'window', {
      value: { location: { search: '?token=sec-tok&conversationId=conv-1' } },
      configurable: true,
      writable: true,
    });
    Object.defineProperty(globalThis, 'fetch', {
      value: async (url: string, init?: RequestInit) => {
        const headers = new Headers(init?.headers);
        calls.push({
          url,
          method: init?.method ?? 'GET',
          token: headers.get('X-Sidecar-Token'),
          body: typeof init?.body === 'string' ? init.body : undefined,
        });
        return new Response(JSON.stringify({ ok: true }), { status: 200 });
      },
      configurable: true,
      writable: true,
    });

    const api = loadTranspiledApi();
    await api.getStatus('C:\\Projects\\Repo', 'team/fork');
    await api.getStatus('C:\\Projects\\Repo');
    await api.getGraph('C:\\Projects\\Repo', false, 'Dev', 'team/fork');
    await api.getPrs('C:\\Projects\\Repo', 'team/fork');
    await api.switchRemote('C:\\Projects\\Repo', 'team/fork');
    await api.previewPush('C:\\Projects\\Repo', 'second');
    await api.runGuarded('C:\\Projects\\Repo', 'checkout', 'dev', { mode: 'fail_if_dirty' }, 'second');
    await api.checkoutRemote('C:\\Projects\\Repo', 'second/dev', 'second');

    assert.equal(calls[0].url, './api/status?repo=C%3A%5CProjects%5CRepo&remote=team%2Ffork');
    assert.equal(calls[0].token, 'sec-tok');
    assert.equal(calls[1].url, './api/status?repo=C%3A%5CProjects%5CRepo');
    assert.equal(
      calls[2].url,
      './api/graph?repo=C%3A%5CProjects%5CRepo&all=false&author=Dev&remote=team%2Ffork',
    );
    assert.equal(calls[3].url, './api/prs?repo=C%3A%5CProjects%5CRepo&remote=team%2Ffork');
    assert.equal(calls[4].url, './api/git/switch-remote');
    assert.deepEqual(JSON.parse(calls[4].body!), { repo: 'C:\\Projects\\Repo', remote: 'team/fork' });
    assert.equal(calls[5].url, './api/git/preview-push');
    assert.deepEqual(JSON.parse(calls[5].body!), { repo: 'C:\\Projects\\Repo', remote: 'second' });
    assert.equal(calls[6].url, './api/git/guarded');
    assert.deepEqual(JSON.parse(calls[6].body!), {
      repo: 'C:\\Projects\\Repo',
      kind: 'checkout',
      target: 'dev',
      strategy: { mode: 'fail_if_dirty' },
      remote: 'second',
    });
    assert.equal(calls[7].url, './api/git/branch-checkout-remote');
    assert.deepEqual(JSON.parse(calls[7].body!), {
      repo: 'C:\\Projects\\Repo',
      remote_branch: 'second/dev',
      remote: 'second',
    });
  });
});

describe('Phase 13 UI gating and v0.4.0 release parity', () => {
  it('keeps Cargo.toml, ui/package.json, and CHANGELOG.md aligned at 0.4.0', () => {
    const cargo = fs.readFileSync(path.join(ROOT, 'Cargo.toml'), 'utf8');
    const pkg = JSON.parse(fs.readFileSync(path.join(ROOT, 'ui', 'package.json'), 'utf8')) as {
      version: string;
    };
    const changelog = fs.readFileSync(path.join(ROOT, 'CHANGELOG.md'), 'utf8');

    const cargoVer = cargo.match(/^\s*version\s*=\s*"([^"]+)"/m)?.[1];
    assert.equal(cargoVer, '0.4.0');
    assert.equal(pkg.version, cargoVer);
    assert.match(changelog, /^## \[(?:v)?0\.4\.0\]/m);
    assert.match(changelog, /Multi-Remote Focus/i);
    assert.match(changelog, /T3 Code Update Auto-Repatch/i);
  });

  it('enforces UI gating invariants across HeaderBar, PrPanel, RemoteModal, NoticeFooter, BranchPanel', () => {
    const header = fs.readFileSync(path.join(ROOT, 'ui', 'src', 'components', 'HeaderBar.tsx'), 'utf8');
    const pr = fs.readFileSync(path.join(ROOT, 'ui', 'src', 'PrPanel.tsx'), 'utf8');
    const modal = fs.readFileSync(path.join(ROOT, 'ui', 'src', 'RemoteModal.tsx'), 'utf8');
    const footer = fs.readFileSync(path.join(ROOT, 'ui', 'src', 'components', 'NoticeFooter.tsx'), 'utf8');
    const branch = fs.readFileSync(path.join(ROOT, 'ui', 'src', 'BranchPanel.tsx'), 'utf8');

    assert.match(header, /\{githubUrl && \(/);
    assert.match(header, /disabled=\{busy \|\| status\.remotes\.length === 0\}/);
    assert.match(pr, /if \(!prData\?\.github_url\)\s*\{\s*return null;/);
    assert.match(modal, /\{!hasExistingRemotes && \(/);
    assert.match(footer, /window\.setTimeout\(\(\) => onDismissFetchWarning\(\), 10000\)/);
    assert.match(footer, /Retry/);
    assert.match(branch, /isDiverged\(status\)/);
    assert.match(branch, /Publish Branch/);
  });
});
