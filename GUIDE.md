# zed-mql Guide

Complete setup and usage guide for MQL5 development in Zed on macOS, with compilation on a Windows machine over SSH.

## 1. What you get

| Feature | How it works |
|---|---|
| Syntax highlighting, outline | Tree-sitter grammar in `grammars/mql` |
| Hover docs, completion, go to definition | `mql-lsp` language server (`lsp/`) |
| Compile, syntax check, clickable errors | Zed tasks that run `scripts/compile-mql.sh` |

MQL5 only. MQL4 is not supported.

```
Zed ──► mql-lsp (hover / completion / definition)
 │
 └─► Task ──► scripts/compile-mql.sh ──► tar+ssh sync ──► Windows: MetaEditor /compile
                                             ▲                       │
                  clickable file:line:col ◄──┴── parse-mql-log.py ◄──┘ (UTF-16 log)
```

## 2. Requirements

- macOS with Zed installed.
- Rust with the wasm target, only if you build the extension or server from source:
  `rustup target add wasm32-wasip2`
- A Windows machine (VM or PC) with MetaTrader 5 and MetaEditor installed and reachable by SSH. Needed only for compile and syntax check.
- `python3`, `ssh`, `scp`, `tar` on the Mac (all present on macOS).

## 3. Install the extension (dev mode)

1. Build the language server:
   ```sh
   cd ~/Projects/zed-mql
   cargo build --release --manifest-path lsp/Cargo.toml
   ```
2. Install the binary. Always `rm` before `cp`, and sign it. Overwriting a signed binary in place makes macOS kill it:
   ```sh
   mkdir -p ~/.local/bin
   rm -f ~/.local/bin/mql-lsp
   cp lsp/target/release/mql-lsp ~/.local/bin/mql-lsp
   codesign -s - -f ~/.local/bin/mql-lsp
   ```
3. In Zed: `cmd+shift+p` → **zed: install dev extension** → select `~/Projects/zed-mql`.
4. Start Zed from a terminal (`zed .`) so it inherits your `PATH` and can find `mql-lsp`.
5. After later changes: Extensions panel → MQL → **Rebuild**, then `cmd+shift+p` → **editor: restart language server**.

Once a GitHub release exists (section 10), Zed downloads `mql-lsp` automatically and steps 1 and 2 are not needed.

## 4. Give the language server the MQL5 standard library

The server indexes `MQL5/Include` (for `CTrade`, `CArrayObj`, and so on) plus your workspace. It looks for it in this order:

1. `initialization_options.mql5Path` from Zed settings
2. An `MQL5` folder found by walking up from the workspace
3. `$MQL5_PATH`
4. `~/.config/zed-mql/MQL5`
5. The macOS Wine prefix and `~/.wine` locations

Recommended: keep a permanent copy so you do not depend on a local MetaTrader install:

```sh
mkdir -p ~/.config/zed-mql/MQL5
scp -r 'mqlvm:C:/Users/MT5/AppData/Roaming/MetaQuotes/Terminal/<HASH>/MQL5/Include' ~/.config/zed-mql/MQL5/
```

Rerun this after MetaTrader updates its headers.

To set the path explicitly, add to Zed `settings.json`:

```json
{ "lsp": { "mql-lsp": { "initialization_options": { "mql5Path": "/path/to/MQL5" } } } }
```

Built-in functions (`OrderSend`, `iMA`, `CopyRates`, …) come from a hand-written table in `lsp/data/builtins.json`. It covers about 134 common entries.

## 5. Set up the Windows machine for SSH

### 5.1 Install and start OpenSSH Server

Open PowerShell **as Administrator** (Start → type `powershell` → right-click → Run as administrator).

Try the built-in installer first:

```powershell
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
```

If it fails (for example `0x80072ee6`), use the offline installer:

```powershell
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
Invoke-WebRequest -Uri "https://github.com/PowerShell/Win32-OpenSSH/releases/latest/download/OpenSSH-Win64.zip" -OutFile "$env:TEMP\OpenSSH-Win64.zip"
Expand-Archive "$env:TEMP\OpenSSH-Win64.zip" -DestinationPath "C:\Program Files" -Force
Rename-Item "C:\Program Files\OpenSSH-Win64" "C:\Program Files\OpenSSH" -ErrorAction SilentlyContinue
cd "C:\Program Files\OpenSSH"
powershell -ExecutionPolicy Bypass -File .\install-sshd.ps1
New-NetFirewallRule -Name sshd -DisplayName "OpenSSH Server" -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22
```

Then, in both cases:

