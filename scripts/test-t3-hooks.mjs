// Integration test for t3-server-hook.cjs (auth enforcement, UTF-8 proxying, HTML injection).
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import http from 'node:http';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const assetsDir = path.resolve(__dirname, '..', 'crates', 'git-widget-sidecar', 'assets');
const hookPath = path.join(assetsDir, 't3-server-hook.cjs');
const bridgePath = path.join(assetsDir, 't3-ui-bridge.js');

function listenRandom(server) {
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => resolve(server.address().port));
  });
}

function req(port, reqPath, headers = {}) {
  return new Promise((resolve, reject) => {
    const r = http.request(
      { hostname: '127.0.0.1', port, path: reqPath, method: 'GET', headers },
      (res) => {
        let body = '';
        res.setEncoding('utf8');
        res.on('data', (c) => (body += c));
        res.on('end', () => resolve({ status: res.statusCode, headers: res.headers, body }));
      }
    );
    r.on('error', reject);
    r.end();
  });
}

// 1. Start mock sidecar
const mockSidecar = http.createServer((request, response) => {
  response.writeHead(200, { 'Content-Type': 'application/json' });
  response.end(JSON.stringify({ ok: true, receivedUrl: request.url }));
});
const sidecarPort = await listenRandom(mockSidecar);

// 2. Load t3-server-hook.cjs
process.env.GIT_WIDGET_FORCE_HOOK = '1';
process.env.GIT_WIDGET_SIDECAR_PORT = String(sidecarPort);
process.env.GIT_WIDGET_BRIDGE_JS = bridgePath;
const require = createRequire(import.meta.url);
require(hookPath);

// 3. Start mock T3 Code HTTP server
const mockT3 = http.createServer((request, response) => {
  if (request.url === '/api/auth/session') {
    const ok =
      request.headers.cookie === 't3_session=valid-token' ||
      request.headers.authorization === 'Bearer valid-token';
    response.writeHead(200, { 'Content-Type': 'application/json' });
    response.end(JSON.stringify({ authenticated: ok }));
    return;
  }
  if (request.url === '/') {
    const html = '<!doctype html><html><body><div id="root">T3</div></body></html>';
    response.writeHead(200, {
      'Content-Type': 'text/html; charset=utf-8',
      'Content-Length': String(Buffer.byteLength(html)),
    });
    response.end(html);
    return;
  }
  response.writeHead(404);
  response.end('Not found');
});
const t3Port = await listenRandom(mockT3);

try {
  // Prohibition P2: unauthenticated request MUST get 401
  const noAuth = await req(t3Port, '/git-widget/api/repos');
  assert.equal(noAuth.status, 401, 'unauthenticated /git-widget/api/repos must return 401');

  const badCookie = await req(t3Port, '/git-widget/api/repos', { cookie: 't3_session=invalid' });
  assert.equal(badCookie.status, 401, 'invalid cookie must return 401');

  // Authenticated via cookie + Cyrillic query param preserved
  const cyrQuery = '/git-widget/api/status?repo=' + encodeURIComponent('C:\\Users\\Константин\\repo');
  const okCookie = await req(t3Port, cyrQuery, { cookie: 't3_session=valid-token' });
  assert.equal(okCookie.status, 200);
  const parsedCookie = JSON.parse(okCookie.body);
  assert.equal(parsedCookie.ok, true);
  assert.equal(
    parsedCookie.receivedUrl,
    '/api/status?repo=' + encodeURIComponent('C:\\Users\\Константин\\repo')
  );

  // Authenticated via Bearer token
  const okBearer = await req(t3Port, '/git-widget/api/repos', {
    authorization: 'Bearer valid-token',
  });
  assert.equal(okBearer.status, 200);
  assert.equal(JSON.parse(okBearer.body).receivedUrl, '/api/repos');

  // HTML injection into index.html
  const htmlRes = await req(t3Port, '/', { accept: 'text/html' });
  assert.equal(htmlRes.status, 200);
  assert.ok(
    htmlRes.body.includes('<script src="/git-widget/t3-ui-bridge.js" defer></script>'),
    'index.html must include injected bridge script tag'
  );

  // Bridge script served
  const bridgeRes = await req(t3Port, '/git-widget/t3-ui-bridge.js');
  assert.equal(bridgeRes.status, 200);
  assert.ok(bridgeRes.body.includes('initGitWidgetT3Bridge'));

  // Regression: NODE_OPTIONS --require with forward-slash Windows path loads cleanly
  const hookPosix = hookPath.replace(/\\/g, '/');
  const out = execFileSync(process.execPath, ['-e', 'process.stdout.write("node-options-ok")'], {
    env: {
      ...process.env,
      NODE_OPTIONS: `--require "${hookPosix.replace(/"/g, '\\"')}"`,
      ELECTRON_RUN_AS_NODE: '0',
      GIT_WIDGET_FORCE_HOOK: '0',
    },
    encoding: 'utf8',
  });
  assert.equal(out, 'node-options-ok', 'NODE_OPTIONS --require must load hook without backslash escaping');

  console.log('[PASS] All 7 T3 Code server hook integration assertions passed.');
} finally {
  mockT3.closeAllConnections?.();
  mockSidecar.closeAllConnections?.();
  mockT3.close();
  mockSidecar.close();
}
