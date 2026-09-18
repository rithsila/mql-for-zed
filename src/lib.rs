use zed_extension_api::{self as zed, LanguageServerId, Result, Worktree};

/// MQL Clangd extension for Zed.
///
/// Provides MQL4/MQL5 (MetaQuotes Language) support backed by clangd.
/// Registers `.mq4`, `.mq5`, and `.mqh` files as the "MQL" language and
/// launches a dedicated clangd instance with MQL-friendly fallback flags.
struct MqlExtension;

impl zed::Extension for MqlExtension {
    fn new() -> Self {
        Self
    }

    /// Locate `clangd` on PATH and build the command to launch it.
    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<zed::Command> {
        // Try the exact binary first, then fall back to versioned names.
        let clangd = worktree
            .which("clangd")
            .or_else(|| worktree.which("clangd-18"))
            .or_else(|| worktree.which("clangd-17"))
            .or_else(|| worktree.which("clangd-16"))
            .ok_or_else(|| {
                concat!(
                    "clangd not found in PATH.\n",
                    "Install it via:\n",
                    "  macOS:  brew install llvm  (then add /opt/homebrew/opt/llvm/bin to PATH)\n",
                    "  Ubuntu: sudo apt install clangd\n",
                    "  Arch:   sudo pacman -S clang"
                )
                .to_string()
            })?;

        Ok(zed::Command {
            command: clangd,
            args: vec![
                // Index the project in the background for faster completions.
                "--background-index".to_string(),
                // Don't auto-insert includes on completion — MetaEditor manages includes.
                "--header-insertion=never".to_string(),
                // Offer completions from all scopes, not just the current one.
                "--all-scopes-completion=true".to_string(),
                // Show full type signatures in completion details.
                "--completion-style=detailed".to_string(),
                // Suppress clangd's own log noise; real errors still show up.
                "--log=error".to_string(),
            ],
            env: Default::default(),
        })
    }

    /// Pass MQL-specific fallback compilation flags to clangd.
    ///
    /// These are used when no `compile_commands.json` or `compile_flags.txt`
    /// is present in the workspace root.  When those files exist (as generated
    /// by the MQL Clangd VS Code extension), clangd reads them automatically
    /// and these flags are ignored.
    fn language_server_initialization_options(
        &mut self,
        _server_id: &LanguageServerId,
        _worktree: &Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(Some(zed::serde_json::json!({
            "fallbackFlags": [
                // Treat MQL files as C++ source.
                "-xc++",
                "-std=c++17",
                // Core MQL preprocessor defines.
                "-D__MQL__",
                "-D__MQL5__",
                // MQL uses MS-style extensions (__int64, __cdecl, etc.).
                "-fms-extensions",
                "-fms-compatibility",
                // Don't cap error output — MQL headers trigger many cascading errors.
                "-ferror-limit=0",
                // Suppress all clangd warnings; MetaEditor is the authoritative compiler.
                "-Wno-everything"
            ]
        })))
    }
}

zed::register_extension!(MqlExtension);
