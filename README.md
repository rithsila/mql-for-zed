# zed-mql

> MQL5 language support for [Zed](https://zed.dev).

Brings MQL (MetaQuotes Language) — used for MetaTrader 4 and MetaTrader 5 Expert Advisors, indicators, and scripts — into Zed with first-class editor support.

## Features

| Feature | Status |
|---|---|
| Syntax highlighting (`.mq5`, `.mqh`) | yes |
| Hover docs: ~130 built-ins (hand-written) + everything declared in `MQL5/Include` and your workspace | yes |
| Completion for built-ins, standard library, and workspace symbols | yes |
| Go to definition (Include + workspace) | yes |
| Compile / syntax check via Windows VM task | yes (see `scripts/`) |
| Local Wine compile | not yet (experimental helper source in `scripts/`) |
| Diagnostics in the Problems panel | not yet |

MQL5 only. MQL4 is out of scope.

Full setup and troubleshooting: see [GUIDE.md](GUIDE.md).

## Install (dev)

`cmd+shift+p` → **zed: install dev extension** → select this folder. Build the server once with `cargo build --release --manifest-path lsp/Cargo.toml` and put `lsp/target/release/mql-lsp` on your `PATH` until a release exists.

## Language server

The extension runs `mql-lsp` (source in `lsp/`). It is downloaded from GitHub releases automatically, or you can put your own build on `PATH`.
It finds `MQL5/Include` by walking up from the workspace folder. Override in Zed settings:

```json
{ "lsp": { "mql-lsp": { "initialization_options": { "mql5Path": "/path/to/MQL5" } } } }
```

## Compile tasks (Windows machine over SSH)

MetaEditor is compiled on a Windows machine (VM or PC) reachable by SSH. The workspace's `.mq5`/`.mqh` files are synced there with `tar` over SSH, so relative `#include`s resolve, then MetaEditor runs `/compile` and errors are printed as clickable `file:line:col` lines. Typical run time is under 2 seconds.

**1. Windows:** install OpenSSH Server (Settings > Optional features, or the `Win32-OpenSSH` zip), start the `sshd` service, and authorize your key (for administrator accounts the key goes in `C:\ProgramData\ssh\administrators_authorized_keys`).

**2. Mac:** add a host alias to `~/.ssh/config`:

```
Host mqlvm
    HostName 10.0.0.5
    User youruser
    IdentityFile ~/.ssh/mql_vm
    IdentitiesOnly yes
```

**3. Config** in `~/.config/zed-mql/env`:

```sh
MQL_VM_HOST=mqlvm
MQL_VM_WORK=C:/Users/youruser/mql-work
MQL_VM_MQL5_ROOT=C:/Users/youruser/AppData/Roaming/MetaQuotes/Terminal/<hash>/MQL5
```

Optional: `MQL_VM_METAEDITOR`, `MQL_VM_INCLUDE`, `MQL_LOCAL_MQL5_ROOT`, `MQL_WORKSPACE_ROOT`.

**4. Zed tasks:** copy `scripts/tasks.mql5-workspace.json` to `.zed/tasks.json` in your workspace. Tasks: **MQL: Compile** (also copies the `.ex5` back next to the source) and **MQL: Syntax check**. For a `.mqh`, put `//###<path/to/Main.mq5>` (relative to the workspace root) on the first line to choose which program to compile.

## Contributing

PRs welcome. The two most impactful improvements would be:

1. **A dedicated tree-sitter-mql grammar** — currently we reuse tree-sitter-cpp,
   which parses MQL correctly but doesn't know MQL-specific directives like
   `#property` and `#import`.
2. **Compile integration** — a task provider that calls MetaEditor's CLI.

## License

MIT
