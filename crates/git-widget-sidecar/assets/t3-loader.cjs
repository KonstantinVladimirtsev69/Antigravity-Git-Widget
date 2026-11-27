// External loader injected via 1-line try/catch into T3 Code's boot.cjs.
// Fail-safe: never throws into Electron main process.
'use strict';

(function initGitWidgetT3Loader() {
  try {
    if (global.__gitWidgetT3LoaderInit) return;
    if (process.env.ELECTRON_RUN_AS_NODE === '1') return;
    global.__gitWidgetT3LoaderInit = true;

    const fs = require('fs');
    const path = require('path');
    const { spawn } = require('child_process');

    const installDir = __dirname;
    const sidecarExe = path.join(installDir, 'git-widget-sidecar.exe');
    const hookScript = path.join(installDir, 't3-server-hook.cjs');
    const bridgeScript = path.join(installDir, 't3-ui-bridge.js');
    const portFile = path.join(installDir, 't3-sidecar.port');

    if (!fs.existsSync(sidecarExe) || !fs.existsSync(hookScript)) {
      return;
    }

    try {
      if (fs.existsSync(portFile)) fs.unlinkSync(portFile);
    } catch (_) {}

    const child = spawn(
      sidecarExe,
      ['--serve', '--port', '0', '--port-file', portFile],
      {
        stdio: 'ignore',
        windowsHide: true,
        detached: false,
      }
    );
    child.on('error', () => {});

    let killed = false;
    let updateWatcherSpawned = false;
    const isPendingT3Update = () => {
      try {
        const localAppData = process.env.LOCALAPPDATA;
        if (!localAppData) return false;
        const pendingDir = path.join(localAppData, 't3code-updater', 'pending');
        if (!fs.existsSync(pendingDir)) return false;
        return fs.readdirSync(pendingDir).length > 0;
      } catch (_) {
        return false;
      }
    };
    const spawnUpdateWatcher = () => {
      if (updateWatcherSpawned) return;
      updateWatcherSpawned = true;
      try {
        const watcher = spawn(
          sidecarExe,
          ['--ensure-t3-patched', '--watch-update'],
          {
            stdio: 'ignore',
            windowsHide: true,
            detached: true,
          }
        );
        watcher.on('error', () => {});
        watcher.unref();
      } catch (_) {}
    };
    const cleanup = () => {
      if (killed) return;
      killed = true;
      if (isPendingT3Update()) {
        spawnUpdateWatcher();
      }
      try {
        if (child && !child.killed) child.kill();
      } catch (_) {}
      try {
        if (fs.existsSync(portFile)) fs.unlinkSync(portFile);
      } catch (_) {}
    };

    process.once('exit', cleanup);
    process.once('SIGINT', cleanup);
    process.once('SIGTERM', cleanup);

    try {
      const electron = require('electron');
      if (electron && electron.autoUpdater && typeof electron.autoUpdater.once === 'function') {
        electron.autoUpdater.once('before-quit-for-update', spawnUpdateWatcher);
      }
      if (electron && electron.app && typeof electron.app.once === 'function') {
        electron.app.once('will-quit', cleanup);
        electron.app.once('before-quit', cleanup);
        electron.app.on('browser-window-created', (_ev, win) => {
          try {
            if (!win || !win.webContents) return;
            win.webContents.on('dom-ready', () => {
              try {
                const url = win.webContents.getURL() || '';
                if (!url || url.startsWith('data:') || url.startsWith('about:')) return;
                const inject = (attempt) => {
                  try {
                    const port = fs.existsSync(portFile)
                      ? fs.readFileSync(portFile, 'utf8').trim()
                      : '';
                    if (!port || !/^\d+$/.test(port)) {
                      if (attempt < 20) setTimeout(() => inject(attempt + 1), 150);
                      return;
                    }
                    const bridgeSrc = fs.readFileSync(bridgeScript, 'utf8');
                    const payload = `window.__GW_T3_BASE_URL = "http://127.0.0.1:${port}";\n${bridgeSrc}`;
                    win.webContents.executeJavaScript(payload).catch(() => {});
                  } catch (_) {}
                };
                inject(0);
              } catch (_) {}
            });
          } catch (_) {}
        });
      }
    } catch (_) {}

    process.env.GIT_WIDGET_PORT_FILE = portFile;
    process.env.GIT_WIDGET_BRIDGE_JS = bridgeScript;

    const hookPosix = hookScript.replace(/\\/g, '/');
    const requireFlag = `--require "${hookPosix.replace(/"/g, '\\"')}"`;
    const currentOpts = process.env.NODE_OPTIONS || '';
    if (!currentOpts.includes(hookPosix) && !currentOpts.includes(hookScript)) {
      process.env.NODE_OPTIONS = currentOpts
        ? `${currentOpts} ${requireFlag}`
        : requireFlag;
    }
  } catch (_) {
    // Fail-safe: T3 Code must always boot normally.
  }
})();
