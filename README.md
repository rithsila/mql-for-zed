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

Run a task with `cmd+shift+p`, **task: spawn**:

- **MQL: Compile** compiles and copies the resulting `.ex5` next to the source.
- **MQL: Syntax check** runs MetaEditor's `/s` check only.

To compile a header, put this on its first line, with a path relative to the workspace root:

```
//###<MyEA/MyEA.mq5>
```

### Limitations of compiling

- Tested against Windows 10 with an RDP session logged in. Behavior with nobody logged in is untested.
- Files you delete locally stay in the synced folder on Windows. Remove that folder if it gets stale.
- Local compile through Wine does not work: headless MetaEditor spins at 100% CPU and never writes a log. Apple is also ending Rosetta, which MetaQuotes' Wine build depends on.

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
- `grammars/mql` and `languages/mql` provide syntax highlighting and the outline.

## Contributing

Pull requests are welcome. Useful areas:

1. **More built-ins** in `lsp/data/builtins.json`. Write original descriptions; do not paste text from the MQL5 documentation.
2. **Problems-panel diagnostics**, for example by running the syntax check on save.
3. **Context-aware completion**, such as members after `.` and `::`.
4. **A working local compile path** for macOS or Linux.

Run the tests with `cargo test --manifest-path lsp/Cargo.toml`. Note changes under `[Unreleased]` in [`changelog.md`](changelog.md).

## License

MIT