```powershell
Set-Service sshd -StartupType Automatic
Start-Service sshd
Get-Service sshd        # must show Running
ipconfig                # note the IPv4 address
whoami                  # note the user name
```

If the Windows machine is a VM with NAT-only networking, switch it to Bridged or Shared so the Mac can reach it. Check from the Mac: `nc -z -G 3 <ip> 22`.

### 5.2 Create a key on the Mac

```sh
ssh-keygen -t ed25519 -f ~/.ssh/mql_vm -C "mql-vm"
cat ~/.ssh/mql_vm.pub
```

### 5.3 Authorize the key on Windows

Check whether your user is an administrator: `net localgroup Administrators`.

**Administrator account** (admin PowerShell):

```powershell
Set-Content -Path C:\ProgramData\ssh\administrators_authorized_keys -Encoding ascii -Value "<paste the .pub line>"
icacls C:\ProgramData\ssh\administrators_authorized_keys /inheritance:r /grant "Administrators:F" /grant "SYSTEM:F"
Restart-Service sshd
```

**Regular account:**

```powershell
mkdir $env:USERPROFILE\.ssh -Force
Set-Content -Path $env:USERPROFILE\.ssh\authorized_keys -Encoding ascii -Value "<paste the .pub line>"
Restart-Service sshd
```

### 5.4 Add an alias on the Mac

Append to `~/.ssh/config`:

```
Host mqlvm
    HostName <windows-ip>
    User <windows-user>
    IdentityFile ~/.ssh/mql_vm
    IdentitiesOnly yes
    StrictHostKeyChecking accept-new
```

Test (must print the Windows user with no password prompt):

```sh
ssh mqlvm 'whoami & hostname'
```

### 5.5 Find the MT5 data folder on Windows

MetaTrader keeps its data under `AppData\Roaming\MetaQuotes\Terminal\<HASH>`. To find the hash of your install:

```sh
ssh mqlvm 'powershell -NoProfile -Command "Get-ChildItem $env:APPDATA\MetaQuotes\Terminal -Directory | ForEach-Object { \"{0}  {1}\" -f $_.Name, ((Get-Content (Join-Path $_.FullName origin.txt) -ErrorAction SilentlyContinue) -join \"\") }"'
```

Use the folder whose origin line equals your MetaTrader install path (for example `C:\Program Files\MetaTrader 5`).

## 6. Configure the compile scripts

Create `~/.config/zed-mql/env`:

```sh
MQL_VM_HOST=mqlvm
MQL_VM_WORK=C:/Users/<user>/mql-work
MQL_VM_MQL5_ROOT=C:/Users/<user>/AppData/Roaming/MetaQuotes/Terminal/<HASH>/MQL5
```

Optional settings (env vars or the same file):

| Variable | Default | Purpose |
|---|---|---|
| `MQL_VM_HOST` | `mqlvm` | SSH host or alias |
| `MQL_VM_WORK` | `C:/Users/MT5/mql-work` | Where workspaces are synced on Windows |
| `MQL_VM_MQL5_ROOT` | none | Maps errors inside `MQL5/Include` back to your local copy |
| `MQL_VM_METAEDITOR` | `C:/Program Files/MetaTrader 5/MetaEditor64.exe` | MetaEditor path |
| `MQL_VM_INCLUDE` | none | Extra `/inc:` directory |
| `MQL_LOCAL_MQL5_ROOT` | `~/.config/zed-mql/MQL5` | Local MQL5 folder for include navigation |
| `MQL_WORKSPACE_ROOT` | git toplevel of the file | Root that gets synced |
| `MQL_DEPLOY` | `1` | After a successful compile, copy the `.ex5` into `MQL5/Experts/<workspace>/...` on the VM (needs `MQL_VM_MQL5_ROOT`) and in a local MetaTrader if found, so it shows up in the Strategy Tester. `0` disables |
| `MQL_LOCAL_MT5_MQL5` | Wine install's `MQL5` dir | Local MetaTrader `MQL5` folder to deploy to |
| `MQL_CONFIG` | `~/.config/zed-mql/env` | Alternate config file |

## 7. Add Zed tasks to your workspace

Copy the template into your MQL project (for example `EA-Research-Lab`):

```sh
mkdir -p <workspace>/.zed
cp ~/Projects/zed-mql/scripts/tasks.mql5-workspace.json <workspace>/.zed/tasks.json
```

If `zed-mql` is not at `~/Projects/zed-mql`, set `MQL_SCRIPTS_DIR` in your environment.

Tasks:

- **MQL: Compile** compiles and copies the `.ex5` back next to the source.
- **MQL: Syntax check** runs MetaEditor's `/s` check only.

