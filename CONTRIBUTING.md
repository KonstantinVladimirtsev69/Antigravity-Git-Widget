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

## Code Guidelines

- **Modularity**: Keep source files focused on a single responsibility (target ≤ 250–300 lines per file).
- **Non-Interactive Git**: All `git.exe` calls in `git-widget-core` must run through `git_cli::run_git` / `run_git_raw` (`GIT_TERMINAL_PROMPT=0`, timeout, `CREATE_NO_WINDOW`).
- **Tests**: Add integration tests using `tempfile::TempDir` under `crates/git-widget-core/tests/` for any new Git engine feature.
