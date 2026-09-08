use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::colors;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Manifest {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub appid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workshopid: Option<u64>,
    pub content: String,
    pub preview: String,
    pub title: String,
    pub description: String,
    pub visibility: u32,
    pub tags: Vec<String>,

    #[serde(skip)]
    source_path: Option<PathBuf>,
}

impl Manifest {
    /// List of default manifest filenames to check for when loading without a specified path.
    const MANIFEST_FILENAMES: [&'static str; 4] = [
        "mod-manifest.json",
        "mod-manifest.toml",
        "mod-manifest.yaml",
        "mod-manifest.yml",
    ];

    /// Initialize a new manifest with default values.
    /// And save it
    pub fn init(extension: &str) -> Self {
        let manifest = Self {
            schema: Some("https://raw.githubusercontent.com/SimKDT/Steam-Uploader-rs/refs/heads/main/manifest_schema/mod-manifest-schema.json".to_string()),
            appid: 0,
            workshopid: None,
            content: "./Contents".to_string(),
            preview: "preview.png".to_string(),
            title: "Mod Template".to_string(),
            description: "./description.bbcode".to_string(),
            visibility: 2, // default to private
            tags: Vec::new(),
            source_path: None,
        };
        let filename = format!("mod-manifest.{}", extension);

        // verify the file doesn't already exist to prevent overwriting
        if Path::new(&filename).exists() {
            colors::error(&format!("File {} already exists. Aborting to prevent overwriting.", filename));
            std::process::exit(1);
        }

        // try to save the manifest
        if let Err(e) = manifest.save(&filename) {
            colors::error(&format!("Failed to save manifest: {}", e));
        } else {
            colors::success(&format!("Initialized new manifest at {}", filename));
        }
        manifest
    }

    /// Load manifest from file, auto-detecting format (JSON, TOML, or YAML).
    /// If no format can be detected, an error is returned.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)?;
        let path_str = path.to_string_lossy();

        let mut manifest: Self = if path_str.ends_with(".json") {
            colors::success("Loaded JSON manifest successfully.");
            serde_json::from_str(&content)?
        } else if path_str.ends_with(".toml") {
            colors::success("Loaded TOML manifest successfully.");
            toml::from_str(&content)?
        } else if path_str.ends_with(".yaml") || path_str.ends_with(".yml") {
            colors::success("Loaded YAML manifest successfully.");
            serde_yaml::from_str(&content)?
        } else {
            // throw an error
            return Err(format!("Could not detect manifest format from file extension for file: {}. Supported extensions are .json, .toml, .yaml, .yml", path_str).into());
        };

        manifest.source_path = Some(path.to_path_buf());
        Ok(manifest)
    }

    /// Load manifest from default locations: `manifest.json`, `manifest.toml`, `manifest.yaml`, or `manifest.yml`.
    pub fn load_default(manifest_path: Option<String>) -> Result<Self, Box<dyn std::error::Error>> {
        // verify that the provided manifest path is a valid manifest format
        if let Some(path) = manifest_path {
            let filename = Path::new(&path)
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("Invalid manifest path")?;
            
            if !Self::MANIFEST_FILENAMES.contains(&filename) {
                return Err(format!(
                    "Invalid manifest filename: {}. Must be one of: {}",
                    filename,
                    Self::MANIFEST_FILENAMES.join(", ")
                ).into());
            }
            
            return Self::load(path);
        }

        // no manifest path provided, check the default location
        for filename in &Self::MANIFEST_FILENAMES {
            if Path::new(filename).exists() {
                return Self::load(filename);
            }
        }
        Err("No manifest file found (manifest.json, manifest.toml, manifest.yaml, or manifest.yml)".into())
    }

    /// Save manifest to file, preserving or detecting format.
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let path = path.as_ref();
        let path_str = path.to_string_lossy();

        let content = if path_str.ends_with(".toml") {
            toml::to_string_pretty(self)?
        } else if path_str.ends_with(".yaml") || path_str.ends_with(".yml") {
            serde_yaml::to_string(self)?
        } else {
            // default to JSON
            serde_json::to_string_pretty(self)?
        };

        fs::write(path, content)?;
        Ok(())
    }

    /// Save manifest back to the original file it was loaded from.
    pub fn save_to_source(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(path) = &self.source_path {
            self.save(path)
        } else {
            Err("Manifest has no source path, cannot save".into())
        }
    }

    /// Save manifest with workshop ID updated to the original file it was loaded from.
    pub fn save_with_id_to_source(
        &mut self,
        workshopid: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.workshopid = Some(workshopid);
        self.save_to_source()
    }



    /// Get the absolute path to the content directory, resolved relative to the manifest file.
    pub fn get_content_path(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        self.resolve_path(&self.content)
    }

    /// Get the absolute path to the preview file, resolved relative to the manifest file.
    pub fn get_preview_path(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        self.resolve_path(&self.preview)
    }

    /// Resolve a path relative to the manifest file's directory.
    fn resolve_path(&self, relative_path: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        if let Some(manifest_dir) = &self.source_path.as_ref().and_then(|p| p.parent()) {
            let resolved = manifest_dir.join(relative_path);
            Ok(resolved)
        } else {
            // if no source path, try the path as-is
            Ok(PathBuf::from(relative_path))
        }
    }

    pub fn get_description(&self) -> Result<String, Box<dyn std::error::Error>> {
        // test if the manifest description is a path to a file
        // if so, load that file content as the description
        // otherwise, return the description as is
        self.try_read_file(self.description.clone())
    }

    pub fn try_read_file(&self, txt: String) -> Result<String, Box<dyn std::error::Error>> {
        let description_path = self.resolve_path(&txt)?;
        if description_path.exists() && description_path.is_file() {
            match fs::read_to_string(&description_path) {
                Ok(content) => Ok(content),
                Err(_) => Ok(txt.clone()), // if there's an error reading the file, return the original description
            }
        } else {
            Ok(txt.clone())
        }
    }
}
