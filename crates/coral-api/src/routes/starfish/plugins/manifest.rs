use std::io::{Cursor, Read};
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::ApiError;


pub const TAG_ALLOWLIST: &[&str] = &[
    "utility", "visual", "combat", "bedwars", "skywars",
    "duels", "hypixel", "anti-ghost", "chat", "hud", "dev",
];

pub const MAX_TAGS_PER_PLUGIN: usize = 10;
pub const MAX_ZIP_SIZE: u64 = 10 * 1024 * 1024;
pub const MAX_FILE_UNCOMPRESSED: u64 = 5 * 1024 * 1024;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    #[serde(alias = "displayName")]
    pub display_name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    #[serde(default = "default_license")]
    pub license: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default, alias = "minStarfishVersion")]
    pub min_starfish_version: Option<String>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    Simple(String),
    Detailed {
        name: String,
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        optional: bool,
    },
}


fn default_license() -> String { "MIT".into() }


pub struct ExtractedPlugin {
    pub manifest: PluginManifest,
    pub manifest_json: serde_json::Value,
    pub readme: Option<String>,
}


pub fn extract_and_validate(zip_bytes: &[u8], expected_version: &str) -> Result<ExtractedPlugin, ApiError> {
    if zip_bytes.len() as u64 > MAX_ZIP_SIZE {
        return Err(ApiError::BadRequest(format!(
            "plugin.zip exceeds {} byte limit", MAX_ZIP_SIZE
        )));
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))
        .map_err(|e| ApiError::BadRequest(format!("invalid zip: {e}")))?;

    let manifest_bytes = read_file(&mut archive, "manifest.json")
        .ok_or_else(|| ApiError::BadRequest("manifest.json not found in plugin.zip".into()))?;

    let manifest_json: serde_json::Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| ApiError::BadRequest(format!("manifest.json is not valid JSON: {e}")))?;

    let manifest: PluginManifest = serde_json::from_value(manifest_json.clone())
        .map_err(|e| ApiError::BadRequest(format!("manifest.json failed schema: {e}")))?;

    validate_manifest(&manifest, expected_version)?;
    validate_entry_file(&mut archive, &manifest.name)?;

    let readme = read_file(&mut archive, "README.md")
        .and_then(|b| String::from_utf8(b).ok());

    Ok(ExtractedPlugin { manifest, manifest_json, readme })
}


pub fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}


fn slug_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-z][a-z0-9-]{2,63}$").unwrap())
}


fn version_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+$").unwrap())
}


fn validate_manifest(m: &PluginManifest, expected_version: &str) -> Result<(), ApiError> {
    if !slug_regex().is_match(&m.name) {
        return Err(ApiError::BadRequest(
            "manifest.name must match ^[a-z][a-z0-9-]{2,63}$".into(),
        ));
    }
    if !version_regex().is_match(&m.version) {
        return Err(ApiError::BadRequest(
            "manifest.version must be strict semver (major.minor.patch)".into(),
        ));
    }
    if m.version != expected_version {
        return Err(ApiError::BadRequest(format!(
            "manifest.version ({}) does not match requested version ({expected_version})",
            m.version,
        )));
    }
    if m.display_name.trim().is_empty() || m.display_name.len() > 64 {
        return Err(ApiError::BadRequest("display_name must be 1-64 chars".into()));
    }
    if m.description.trim().is_empty() || m.description.len() > 280 {
        return Err(ApiError::BadRequest("description must be 1-280 chars".into()));
    }
    if m.author.trim().is_empty() {
        return Err(ApiError::BadRequest("author is required".into()));
    }
    if m.tags.len() > MAX_TAGS_PER_PLUGIN {
        return Err(ApiError::BadRequest(format!(
            "at most {MAX_TAGS_PER_PLUGIN} tags allowed"
        )));
    }
    for tag in &m.tags {
        if !TAG_ALLOWLIST.contains(&tag.as_str()) {
            return Err(ApiError::BadRequest(format!(
                "tag '{tag}' is not in the allowlist"
            )));
        }
    }
    Ok(())
}


fn validate_entry_file(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, slug: &str) -> Result<(), ApiError> {
    let candidates = [format!("{slug}.lua"), "init.lua".into()];
    for candidate in &candidates {
        if archive.by_name(candidate).is_ok() {
            return Ok(());
        }
    }
    Err(ApiError::BadRequest(format!(
        "no entry file found (expected {slug}.lua or init.lua)"
    )))
}


fn read_file(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    let mut file = archive.by_name(name).ok()?;
    if file.size() > MAX_FILE_UNCOMPRESSED { return None; }
    let mut buf = Vec::new();
    file.take(MAX_FILE_UNCOMPRESSED).read_to_end(&mut buf).ok()?;
    Some(buf)
}
