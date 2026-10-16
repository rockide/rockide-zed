use std::{env, fs};
use zed_extension_api::{
    self as zed, Result,
    serde_json::{self},
};

struct RockideExtension {
    cached_binary_path: Option<String>,
    cached_json_configuration: Option<serde_json::Value>,
}

impl RockideExtension {
    fn get_binary_path(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<String> {
        if let Some(path) = worktree.which("rockide") {
            return Ok(path);
        }

        if let Some(path) = &self.cached_binary_path
            && fs::metadata(path).is_ok_and(|m| m.is_file())
        {
            return Ok(path.to_owned());
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let (os, arch) = zed::current_platform();
        let binary_name = match os {
            zed::Os::Windows => "rockide.exe",
            _ => "rockide",
        };
        let binary_path = format!("bin/{binary_name}");
        let release = zed::latest_github_release(
            "rockide/language-server",
            zed::GithubReleaseOptions {
                pre_release: false,
                require_assets: true,
            },
        );

        let binary_exists = fs::metadata(&binary_path).is_ok_and(|m| m.is_file());
        if binary_exists {
            let is_latest = match &release {
                Ok(release) => {
                    fs::read_to_string("bin_version").is_ok_and(|v| v == release.version)
                }
                Err(_) => true,
            };
            if is_latest {
                zed::set_language_server_installation_status(
                    language_server_id,
                    &zed::LanguageServerInstallationStatus::None,
                );

                self.cached_binary_path = Some(binary_path.clone());
                return Ok(binary_path);
            }
        }

        let release = release?;
        let asset_name = format!(
            "rockide_{version}_{os}_{arch}.tar.gz",
            version = release.version,
            os = match os {
                zed::Os::Mac => "darwin",
                zed::Os::Linux => "linux",
                zed::Os::Windows => "windows",
            },
            arch = match arch {
                zed::Architecture::Aarch64 => "arm64",
                zed::Architecture::X8664 => "amd64",
                _ => return Err(format!("unsupported architecture: {arch:?}")),
            }
        );
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("failed to find asset: {asset_name}"))?;

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::Downloading,
        );

        if binary_exists {
            fs::rename(&binary_path, format!("{binary_path}.old")).map_err(|e| e.to_string())?;
        }
        zed::download_file(&asset.download_url, "bin", zed::DownloadedFileType::GzipTar)?;
        zed::make_file_executable(&binary_path)?;
        fs::write("bin_version", release.version).map_err(|e| e.to_string())?;

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::None,
        );

        Ok(binary_path)
    }

    fn get_json_configuration(&mut self) -> Result<Option<serde_json::Value>> {
        if let Some(config) = &self.cached_json_configuration {
            return Ok(Some(config.clone()));
        }
        self.fetch_schemas()?;

        let schemas_dir = env::current_dir()
            .map_err(|e| e.to_string())?
            .join("schemas");

        let package_json_path = schemas_dir.join("package.json");
        let package_json = fs::read_to_string(&package_json_path)
            .map_err(|e| format!("failed to read {}: {e}", package_json_path.display()))?;
        let package: serde_json::Value =
            serde_json::from_str(&package_json).map_err(|e| e.to_string())?;

        let registry = package
            .pointer("/contributes/jsonValidation")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                "schemas/package.json is missing contributes.jsonValidation".to_string()
            })?;

        let mut schemas = Vec::with_capacity(registry.len());
        for entry in registry {
            let file_match = entry
                .get("fileMatch")
                .and_then(|v| v.as_array())
                .ok_or_else(|| "schema entry is missing fileMatch".to_string())?;
            let url = entry
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "schema entry is missing url".to_string())?;

            let url = resolve_schema_url(&schemas_dir, url);
            schemas.push(serde_json::json!({
                "fileMatch": file_match,
                "url": url,
            }));
        }

        let config = serde_json::json!({
            "json": { "schemas": schemas }
        });
        self.cached_json_configuration = Some(config.clone());
        Ok(Some(config))
    }

    fn fetch_schemas(&self) -> Result<()> {
        let release = zed::latest_github_release(
            "rockide/schemas",
            zed::GithubReleaseOptions {
                pre_release: false,
                require_assets: true,
            },
        );

        let schemas_path = "schemas";
        if fs::metadata(schemas_path).is_ok_and(|m| m.is_dir()) {
            let is_latest = match &release {
                Ok(release) => {
                    fs::read_to_string("schemas_version").is_ok_and(|v| v == release.version)
                }
                Err(_) => true,
            };
            if is_latest {
                return Ok(());
            }
        }

        let release = release?;
        let asset_name = "schemas.tar.gz";
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("failed to find asset: {asset_name}"))?;

        zed::download_file(
            &asset.download_url,
            schemas_path,
            zed_extension_api::DownloadedFileType::GzipTar,
        )?;
        fs::write("schemas_version", release.version).map_err(|e| e.to_string())?;

        Ok(())
    }
}

fn resolve_schema_url(schemas_dir: &std::path::Path, url: &str) -> String {
    if url.starts_with("file://") || url.starts_with("http://") || url.starts_with("https://") {
        return url.to_string();
    }
    let relative = url
        .trim_start_matches("./")
        .trim_start_matches(".\\")
        .trim_start_matches('/')
        .trim_start_matches('\\');
    path_to_file_url(&schemas_dir.join(relative))
}

fn path_to_file_url(path: &std::path::Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let s = s.trim_start_matches('/');
    format!("file:///{s}")
}

impl zed::Extension for RockideExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
            cached_json_configuration: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        Ok(zed::Command::new(
            self.get_binary_path(language_server_id, worktree)?,
        ))
    }

    fn language_server_additional_workspace_configuration(
        &mut self,
        _language_server_id: &zed_extension_api::LanguageServerId,
        target_language_server_id: &zed_extension_api::LanguageServerId,
        _worktree: &zed_extension_api::Worktree,
    ) -> Result<Option<serde_json::Value>> {
        match target_language_server_id.as_ref() {
            "json-language-server" => self.get_json_configuration(),
            _ => Ok(None),
        }
    }
}

zed::register_extension!(RockideExtension);
