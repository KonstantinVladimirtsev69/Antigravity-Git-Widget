# Changelog

All notable changes to **Antigravity Git Widget** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-11-25

### Added
- **Repository Cloning (Phase 12)**:
  - **Clone button & Modal (`CloneModal`)**: Clone any HTTPS or SSH repository directly from the widget header with URL validation, automatic folder name extraction, optional shallow clone (`--depth 1`), and persistent default parent directory (`last_clone_dir`).
  - **Native Windows Folder Picker**: Non-flickering Win32 `IFileOpenDialog` picker with layered invisible owner window and smooth host focus restoration.
  - **Real-time Clone Progress (`CloneProgress`)**: Live NDJSON streaming of `git clone --progress` percentage, phase status, collapsible git log, single-flight conflict guard (`409`), strict inactivity/total timeouts, and idempotent cancellation with `taskkill` process-tree cleanup.
  - **Host-Aware Post-Clone Actions**:
    - **All hosts**: `Open in Explorer`, `Copy Path`, and persistent `adHocRepos` tracking in the repository selector (without overriding host-driven repository switching).
    - **T3 Code (Desktop & Mobile Web)**: `Add to T3 Code` button using `/api/projects/mutate` (`project.create`) with Desktop `desktopBridge` bearer/CORS support and clipboard fallback on conflict/error.

### Fixed
- Eliminated Windows folder picker focus flicker and Antigravity host window flash on folder confirmation.
- Standardized all Clone UI strings, tooltips, and T3 Code toast notifications to English.
- Fixed T3 Code Desktop `t3code://app` CORS handling (`credentials: 'omit'` with `Authorization: Bearer`) when creating projects.
- Prevented unintended automatic repository switching in `App.tsx` / `t3-project-bridge.js` on clone completion so the host shell remains the single source of truth.

## [0.2.0] - 2026-11-20

### Added
- **T3 Code Integration**: Non-invasive `boot.cjs` ASAR hook, `/git-widget/*` HTTP proxy for Desktop and Tailscale Mobile Web, resizable docked drawer (`t3-ui-bridge.js`), and Task Scheduler auto-repatcher (`--ensure-t3-patched`).
- **In-App Self-Updater**: Automatic GitHub `releases/latest` check (6h TTL + manual refresh), `Update vX.Y.Z` header badge with release notes, `SHA256SUMS.txt` verification, and 1-click multi-host binary update without closing T3 Code or modifying `app.asar`.
- **GitHub OAuth Device Flow**: Sign in to GitHub directly from the widget to list/create Pull Requests and push private/public repositories.

## [0.1.0] - 2026-11-10

### Added
- Initial release for Antigravity 2.0 (`AUX_PANE`): VS Code-style `Staged Changes` / `Changes`, interactive SVG commit graph, per-hunk diff review, `💬 AI` chat integration, 1-click directional merge with dirty-tree stash protection, and `Create on GitHub`.