### Opt-in Problems-panel compiler checks

Install the current `mql-lsp` binary, then set Zed `settings.json`:

```json
{ "lsp": { "mql-lsp": { "initialization_options": { "compilerCheck": { "enabled": true, "script": "/absolute/path/to/zed-mql/scripts/compile-mql.sh" } } } } }
```

Restart the language server. Disabled by default, this runs `--check` on save; it **syncs `.mq5` and `.mqh` files to your configured Windows SSH host** but does not retrieve or deploy an EA. Keep the existing first-line `.mqh` marker for header checks. Set `MQL_VM_MQL5_ROOT`/`MQL_LOCAL_MQL5_ROOT` to map standard-library diagnostics. If the worktree differs from your source root, set the absolute `compilerCheck.workspaceRoot` option. `~/.config/zed-mql/env` is sourced as shell code by the existing scripts: use a trusted config only. Checks debounce saves and discard stale snapshots; failed SSH/launch/log checks do not clear earlier compiler findings. Compiler columns use one-based UTF-16 offsets in MetaEditor's log and are converted to LSP zero-based UTF-16 positions (verified against a Windows Unicode fixture). See README for the v1 `--json` result contract.

## 8. Daily workflow

1. Open a `.mq5` or `.mqh` file in Zed.
2. Hover for docs, type for completion, cmd-click to jump to a definition.
3. Run a task, any of these:
   - Click the ▶ in the gutter next to `OnInit`, `OnTick`, `OnStart` or `OnCalculate` (from `languages/mql/runnables.scm`; both tasks carry the `mql5` tag).
   - Keybindings in `~/.config/zed/keymap.json`: `cmd-shift-b` runs Compile, `cmd-alt-b` runs Syntax check (see below).
   - `cmd+shift+p` → **task: spawn** → pick Compile or Syntax check.
4. Errors appear in the terminal as `path:line:col: error|warning: message`. Cmd-click one to jump to it.

Typical compile time is under 2 seconds.

### Keybindings

Add to the `Workspace` context in `~/.config/zed/keymap.json`:

```json
{
  "context": "Workspace",
  "bindings": {
    "cmd-shift-b": ["task::Spawn", { "task_name": "MQL: Compile" }],
    "cmd-alt-b": ["task::Spawn", { "task_name": "MQL: Syntax check" }]
  }
}
```

Zed has no toolbar button API, so the gutter ▶ and keybindings are the one-click options.

### Compiling a `.mqh`

A header cannot be compiled alone. Put this on its first line, with a path relative to the workspace root:

```
//###<FlexUltimateGRH/FlexUltimateGRH.mq5>
```

The task then compiles that program.

### What the compile script does

1. Finds the workspace root (git toplevel).
2. Syncs all `.mq5` and `.mqh` files to `MQL_VM_WORK/<workspace-name>` on Windows with `tar` over SSH, so relative `#include`s resolve.
3. Runs `MetaEditor64.exe /compile:… /log:…` and waits for it.
4. Fetches the UTF-16 log and prints errors with local paths.
5. On success, copies the `.ex5` back next to your source.

Files deleted on the Mac are not deleted on the VM copy. Remove old copies by hand if needed: `ssh mqlvm 'rmdir /s /q C:\Users\<user>\mql-work\<workspace>'`.

### AI Agent Integration (MCP)

The project includes a native Rust **MCP Server** (`mql-mcp`) designed to let AI agents safely interact with MQL5 tools. It provides tools for syntax checking, compiling, and launching bounded asynchronous backtests without letting the agent execute arbitrary shell commands. It tracks Job IDs and pollable states so that long-running backtests can survive client disconnections.

To run the MCP server:

1. Build it from source:
   ```sh
   cd ~/Projects/zed-mql/mql-mcp
   cargo build --release
   ```
