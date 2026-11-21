# Contributing to Antigravity Git Widget

Thank you for your interest in improving **Antigravity Git Widget**!

## Development Setup

1. **Prerequisites (Windows):**
   - Rust (`rustup`, `x86_64-pc-windows-msvc` target, Visual Studio C++ Build Tools)
   - Node.js 20+ and `npm`
   - Git 2.35+

2. **Build the UI first:**
   Because `git-widget-sidecar` embeds `ui/dist` at compile time via `rust-embed`, always build the frontend before compiling the sidecar crate:
   ```powershell
   cd ui
   npm install
   npx tsc --noEmit
   npm run build
   cd ..
   ```

3. **Run tests and build:**
   ```powershell
   cargo test --workspace
   cargo build --release --workspace
   ```

4. **Run standalone in a browser (for UI testing outside Antigravity):**
   ```powershell
   $env:ANTIGRAVITY_SIDECAR_WEB_PORT = "39871"
   .\target\release\git-widget-sidecar.exe --serve
   ```
   Open `http://127.0.0.1:39871/` in your browser.

---

## Forking & Custom GitHub OAuth App

The **Sign in with GitHub** button uses **GitHub OAuth Device Flow** (RFC 8628), which requires only a public `Client ID` (no `client_secret`).

If you fork this repository and want to use your own GitHub OAuth App:
1. Go to GitHub **Settings → Developer settings → OAuth Apps → New OAuth App**.
2. Set **Homepage URL** to your fork URL and **Authorization callback URL** to `http://127.0.0.1`.
3. Check **Enable Device Flow** and leave **Expire user access tokens** unchecked.
4. Configure your `Client ID` using any of these options (in priority order):
   - **Runtime (`settings.json`)**: set `"github_oauth_client_id": "Ov23..."` in `~/.gemini/config/plugins/git-widget/settings.json`.
   - **Runtime env**: set `GITHUB_OAUTH_CLIENT_ID=Ov23...` (see `.env.example`).
   - **GitHub Actions CI/Release**: add a Repository Variable `GITHUB_OAUTH_CLIENT_ID` in **Settings → Secrets and variables → Actions → Variables**.

---

## Porting to macOS & Linux (Guide for Contributors)

The core engine (`git-widget-core`) and React UI are already mostly cross-platform. Currently only **Windows 10/11** binaries are built and tested. To add macOS or Linux support in a fork/PR, check these 4 areas:

1. **Default scanner roots (`crates/git-widget-core/src/scanner.rs`)**:
   - `ScannerConfig::default()` currently defaults to `C:\Projects\`. On Unix, resolve `$HOME/Projects` (or `$HOME`).
2. **Workspace URI normalization (`crates/git-widget-sidecar/src/workspace.rs`)**:
   - `uri_to_path` currently strips `file:///` and converts `/` → `\` for Windows drive paths (`file:///C:/...`). On macOS/Linux, `file:///Users/...` must preserve leading `/` and forward slashes.
3. **Self-installer & manifest paths (`crates/git-widget-sidecar/src/installer.rs` + `install.sh`)**:
   - Use binary name `git-widget-sidecar` (no `.exe`) on Unix, set executable permissions (`chmod 0755`), replace PowerShell `Stop-Process` with `pkill` (or `pid` check), and write the Unix binary path into `sidecar.json`.
   - Note for macOS distribution: unsigned binaries downloaded from the internet require `xattr -d com.apple.quarantine` unless notarized with an Apple Developer ID.
4. **Git SSH wrapper (`crates/git-widget-core/src/git_cli.rs`)**:
   - The `C:\Windows\System32\OpenSSH\ssh.exe` override is already gated by `#[cfg(windows)]`. macOS/Linux will use system `ssh` automatically.

---

## Code Guidelines

- **Modularity**: Keep source files focused on a single responsibility (target ≤ 250–300 lines per file).
- **Non-Interactive Git**: All `git.exe` calls in `git-widget-core` must run through `git_cli::run_git` / `run_git_raw` (`GIT_TERMINAL_PROMPT=0`, `-c core.quotePath=false`, timeout, `CREATE_NO_WINDOW`).
- **Tests**: Add integration tests using `tempfile::TempDir` under `crates/git-widget-core/tests/` for any new Git engine feature.
