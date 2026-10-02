use std::fs;
use zed_extension_api::{self as zed, settings::LspSettings, LanguageServerId, Result, Worktree};

const REPO: &str = "rithsila/mql-for-zed";
const BINARY: &str = "mql-lsp";

/// MQL5 extension for Zed, backed by the bundled `mql-lsp` language server.
struct MqlExtension {
    cached_binary: Option<String>,
}

impl MqlExtension {
    fn binary_path(&mut self, id: &LanguageServerId, worktree: &Worktree) -> Result<String> {
        if let Some(path) = worktree.which(BINARY) {
            return Ok(path);
        }
        if let Some(path) = self
            .cached_binary
            .as_ref()
            .filter(|p| fs::metadata(p).is_ok())
        {
            return Ok(path.clone());
        }

        zed::set_language_server_installation_status(
            id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let release = zed::latest_github_release(
            REPO,
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (os, arch) = zed::current_platform();
        let os_name = match os {
            zed::Os::Mac => "apple-darwin",
            zed::Os::Linux => "unknown-linux-gnu",
            zed::Os::Windows => "pc-windows-msvc",
        };
        let arch_name = match arch {
            zed::Architecture::Aarch64 => "aarch64",
            zed::Architecture::X8664 => "x86_64",
            zed::Architecture::X86 => return Err("32-bit x86 is not supported".into()),
        };
        let (ext, file_type) = match os {
            zed::Os::Windows => ("zip", zed::DownloadedFileType::Zip),
            _ => ("tar.gz", zed::DownloadedFileType::GzipTar),
        };
        let asset_name = format!("{BINARY}-{arch_name}-{os_name}.{ext}");
        let asset = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| format!("no release asset named {asset_name}"))?;

        let dir = format!("{BINARY}-{}", release.version);
        let exe = if matches!(os, zed::Os::Windows) {
            format!("{BINARY}.exe")
        } else {
            BINARY.to_string()
        };
        let path = format!("{dir}/{exe}");

        if fs::metadata(&path).is_err() {
            zed::set_language_server_installation_status(
                id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );
            zed::download_file(&asset.download_url, &dir, file_type)
                .map_err(|e| format!("failed to download {asset_name}: {e}"))?;
            zed::make_file_executable(&path)?;
            if let Ok(entries) = fs::read_dir(".") {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(&format!("{BINARY}-")) && name != dir {
                        let _ = fs::remove_dir_all(entry.path());
                    }
                }
            }
        }

        zed::set_language_server_installation_status(
            id,
            &zed::LanguageServerInstallationStatus::None,
        );
        self.cached_binary = Some(path.clone());
        Ok(path)
    }
}

impl zed::Extension for MqlExtension {
    fn new() -> Self {
        Self {
            cached_binary: None,
        }
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<zed::Command> {
        Ok(zed::Command {
            command: self.binary_path(id, worktree)?,
            args: vec![],
            env: Default::default(),
        })
    }

    /// Settings: `{"lsp": {"mql-lsp": {"initialization_options": {"mql5Path": "/path/to/MQL5"}}}}`
    fn language_server_initialization_options(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(LspSettings::for_worktree(id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.initialization_options))
    }
}

zed::register_extension!(MqlExtension);
