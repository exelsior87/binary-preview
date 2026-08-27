use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use zed_extension_api as zed;

const GITHUB_REPOSITORY: &str = "exelsior87/binary-preview";

struct BinaryPreviewExtension {
    cached_binary_path: Option<String>,
}

impl BinaryPreviewExtension {
    fn release_asset_name() -> zed::Result<&'static str> {
        match zed::current_platform() {
            (zed::Os::Windows, zed::Architecture::X8664) => {
                Ok("binary-preview-lsp-windows-x86_64.exe")
            }
            (zed::Os::Linux, zed::Architecture::X8664) => Ok("binary-preview-lsp-linux-x86_64"),
            (zed::Os::Mac, zed::Architecture::X8664) => Ok("binary-preview-lsp-macos-x86_64"),
            (zed::Os::Mac, zed::Architecture::Aarch64) => Ok("binary-preview-lsp-macos-aarch64"),
            (os, architecture) => Err(format!(
                "binary-preview-lsp does not support {os:?}/{architecture:?}"
            )),
        }
    }

    fn language_server_binary_path(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<String> {
        if let Some(path) = worktree.which("binary-preview-lsp") {
            return Ok(path);
        }

        if let Some(path) = self.cached_binary_path.as_ref() {
            return Ok(path.clone());
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = match zed::latest_github_release(
            GITHUB_REPOSITORY,
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        ) {
            Ok(release) => release,
            Err(error) => {
                if let Some(path) = Self::cached_binary_path_on_disk() {
                    self.cached_binary_path = Some(path.clone());
                    return Ok(path);
                }
                return Err(error);
            }
        };
        let asset_name = Self::release_asset_name()?;
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| {
                format!(
                    "release {} has no asset named {asset_name}",
                    release.version
                )
            })?;

        let version_dir = format!("binary-preview-lsp-{}", release.version);
        let binary_name = if matches!(zed::current_platform().0, zed::Os::Windows) {
            "binary-preview-lsp.exe"
        } else {
            "binary-preview-lsp"
        };
        let binary_path = format!("{version_dir}/{binary_name}");

        if !fs::metadata(&binary_path).is_ok_and(|metadata| metadata.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );
            fs::create_dir_all(&version_dir)
                .map_err(|error| format!("failed to create {version_dir}: {error}"))?;
            zed::download_file(
                &asset.download_url,
                &binary_path,
                zed::DownloadedFileType::Uncompressed,
            )
            .map_err(|error| format!("failed to download {asset_name}: {error}"))?;
            zed::make_file_executable(&binary_path)
                .map_err(|error| format!("failed to make {binary_path} executable: {error}"))?;
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }

    fn cached_binary_path_on_disk() -> Option<String> {
        let binary_name = if matches!(zed::current_platform().0, zed::Os::Windows) {
            "binary-preview-lsp.exe"
        } else {
            "binary-preview-lsp"
        };

        fs::read_dir(".")
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let directory_name = entry.file_name();
                if !directory_name
                    .to_string_lossy()
                    .starts_with("binary-preview-lsp-")
                {
                    return None;
                }

                let path = entry.path().join(binary_name);
                let metadata = path.metadata().ok()?;
                if !metadata.is_file() {
                    return None;
                }

                let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                Some((modified, path))
            })
            .max_by_key(|(modified, _)| *modified)
            .and_then(|(_, path)| Self::relative_path(path))
    }

    fn relative_path(path: PathBuf) -> Option<String> {
        let path = path.strip_prefix(Path::new(".")).unwrap_or(&path);
        path.to_str().map(str::to_owned)
    }
}

impl zed::Extension for BinaryPreviewExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        let command = self
            .language_server_binary_path(language_server_id, worktree)
            .inspect_err(|error| {
                zed::set_language_server_installation_status(
                    language_server_id,
                    &zed::LanguageServerInstallationStatus::Failed(error.clone()),
                );
            })?;

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::None,
        );

        Ok(zed::Command {
            command,
            args: vec![],
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(BinaryPreviewExtension);