2. Configure your AI agent (like Claude Desktop or Zed's built-in MCP client) to start the `mql-mcp` binary via standard input/output. For example, in Zed's `settings.json`:
   ```json
   {
     "mcp": {
       "servers": {
         "mql-mcp": {
           "command": "/absolute/path/to/zed-mql/mql-mcp/target/release/mql-mcp",
           "args": []
         }
       }
     }
   }
   ```
   
The server supports the following tools automatically:
- `mql_doctor`: Check tools, host, and tester readiness.
- `mql_lint`: Run local MQL5 lint findings.
- `mql_compile`: Start a syntax-check or full compile job.
- `mql_backtest`: Start an asynchronous backtest on the configured Windows machine.
- `mql_job_status`: Poll an ongoing backtest or compile job.
- `mql_job_cancel`: Cancel a pending or running job.
- `mql_list_runs`, `mql_get_run`, `mql_compare_runs`: Read, query, and compare the immutable backtest runs saved in `.mql/runs/`.

## 9. Troubleshooting

| Symptom | Cause and fix |
|---|---|
| No hover or completion | Run **editor: restart language server**. Check `zed: open log` for `mql-lsp` lines. If the log says `server shut down`, the process was killed (see below). |
| `server shut down` in the log | The binary was replaced or killed. Rebuild using `rm` then `cp` then `codesign`, then restart the language server. |
| Zed cannot find `mql-lsp` | Put it in `~/.local/bin` and start Zed from a terminal (`zed .`). |
| `CTrade` has no hover | Standard library not found. Do section 4 and check `ls ~/.config/zed-mql/MQL5/Include/Trade/Trade.mqh`. |
| `ssh: connection refused` | `sshd` not running or firewall. On Windows: `Get-Service sshd`, `Start-Service sshd`. |
| Password prompt appears | Key not authorized. Re-check section 5.3 (administrator accounts use `administrators_authorized_keys`). |
| `tar: Write error` | The remote command did not read the data. Make sure the script's `mkdir … 2>nul & tar …` form is used (not `if not exist … & tar`, which is conditional in cmd). |
| `no compile log produced` | MetaEditor did not run. Run the SSH command by hand, check the MetaEditor path, and try with an RDP session logged in. |
| `.mqh needs first line //###<…>` | Add the marker line, relative to the workspace root. |
| Paths in errors point nowhere | Set `MQL_VM_MQL5_ROOT` and keep the local Include copy (section 4). |
| Stale results | Rerun the task. It re-syncs every time. |

## 10. Publishing to the Zed registry

1. The repository is `rithsila/mql-for-zed` (already set in `extension.toml` and `src/lib.rs`).
2. Push the code to the default branch.
3. Tag a release: `git tag v0.1.0 && git push origin v0.1.0`. The workflow `.github/workflows/release-lsp.yml` builds `mql-lsp` for macOS (arm64, x64), Linux and Windows and uploads assets named `mql-lsp-<arch>-<os>.tar.gz` (or `.zip` on Windows). The extension downloads these on first use. v0.1.1 is released with assets for all platforms.
4. Clean install tested: with no `mql-lsp` on `PATH`, the extension downloaded `mql-lsp-v0.1.1` into Zed's `extensions/work/mql` and hover worked.
5. Submit the extension to the `zed-industries/extensions` repository following Zed's extension publishing docs. Their rules require an accepted open-source license (this repo has MIT in `LICENSE`).

## 11. Known limitations

- Local Wine compile does not work. Headless `MetaEditor64.exe /compile` under MetaQuotes' bundled Wine spins at about 100% CPU and never writes a log. Also, that Wine needs Rosetta, which Apple is phasing out after macOS 27.
- Remote compile was tested with an active RDP session on Windows. Behaviour with nobody logged in is untested.
- Problems-panel compiler diagnostics require explicit opt-in and a reachable Windows MetaEditor; a syntax-only Unicode fixture was verified on the configured Windows VM, but the Zed Problems UI still needs direct inspection.
- Completion is not context-aware (no filtering after `.` or `::`). The indexer is regex-based and can miss unusual declarations.
- Hover docs for built-ins cover only the hand-written table (about 134 entries).
- Zed extensions cannot add menu items, commands or colour pickers, so features such as MQL-Tools' context menu are not possible.

## 12. Project layout

```
zed-mql/
├── src/lib.rs                      Zed extension (starts or downloads mql-lsp)
├── extension.toml                  Extension manifest
├── grammars/mql, languages/mql     Tree-sitter grammar and language config
├── lsp/                            mql-lsp language server (Rust)
│   ├── src/                        main.rs (LSP), index.rs (header indexer)
│   └── data/builtins.json          Hand-written built-in function table
├── scripts/
│   ├── compile-mql.sh              Dispatcher (MQL_BACKEND)
│   ├── compile-mql-remote.sh       Sync + compile over SSH
│   ├── parse-mql-log.py            MetaEditor log → clickable diagnostics
│   └── tasks.mql5-workspace.json   Task template for your workspace
├── .github/workflows/release-lsp.yml
├── README.md, GUIDE.md, changelog.md
```

## 13. Maintaining

- Add built-ins: edit `lsp/data/builtins.json` (`name`, `kind`, `signature`, `doc`), rebuild, reinstall (section 3).
- Run tests: `cargo test --manifest-path lsp/Cargo.toml`.
- Update the changelog under `[Unreleased]` for every change.
