// Node server hook loaded via NODE_OPTIONS in T3 Code's backend server process.
// Proxies /git-widget/* under T3 Code's native session auth and injects t3-ui-bridge.js.
'use strict';

(function initGitWidgetServerHook() {
  try {
    if (global.__gitWidgetServerHookInit) return;
    const isBackend =
      process.env.GIT_WIDGET_FORCE_HOOK === '1' ||
      (process.env.ELECTRON_RUN_AS_NODE === '1' &&
        process.argv.some((a) => /bin\.mjs$/i.test(String(a))));
    if (!isBackend) return;
    global.__gitWidgetServerHookInit = true;

    const fs = require('fs');
    const http = require('http');

    const AUTH_CACHE_TTL_MS = 10_000;
    const authCache = new Map();
    const BRIDGE_TAG = '<script src="/git-widget/t3-ui-bridge.js" defer></script>';

    function readSidecarPort() {
      if (process.env.GIT_WIDGET_SIDECAR_PORT) {
        const p = Number(process.env.GIT_WIDGET_SIDECAR_PORT);
        if (Number.isInteger(p) && p > 0 && p <= 65535) return p;
      }
      const pf = process.env.GIT_WIDGET_PORT_FILE;
      if (!pf) return null;
      try {
        const raw = fs.readFileSync(pf, 'utf8').trim();
        const p = Number(raw);
        if (Number.isInteger(p) && p > 0 && p <= 65535) return p;
      } catch (_) {}
      return null;
    }

    function verifyT3Auth(req, t3Port) {
      return new Promise((resolve) => {
        const cookie = req.headers.cookie || '';
        const auth = req.headers.authorization || '';
        if (!cookie && !auth) {
          resolve(false);
          return;
        }
        const key = `${cookie}\n${auth}`;
        const now = Date.now();
        const cached = authCache.get(key);
        if (cached && cached.exp > now) {
          resolve(cached.ok);
          return;
        }

        const headers = {};
        if (cookie) headers.cookie = cookie;
        if (auth) headers.authorization = auth;

        const checkReq = http.request(
          {
            hostname: '127.0.0.1',
            port: t3Port,
            path: '/api/auth/session',
            method: 'GET',
            headers,
            timeout: 3000,
          },
          (checkRes) => {
            let body = '';
            checkRes.setEncoding('utf8');
            checkRes.on('data', (c) => {
              if (body.length < 16384) body += c;
            });
            checkRes.on('end', () => {
              let ok = false;
              if (checkRes.statusCode === 200) {
                try {
                  const parsed = JSON.parse(body);
                  ok = Boolean(parsed && parsed.authenticated === true);
                } catch (_) {}
              }
              if (authCache.size > 128) authCache.clear();
              authCache.set(key, { ok, exp: now + AUTH_CACHE_TTL_MS });
              resolve(ok);
            });
          }
        );
        checkReq.on('error', () => resolve(false));
        checkReq.on('timeout', () => {
          checkReq.destroy();
          resolve(false);
        });
        checkReq.end();
      });
    }

    function proxyToSidecar(req, res, targetPort) {
      const origUrl = req.url || '/git-widget/';
      let stripped = origUrl.replace(/^\/git-widget(?=\/|\?|$)/, '');
      if (!stripped || stripped.startsWith('?')) {
        stripped = `/${stripped}`;
      }
      const fwdHeaders = { ...req.headers, host: `127.0.0.1:${targetPort}` };
      delete fwdHeaders['accept-encoding'];

      const proxyReq = http.request(
        {
          hostname: '127.0.0.1',
          port: targetPort,
          path: stripped,
          method: req.method,
          headers: fwdHeaders,
        },
        (proxyRes) => {
          res.writeHead(proxyRes.statusCode || 502, proxyRes.headers);
          proxyRes.pipe(res);
        }
      );
      proxyReq.on('error', () => {
        if (!res.headersSent) {
          res.writeHead(502, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: 'Git Widget sidecar unavailable' }));
        }
      });
      req.pipe(proxyReq);
    }

    function serveBridgeScript(res) {
      const bridgePath = process.env.GIT_WIDGET_BRIDGE_JS;
      if (!bridgePath) {
        res.writeHead(404, { 'Content-Type': 'text/plain' });
        res.end('Bridge script not configured');
        return;
      }
      fs.readFile(bridgePath, (err, data) => {
        if (err) {
          res.writeHead(404, { 'Content-Type': 'text/plain' });
          res.end('Bridge script missing');
          return;
        }
        res.writeHead(200, {
          'Content-Type': 'application/javascript; charset=utf-8',
          'Cache-Control': 'no-cache',
        });
        res.end(data);
      });
    }

    function wrapHtmlInjection(req, res) {
      delete req.headers['accept-encoding'];
      const origWriteHead = res.writeHead.bind(res);
      const origWrite = res.write.bind(res);
      const origEnd = res.end.bind(res);
      let decided = false;
      let isHtml = false;
      let chunks = [];
      let savedStatus = null;
      let savedHeaders = null;

      function decide(hdrs) {
        if (decided) return isHtml;
        decided = true;
        const ct =
          (hdrs && (hdrs['content-type'] || hdrs['Content-Type'])) ||
          res.getHeader('content-type') ||
          '';
        const ce =
          (hdrs && (hdrs['content-encoding'] || hdrs['Content-Encoding'])) ||
          res.getHeader('content-encoding') ||
          '';
        isHtml = String(ct).includes('text/html') && !ce;
        if (isHtml) {
          res.removeHeader('content-length');
        }
        return isHtml;
      }

      res.writeHead = function (statusCode, statusMsg, headersObj) {
        let hdrs = headersObj;
        if (typeof statusMsg === 'object' && statusMsg !== null) {
          hdrs = statusMsg;
        }
        if (decide(hdrs)) {
          savedStatus = statusCode;
          savedHeaders = hdrs ? { ...hdrs } : {};
          delete savedHeaders['content-length'];
          delete savedHeaders['Content-Length'];
          return res;
        }
        return origWriteHead.apply(res, arguments);
      };

      res.write = function (chunk, encoding) {
        if (!decide(null)) return origWrite.apply(res, arguments);
        if (chunk) {
          chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk, encoding));
        }
        return true;
      };

      res.end = function (chunk, encoding) {
        if (!decide(null)) return origEnd.apply(res, arguments);
        if (chunk) {
          chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk, encoding));
        }
        let html = Buffer.concat(chunks).toString('utf8');
        if (!html.includes('/git-widget/t3-ui-bridge.js')) {
          if (html.includes('</body>')) {
            html = html.replace('</body>', `  ${BRIDGE_TAG}\n</body>`);
          } else {
            html += `\n${BRIDGE_TAG}\n`;
          }
        }
        const outBuf = Buffer.from(html, 'utf8');
        res.writeHead = origWriteHead;
        res.write = origWrite;
        res.end = origEnd;
        if (savedStatus !== null) {
          const outHeaders = savedHeaders || {};
          outHeaders['content-length'] = String(outBuf.length);
          origWriteHead(savedStatus, outHeaders);
        } else {
          res.removeHeader('transfer-encoding');
          res.setHeader('content-length', String(outBuf.length));
        }
        return origEnd(outBuf);
      };
    }

    const origEmit = http.Server.prototype.emit;
    http.Server.prototype.emit = function patchedEmit(event, req, res) {
      if (event === 'request' && req && res && typeof req.url === 'string') {
        const urlPath = req.url.split('?')[0];
        if (urlPath === '/git-widget/t3-ui-bridge.js') {
          serveBridgeScript(res);
          return true;
        }
        if (urlPath === '/git-widget' || urlPath.startsWith('/git-widget/')) {
          const addr = this.address && this.address();
          const t3Port = addr && typeof addr === 'object' ? addr.port : null;
          if (!t3Port) {
            res.writeHead(503, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'Host server port unavailable' }));
            return true;
          }
          verifyT3Auth(req, t3Port).then((ok) => {
            if (!ok) {
              res.writeHead(401, { 'Content-Type': 'application/json' });
              res.end(JSON.stringify({ error: 'Unauthorized' }));
              return;
            }
            const sidecarPort = readSidecarPort();
            if (!sidecarPort) {
              res.writeHead(503, { 'Content-Type': 'application/json' });
              res.end(JSON.stringify({ error: 'Git Widget sidecar starting' }));
              return;
            }
            proxyToSidecar(req, res, sidecarPort);
          });
          return true;
        }
        if (
          req.method === 'GET' &&
          !urlPath.startsWith('/api/') &&
          !urlPath.startsWith('/assets/') &&
          String(req.headers.accept || '').includes('text/html')
        ) {
          wrapHtmlInjection(req, res);
        }
      }
      return origEmit.apply(this, arguments);
    };
  } catch (_) {
    // Fail-safe: never break T3 Code backend server.
  }
})();
