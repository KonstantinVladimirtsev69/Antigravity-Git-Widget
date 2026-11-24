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
    const cleanup = () => {
      if (killed) return;
      killed = true;
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
      if (electron && electron.app && typeof electron.app.once === 'function') {
        electron.app.once('will-quit', cleanup);
        electron.app.once('before-quit', cleanup);
      }
    } catch (_) {}

    process.env.GIT_WIDGET_PORT_FILE = portFile;
    process.env.GIT_WIDGET_BRIDGE_JS = bridgeScript;

    const requireFlag = `--require "${hookScript.replace(/"/g, '\\"')}"`;
    const currentOpts = process.env.NODE_OPTIONS || '';
    if (!currentOpts.includes(hookScript)) {
      process.env.NODE_OPTIONS = currentOpts
        ? `${currentOpts} ${requireFlag}`
        : requireFlag;
    }
  } catch (_) {
    // Fail-safe: T3 Code must always boot normally.
  }
})();
