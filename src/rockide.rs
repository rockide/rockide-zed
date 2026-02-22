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

        let root = env::current_dir()
            .map_err(|e| e.to_string())?
            .join("schemas")
            .display()
            .to_string();

        let match_bp = |path: &str| -> Vec<String> {
            ["behavior_pack", "*BP", "BP_*", "*bp", "bp_*"]
                .iter()
                .map(|bp| format!("{bp}/{path}"))
                .collect()
        };
        let match_rp = |path: &str| -> Vec<String> {
            ["resource_pack", "*RP", "RP_*", "*rp", "rp_*"]
                .iter()
                .map(|rp| format!("{rp}/{path}"))
                .collect()
        };
        let schema = |name: &str, file_match: Vec<String>| -> serde_json::Value {
            serde_json::json!({
                "fileMatch": file_match,
                "url": format!("file:///{root}/{name}/main.json")
            })
        };

        #[rustfmt::skip]
        let schemas = vec![
            // Behavior Pack schemas
            schema("aimAssistCategories", match_bp("aim_assist/categories/**/*.{json,jsonc,json5}")),
            schema("aimAssistPreset", match_bp("aim_assist/presets/**/*.{json,jsonc,json5}")),
            schema("animationController", match_bp("animation_controllers/**/*.{json,jsonc,json5}")),
            schema("animation", match_bp("animations/**/*.{json,jsonc,json5}")),
            schema("biome", match_bp("biomes/**/*.{json,jsonc,json5}")),
            schema("block", match_bp("blocks/**/*.{json,jsonc,json5}")),
            schema("cameraPreset", match_bp("cameras/presets/**/*.{json,jsonc,json5}")),
            schema("dialogue", match_bp("dialogue/**/*.{json,jsonc,json5}")),
            schema("dimension", match_bp("dimensions/**/*.{json,jsonc,json5}")),
            schema("entity", match_bp("entities/**/*.{json,jsonc,json5}")),
            schema("featureRule", match_bp("feature_rules/**/*.{json,jsonc,json5}")),
            schema("feature", match_bp("features/**/*.{json,jsonc,json5}")),
            schema("tick", match_bp("functions/tick.{json,jsonc,json5}")),
            schema("craftingItemCatalog", match_bp("item_catalog/crafting_item_catalog.{json,jsonc,json5}")),
            schema("item", match_bp("items/**/*.{json,jsonc,json5}")),
            schema("lootTable", match_bp("loot_tables/**/*.{json,jsonc,json5}")),
            schema("manifest", match_bp("manifest.{json,jsonc,json5}")),
            schema("recipe", match_bp("recipes/**/*.{json,jsonc,json5}")),
            schema("spawnRule", match_bp("spawn_rules/**/*.{json,jsonc,json5}")),
            schema("tradeTable", match_bp("trading/**/*.{json,jsonc,json5}")),
            schema("volume", match_bp("volumes/**/*.{json,jsonc,json5}")),
            schema("processorList", match_bp("worldgen/processors/**/*.{json,jsonc,json5}")),
            schema("structureSet", match_bp("worldgen/structure_sets/**/*.{json,jsonc,json5}")),
            schema("jigsawStructure", match_bp("worldgen/structures/**/*.{json,jsonc,json5}")),
            schema("templatePool", match_bp("worldgen/template_pools/**/*.{json,jsonc,json5}")),
            // Resource Pack schemas
            schema("clientAnimationController", match_rp("animation_controllers/**/*.{json,jsonc,json5}")),
            schema("clientAnimation", match_rp("animations/**/*.{json,jsonc,json5}")),
            schema("atmosphereSettings", match_rp("atmospherics/**/*.{json,jsonc,json5}")),
            schema("attachable", match_rp("attachables/**/*.{json,jsonc,json5}")),
            schema("biomesClient", match_rp("biomes_client.{json,jsonc,json5}")),
            schema("clientBiome", match_rp("biomes/**/*.{json,jsonc,json5}")),
            schema("blockCulling", match_rp("block_culling/**/*.{json,jsonc,json5}")),
            schema("clientBlock", match_rp("blocks.{json,jsonc,json5}")),
            schema("colorGradingSettings", match_rp("color_grading/**/*.{json,jsonc,json5}")),
            schema("clientEntity", match_rp("entity/**/*.{json,jsonc,json5}")),
            schema("fog", match_rp("fogs/**/*.{json,jsonc,json5}")),
            schema("clientItem", match_rp("items/**/*.{json,jsonc,json5}")),
            schema("lightingSettings", match_rp("lighting/**/*.{json,jsonc,json5}")),
            schema("manifest", match_rp("manifest.{json,jsonc,json5}")),
            schema("material", match_rp("materials/**/*.material")),
            schema("geometry", match_rp("models/**/*.{json,jsonc,json5}")),
            schema("particle", match_rp("particles/**/*.{json,jsonc,json5}")),
            schema("pbrFallbackSettings", match_rp("pbr/global.{json,jsonc,json5}")),
            schema("pointLightSettings", match_rp("point_lights/global.{json,jsonc,json5}")),
            schema("renderController", match_rp("render_controllers/**/*.{json,jsonc,json5}")),
            schema("shadowSettings", match_rp("shadows/global.{json,jsonc,json5}")),
            schema("clientSound", match_rp("sounds.{json,jsonc,json5}")),
            schema("musicDefinition", match_rp("sounds/music_definitions.{json,jsonc,json5}")),
            schema("soundDefinition", match_rp("sounds/sound_definitions.{json,jsonc,json5}")),
            schema("splashes", match_rp("splashes.{json,jsonc,json5}")),
            schema("textureSet", match_rp("textures/**/*.texture_set.{json,jsonc,json5}")),
            schema("flipbookTexture", match_rp("textures/flipbook_textures.{json,jsonc,json5}")),
            schema("itemTexture", match_rp("textures/item_texture.{json,jsonc,json5}")),
            schema("terrainTexture", match_rp("textures/terrain_texture.{json,jsonc,json5}")),
            schema("ui", match_rp("ui/**/*.{json,jsonc,json5}")),
            schema("waterSettings", match_rp("water/**/*.{json,jsonc,json5}")),
            // Skin Pack schemas
            schema("skins", vec!["skin_pack/skins.{json,jsonc,json5}".to_string()])
        ];

        let config = serde_json::json!({
            "json": { "schemas": schemas }
        });
        self.cached_json_configuration = Some(config.clone());
        Ok(Some(config))
    }

    fn fetch_schemas(&self) -> Result<()> {
        let release = zed::latest_github_release(
            "rockide/editor-packages",
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
