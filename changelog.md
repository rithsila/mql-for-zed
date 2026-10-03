# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `MQL: Backtest` task (`scripts/tester-mql-remote.sh`, `scripts/parse-tester-log.py`): compiles and deploys the EA, writes a tester `.ini`, runs `terminal64.exe /config:` with `ShutdownTerminal=1` on the Windows machine, waits, and prints a summary (deposit, final balance, net, deals, `OnTester`, test time) parsed from the tester log. Configured with `MQL_BT_*` variables, `~/.config/zed-mql/env` and an optional `<workspace>/.zed/mql-tester.env`; `<EA>.set` next to the source is used as the inputs file. Closes a running GUI terminal of the same install first (`MQL_BT_CLOSE_GUI=0` to abort instead).
- Added to `scripts/tasks.mql5-workspace.json`.

- `mql/ZedMqlStats.mqh`: `ZedMqlPrintStats()` for an EA's `OnTester` prints trades, profit factor, equity drawdown, recovery factor and Sharpe from `TesterStatistics()`; the backtest summary shows them. Uploaded to the VM `MQL5/Include` on each compile.

- `<EA>.report.html` next to the source: statistics, win rate, average/largest win and loss, and a balance curve, built from a deal list that `ZedMqlPrintStats()` writes to `Common/Files` (the terminal's own HTML report was not produced on the test VM).

### Changed

- Backtest: if `<EA>.set` is missing it is generated from the `.mq5` input defaults (`scripts/gen-set.py`; enum defaults are resolved to integers) and always passed to the tester, because otherwise the tester silently uses the inputs last saved on the VM. `MQL_BT_GENSET=0` disables this and prints a warning instead. The set is uploaded as `zedmql_<EA>.set` so VM's own saved profile is not overwritten.
- Backtest results (`<EA>.summary.txt`, `<EA>.tester.log`, `<EA>.htm` if any) are now saved next to the `.mq5` instead of `<workspace>/.mql-tester/`.
- Compile, syntax check and backtest resolve a non-`.mq5` active file (`_tester.ini`, `.set`, ...) to the `.mq5` in the same folder (`scripts/resolve-mql.sh`).

### Known limits

- `ZedMqlStats.mqh`, `.set` generation and the HTML report were run against one VM and one EA (a scratch copy of FlexUltimateGRH) only; numbers were not compared with the MetaTrader GUI.

- Checked end to end with FlexUltimateGRH on one Windows 10 VM (build 6230) only. No HTML report was produced there, so there is no drawdown or profit factor. A pending MetaTrader LiveUpdate stops the run until it is approved on the Windows desktop. Local Wine backend not implemented or tested.

## [0.1.2] - 2026-10-02

### Added

- `languages/mql/runnables.scm` — a gutter ▶ next to `OnInit`, `OnTick`, `OnStart` and `OnCalculate` that offers the compile and syntax-check tasks (tag `mql5`).
- Deploy step in `scripts/compile-mql-remote.sh`: after a successful compile the `.ex5` is copied into `MQL5/Experts/<workspace>/...` on the VM (needs `MQL_VM_MQL5_ROOT`) and in a local MetaTrader if found, so it appears in the Strategy Tester. Controlled by `MQL_DEPLOY` (default `1`, `0` disables) and `MQL_LOCAL_MT5_MQL5`.

### Changed

- Registry preparation: the extension now declares its grammar in `extension.toml` (`[grammars.cpp]`, pinned to upstream tree-sitter-cpp) instead of relying on Zed's built-in one, and the vendored copy in `grammars/` is no longer committed. `extension.wasm` is no longer tracked. Author set in `extension.toml`.
- Zed tasks are now labelled "MQL: Compile" and "MQL: Syntax check" (the "(Windows over SSH)" suffix and the redundant `MQL_BACKEND=remote` prefix are gone; `remote` is the dispatcher default). If you bound keys to the old labels, update `task_name`.

### Removed

- `implement_plan.md` and `scripts/mql-compile-helper.swift` (the abandoned local Wine approach).

## [0.1.1] - 2026-10-02

First release with the `mql-lsp` language server and Windows-over-SSH compile tasks (`v0.1.0` was tagged with the same code but lacked the Intel macOS server asset).

### Added

- `scripts/mql-compile-helper.swift` — native arm64 Swift binary that initialises `NSApplication` before spawning Wine, providing the Cocoa event loop required by Wine's macOS display driver (`winemac.drv`) for headless MetaEditor compilation.
- `implement_plan.md` — detailed implementation plan for the MQL compile feature, covering a local Wine backend (macOS + Rosetta 2) and a remote Windows VM backend, shared log parser design, Zed task integration, and path-translation strategy.
- `lsp/` — `mql-lsp`, a Rust language server providing hover, completion and go-to-definition. It indexes `MQL5/Include` and the workspace (classes, enums, functions, `#define`s, doc comments) and ships a hand-written table of ~134 MQL5 built-ins (`lsp/data/builtins.json`). The MQL5 folder can be set with the `mql5Path` initialization option.
- Extension launches `mql-lsp` from `PATH`, or downloads it from the latest GitHub release (`mql-lsp-<arch>-<os>.tar.gz` / `.zip`).
- `.github/workflows/release-lsp.yml` — builds and uploads `mql-lsp` for macOS (arm64, x64), Linux and Windows on `v*` tags.
- `scripts/parse-mql-log.py` — decodes MetaEditor logs (UTF-16/UTF-8) and prints clickable `path:line:col: error|warning: message` lines; maps Wine/VM paths to local POSIX paths; exits non-zero on errors.
- `scripts/compile-mql-remote.sh` — syncs the workspace's `.mq5`/`.mqh` files to a Windows machine (tar over SSH, persistent connection), compiles with MetaEditor `/compile`, pulls back the log and `.ex5`, and maps errors (including ones in `MQL5/Include`) to local paths. Supports `--check` (MetaEditor `/s` syntax check only) and `.mqh` files via a `//###<path/to/Main.mq5>` first-line marker (relative to the workspace root). Settings come from env vars or `~/.config/zed-mql/env`. Verified against a real Windows 10 machine: ~1.7 s per compile.
- `scripts/compile-mql.sh` — backend dispatcher driven by `MQL_BACKEND` (`remote` default; `local` not implemented yet).
- `.zed/tasks.json` and `scripts/tasks.mql5-workspace.json` — "MQL: Compile" and "MQL: Syntax check" Zed tasks.

- `mql-lsp` falls back to standard MT5 install locations (`$MQL5_PATH`, `~/.config/zed-mql/MQL5`, the macOS Wine prefix, `~/.wine`, `C:/Program Files/MetaTrader 5/MQL5`) when the workspace is not inside an `MQL5` folder.

### Fixed

- Release workflow builds the Intel macOS `mql-lsp` by cross-compiling on an Apple-silicon runner (the `macos-13` runner never started).
- Hover docs no longer show MetaQuotes' ASCII banner comments (`+-----+ | ... |`); they are cleaned into plain sentences.

### Changed

- Language server is now `mql-lsp` instead of clangd; the clangd requirement and fallback flags were removed.
- Project scope is MQL5 only (MQL4 dropped from extension metadata, Cargo description and README).
- README rewritten to document the language server, settings, compile tasks and dev install.
- `.gitignore` now ignores `lsp/target/`.

### Known limitations

- Remote compile needs an interactive Windows session logged in (an RDP session was active during testing); behaviour with no logged-in user is untested.
- Local Wine compile backend is not working yet: headless `MetaEditor64.exe /compile` under the MT5-bundled Wine (via `~/.local/bin/mt5-wine`) spins at ~100% CPU and never writes a log, even with `winevulkan`/`d3d*`/`dxgi` disabled (reproduced 2026-10-02). Remote VM only for now.
- Completion is not context-aware (no member filtering after `.`/`::`); the indexer is regex-based.
- No Problems-panel diagnostics yet.


## [0.1.0] - 2026-09-18

### Added

- Initial release of MQL (MetaQuotes Language) extension for Zed editor.
- Tree-sitter grammar for MQL4/MQL5 syntax parsing.
- Syntax highlighting via `highlights.scm`.
- Code outline support via `outline.scm`.
- Bracket matching via `brackets.scm`.
- Language configuration (`config.toml`) with comment and bracket definitions.
- Compiled `extension.wasm` for Zed extension loading.
