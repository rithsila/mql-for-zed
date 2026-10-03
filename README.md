# zed-mql

> MQL5 language support for [Zed](https://zed.dev).

Brings MQL5 (MetaQuotes Language), used for MetaTrader 5 Expert Advisors, indicators and scripts, into Zed: syntax highlighting, hover docs, completion, go-to-definition, and one-keystroke compile with clickable errors.

MQL5 only. MQL4 is out of scope.

## Features

| Feature | Status |
|---|---|
| Syntax highlighting and outline (`.mq5`, `.mqh`) | yes |
| Hover docs for ~130 common built-ins (`OrderSend`, `iMA`, `CopyRates`, ...) | yes |
| Hover, completion and go-to-definition for the standard library (`CTrade`, `CArrayObj`, ...) and your own workspace | yes |
| Compile and syntax check from Zed tasks, with clickable `file:line:col` errors | yes, via a Windows machine over SSH (see below) |
| Strategy Tester backtest from a Zed task (compile, deploy, run on the Windows machine, summary in the terminal) | yes, see "Backtest" below |
| Compile on macOS locally (Wine) | no (MetaEditor hangs when run headless under Wine) |
| Diagnostics in the Problems panel | not yet |

## Install

### From the Zed extension registry

Not published yet. Until then, install it as a dev extension.

### As a dev extension

1. Clone this repository and install Rust with the wasm target: `rustup target add wasm32-wasip2`.
2. In Zed: `cmd+shift+p`, then **zed: install dev extension**, and select the cloned folder.
3. Start Zed from a terminal (`zed .`) the first time so it can find tools on your `PATH`.

The extension downloads its language server (`mql-lsp`) from this repository's [GitHub releases](https://github.com/rithsila/mql-for-zed/releases) on first use. Prebuilt binaries exist for macOS (Apple silicon and Intel), Linux (x86_64) and Windows (x86_64).

To use your own build instead, put `mql-lsp` on your `PATH`:

```sh
cargo build --release --manifest-path lsp/Cargo.toml
rm -f ~/.local/bin/mql-lsp
cp lsp/target/release/mql-lsp ~/.local/bin/mql-lsp
codesign -s - -f ~/.local/bin/mql-lsp   # macOS: required after copying
```

On macOS always `rm` before `cp`. Overwriting a running, signed binary in place makes the system kill it.

## Language server

`mql-lsp` indexes `MQL5/Include` and your workspace and serves hover, completion and go-to-definition. Built-in functions (the ones that are not declared in any `.mqh`) come from a small hand-written table in [`lsp/data/builtins.json`](lsp/data/builtins.json). Contributions that add entries are welcome.

### Where it looks for the standard library

In order:

1. `initialization_options.mql5Path` in Zed settings
2. An `MQL5` folder found by walking up from your workspace
3. The `MQL5_PATH` environment variable
4. `~/.config/zed-mql/MQL5`
5. Common MetaTrader install locations (Wine prefixes on macOS and Linux, `C:/Program Files/MetaTrader 5/MQL5`)

If your project is not inside an `MQL5` folder, copy the `Include` folder from a MetaTrader installation to a permanent place such as `~/.config/zed-mql/MQL5/Include`, or point Zed at it:

```json
{ "lsp": { "mql-lsp": { "initialization_options": { "mql5Path": "/path/to/MQL5" } } } }
```

## Compile and syntax check

MetaEditor only runs on Windows, so compiling means running it on a Windows machine (a VM or another PC) that your Mac or Linux box can reach over SSH. The scripts copy your workspace's `.mq5` and `.mqh` files there with `tar` over SSH, so relative `#include`s resolve, run `MetaEditor64.exe /compile`, and print the errors as `path:line:col: error: message` lines you can click in Zed's terminal. A compile takes about two seconds.

### 1. Windows: enable SSH

Install OpenSSH Server (Settings, Optional features, or the [Win32-OpenSSH](https://github.com/PowerShell/Win32-OpenSSH/releases) zip), then in an **administrator** PowerShell:

```powershell
Set-Service sshd -StartupType Automatic
Start-Service sshd
```

Authorize your public key. For accounts in the Administrators group, the key goes in `C:\ProgramData\ssh\administrators_authorized_keys` and the file needs strict permissions:

```powershell
Set-Content -Path C:\ProgramData\ssh\administrators_authorized_keys -Encoding ascii -Value "<your .pub line>"
icacls C:\ProgramData\ssh\administrators_authorized_keys /inheritance:r /grant "Administrators:F" /grant "SYSTEM:F"
Restart-Service sshd
```

Regular accounts use `%USERPROFILE%\.ssh\authorized_keys` instead. If the Windows machine is a VM, use bridged or shared networking so your computer can reach it.

### 2. Your computer: add an SSH alias

```sh
ssh-keygen -t ed25519 -f ~/.ssh/mql_vm
```

Add to `~/.ssh/config`:

```
Host mqlvm
    HostName 10.0.0.5
    User youruser
    IdentityFile ~/.ssh/mql_vm
    IdentitiesOnly yes
```

Check that `ssh mqlvm 'whoami'` works without a password prompt.

### 3. Configure the scripts

MetaTrader keeps its data under `%APPDATA%\MetaQuotes\Terminal\<hash>`. Pick the folder whose `origin.txt` is your install path. Then create `~/.config/zed-mql/env`:

```sh
MQL_VM_HOST=mqlvm
MQL_VM_WORK=C:/Users/youruser/mql-work
MQL_VM_MQL5_ROOT=C:/Users/youruser/AppData/Roaming/MetaQuotes/Terminal/<hash>/MQL5
```

| Variable | Default | Purpose |
|---|---|---|
| `MQL_VM_HOST` | `mqlvm` | SSH host or alias |
| `MQL_VM_WORK` | `C:/Users/MT5/mql-work` | Folder on Windows where workspaces are synced |
| `MQL_VM_MQL5_ROOT` | none | Maps errors inside `MQL5/Include` back to your local copy |
| `MQL_VM_METAEDITOR` | `C:/Program Files/MetaTrader 5/MetaEditor64.exe` | Path to MetaEditor |
| `MQL_VM_INCLUDE` | none | Extra `/inc:` directory |
| `MQL_LOCAL_MQL5_ROOT` | `~/.config/zed-mql/MQL5` | Local `MQL5` folder used for include navigation |
| `MQL_WORKSPACE_ROOT` | git root of the file | Root folder that gets synced |
| `MQL_DEPLOY` | `1` | After a successful compile, copy the `.ex5` into `MQL5/Experts/<workspace>/...` on the VM (needs `MQL_VM_MQL5_ROOT`) and in a local MetaTrader if found, so it shows up in the Strategy Tester. `0` disables |
| `MQL_LOCAL_MT5_MQL5` | Wine install's `MQL5` dir | Local MetaTrader `MQL5` folder to deploy to |

### 4. Add the Zed tasks

Copy [`scripts/tasks.mql5-workspace.json`](scripts/tasks.mql5-workspace.json) to `.zed/tasks.json` in your MQL project. If you did not clone this repository to `~/Projects/zed-mql`, set `MQL_SCRIPTS_DIR` to its `scripts` folder.

Run a task with `cmd+shift+p`, **task: spawn**. The tasks act on the active file. If it is not an `.mq5` (for example `_tester.ini`, a `.set` or a log), the single `.mq5` in the same folder is used, or the one named like the folder; with several candidates the task stops and asks you to open the EA:

- **MQL: Compile** compiles and copies the resulting `.ex5` next to the source.
- **MQL: Syntax check** runs MetaEditor's `/s` check only.
- **MQL: Backtest** compiles, deploys and runs the EA in the Strategy Tester on the Windows machine, then prints a summary. See [Backtest](#backtest).

To compile a header, put this on its first line, with a path relative to the workspace root:

```
//###<MyEA/MyEA.mq5>
```

### Limitations of compiling

- Tested against Windows 10 with an RDP session logged in. Behavior with nobody logged in is untested.
- Files you delete locally stay in the synced folder on Windows. Remove that folder if it gets stale.
- Local compile through Wine does not work: headless MetaEditor spins at 100% CPU and never writes a log. Apple is also ending Rosetta, which MetaQuotes' Wine build depends on.

## Backtest

**MQL: Backtest** (`scripts/tester-mql-remote.sh`) compiles and deploys the EA, writes a tester `.ini`, runs `terminal64.exe /config:<ini>` on the Windows machine with `ShutdownTerminal=1`, waits for it to exit and prints a summary taken from the tester log: initial deposit, final balance, net result, number of deals, `OnTester` value, test time and data size. After each run it saves the results next to the `.mq5`: `<EA>.summary.txt` (the summary above), `<EA>.tester.log` (this run's tester log, can be several MB) and `<EA>.htm` if the terminal wrote a report (it did not in testing), and `<EA>.report.html` when the EA uses the helper below. They are overwritten on the next run; consider adding `*.summary.txt` and `*.tester.log` to your `.gitignore`.

Requirements and behaviour, as observed on Windows 10 with MetaTrader 5 build 6230:

- The tester needs an account to log in with. By default the script reads `Login` and `Server` from the terminal's `config\common.ini` (the last account you logged in with). The terminal must have been logged in once, with the password saved. The tester only reads quotes and trades nothing live, but it does connect to that account.
- The symbol and deposit currency must match that account's server (for example a cent account has `XAUUSDc` and `USC`). A wrong symbol makes the run fail with `tester symbol does not exist`, which the script reports.
- MetaTrader allows one instance per data folder. If a GUI terminal of the same install is running, the script closes it first through a scheduled task in your RDP session (`MQL_BT_CLOSE_GUI=0` aborts instead). It does not reopen it afterwards.
- MetaTrader must not have a LiveUpdate pending. A pending update makes the terminal exit immediately, with a UAC prompt that needs a click on the Windows desktop, and no test runs.
- The HTML report is not produced by this setup (the script asks for one and copies it if it appears, but none was written in testing). For drawdown, profit factor and similar, add the helper to your EA. [`mql/ZedMqlStats.mqh`](mql/ZedMqlStats.mqh) is uploaded to the VM's `MQL5/Include` on every compile; then in the EA:

  ```mql5
  #include <ZedMqlStats.mqh>
  double OnTester() { ZedMqlPrintStats(); /* your existing criterion */ }
  ```

  It prints one `ZEDMQL_STATS` line from `TesterStatistics()` inside the tester only, and the summary shows trades, profit factor, equity drawdown, recovery factor and Sharpe. It also writes the deal list to `Common/Files/zedmql_<EA>_deals.csv` on the VM; the script fetches it and writes `<EA>.report.html` next to the `.mq5` (statistics, win rate, average and largest win/loss, and a balance curve drawn from closed-deal results). The curve is balance, not equity: floating profit and loss is not sampled, so use the tester's equity drawdown figure for risk. Without the helper the summary says the figures are unavailable and no report is written.
- Only the remote backend exists. A local Wine run is untested.

**Inputs.** Without an explicit inputs file, the tester reuses the inputs last saved for that EA on the VM (`MQL5/Profiles/Tester/<EA>.set`, also rewritten by GUI runs), not the defaults in your source. In testing this gave a +153% result for FlexUltimateGRH where the source defaults gave -4%. So if `<EA>.set` is missing, the script writes one next to the `.mq5` from the literal `input` defaults (`scripts/gen-set.py`). Inputs whose default is not a plain literal (enums, expressions, macros) are left out, so the EA's compiled default applies; the file lists them in a comment. Edit the file to change inputs; delete its first line (`; generated by zed-mql ...`) to stop it being regenerated when the `.mq5` changes.

Defaults come from env vars, `~/.config/zed-mql/env`, then an optional `<workspace>/.zed/mql-tester.env` (later wins). Put per-project values in the last one:

```sh
MQL_BT_SYMBOL=XAUUSDc
MQL_BT_PERIOD=M5
MQL_BT_FROM=2026.01.01
MQL_BT_TO=2026.06.30
```

| Variable | Default | Purpose |
|---|---|---|
| `MQL_VM_TERMINAL` | `C:/Program Files/MetaTrader 5/terminal64.exe` | Terminal to run |
| `MQL_BT_SYMBOL` | `XAUUSDc` | Tester symbol, as the broker names it |
| `MQL_BT_PERIOD` | `M5` | Timeframe |
| `MQL_BT_MODEL` | `1` | `0` every tick, `1` 1-minute OHLC, `2` open prices, `4` real ticks |
| `MQL_BT_FROM`, `MQL_BT_TO` | last 30 days | `YYYY.MM.DD` |
| `MQL_BT_DEPOSIT` | `10000` | Initial deposit |
| `MQL_BT_CURRENCY` | `USC` | Must match the account currency |
| `MQL_BT_LEVERAGE` | `1:500` | Leverage |
| `MQL_BT_LOGIN`, `MQL_BT_SERVER` | from the terminal's `common.ini` | Account the tester logs in with |
| `MQL_BT_SET` | `<EA>.set` next to the source | Inputs file, uploaded to `MQL5/Profiles/Tester` and passed to the tester. If `<EA>.set` is missing it is generated from the `.mq5` (see below) |
| `MQL_BT_GENSET` | `1` | `0` turns generation off; the run then warns that the tester will use the inputs last saved on the VM |
| `MQL_BT_TIMEOUT` | `1800` | Seconds to wait before giving up (the run keeps going on the VM) |
| `MQL_BT_CLOSE_GUI` | `1` | Close a running GUI terminal of the same install first |

## Troubleshooting

| Symptom | Fix |
|---|---|
| No hover or completion | Run **editor: restart language server**. Open **zed: open log** and look for `mql-lsp` lines. |
| `server shut down` in the log | The binary was replaced while running. Reinstall it (`rm`, `cp`, `codesign`) and restart the language server. |
| `CTrade` has no hover | The standard library was not found. See [Where it looks for the standard library](#where-it-looks-for-the-standard-library). |
| `Permission denied` or a password prompt over SSH | The key is not authorized. For administrator accounts use `administrators_authorized_keys`. |
| `no compile log produced` | MetaEditor did not run. Run the `ssh` command by hand and check the MetaEditor path. |
| `.mqh needs first line //###<...>` | Add the marker line described above. |

## How it works

```
Zed ──► mql-lsp ── hover, completion, go-to-definition

Zed task ──► scripts/compile-mql.sh ──► tar over ssh ──► MetaEditor /compile (Windows)
                    ▲                                           │
   clickable errors ┴── scripts/parse-mql-log.py ◄── UTF-16 log ─┘
```

- `src/lib.rs` is the Zed extension: it starts or downloads `mql-lsp`.
- `lsp/` is the language server (Rust).
- `scripts/` holds the compile scripts and the task template.
- `languages/mql` provides syntax highlighting and the outline, using the upstream `tree-sitter-cpp` grammar (pinned in `extension.toml`).

## Contributing

Pull requests are welcome. Useful areas:

1. **More built-ins** in `lsp/data/builtins.json`. Write original descriptions; do not paste text from the MQL5 documentation.
2. **Problems-panel diagnostics**, for example by running the syntax check on save.
3. **Context-aware completion**, such as members after `.` and `::`.
4. **A working local compile path** for macOS or Linux.

Run the tests with `cargo test --manifest-path lsp/Cargo.toml`. Note changes under `[Unreleased]` in [`changelog.md`](changelog.md).

## License

MIT
