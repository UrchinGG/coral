use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    error::Error,
    fs,
    path::Path,
    time::Duration,
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha1::{Digest, Sha1};

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const VERSION: &str = "1.8.9";
const META_HOSTS: [&str; 2] = [
    "https://piston-meta.mojang.com/",
    "https://launchermeta.mojang.com/",
];
const RESOURCES: &str = "https://resources.download.minecraft.net";

#[derive(Deserialize)]
struct Manifest {
    versions: Vec<Version>,
}

#[derive(Deserialize)]
struct Version {
    id: String,
    url: String,
}

#[derive(Deserialize)]
struct VersionFile {
    #[serde(rename = "assetIndex")]
    asset_index: AssetIndexLink,
}

#[derive(Deserialize)]
struct AssetIndexLink {
    url: String,
}

#[derive(Deserialize)]
struct AssetIndex {
    objects: HashMap<String, Asset>,
}

#[derive(Deserialize)]
struct Asset {
    hash: String,
}

#[derive(Deserialize)]
struct Event {
    sounds: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Entry {
    File(String),
    Detailed(Detail),
}

#[derive(Deserialize)]
#[serde(default)]
struct Detail {
    name: String,
    volume: f32,
    pitch: f32,
    weight: u32,
    stream: bool,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Serialize)]
struct Variant {
    file: String,
    weight: u32,
    volume: f32,
    pitch: f32,
}

pub fn bake_sounds(out: &Path) -> Result<(), Box<dyn Error>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();
    let manifest: Manifest = json(&fetch(&agent, MANIFEST)?)?;
    let version = manifest
        .versions
        .into_iter()
        .find(|version| version.id == VERSION)
        .ok_or("the version manifest has no 1.8.9")?;
    let version: VersionFile = json(&fetch(&agent, trusted(&version.url)?)?)?;
    let index: AssetIndex = json(&fetch(&agent, trusted(&version.asset_index.url)?)?)?;
    let hash = |key: &str| {
        index
            .objects
            .get(key)
            .map(|asset| asset.hash.as_str())
            .ok_or_else(|| format!("the asset index has no {key}"))
    };

    let events: BTreeMap<String, Event> = json(&object(&agent, hash("minecraft/sounds.json")?)?)?;
    let table: BTreeMap<String, Vec<Variant>> = events
        .into_iter()
        .map(|(name, event)| {
            (
                name,
                event
                    .sounds
                    .into_iter()
                    .filter_map(Entry::variant)
                    .collect(),
            )
        })
        .filter(|(_, variants): &(String, Vec<Variant>)| !variants.is_empty())
        .collect();
    let files: BTreeSet<&str> = table
        .values()
        .flatten()
        .map(|variant| variant.file.as_str())
        .collect();

    let sounds = out.join("sounds");
    let mut total = 0;
    for file in &files {
        let hash = hash(&format!("minecraft/sounds/{file}.ogg"))?;
        let path = sounds.join(format!("{file}.ogg"));
        let bytes = match fs::read(&path) {
            Ok(bytes) if sha1(&bytes) == hash => bytes,
            _ => {
                let bytes = object(&agent, hash)?;
                fs::create_dir_all(path.parent().ok_or("sound without a folder")?)?;
                fs::write(&path, &bytes)?;
                bytes
            }
        };
        total += bytes.len();
    }
    fs::write(sounds.join("sounds.json"), serde_json::to_vec(&table)?)?;
    println!(
        "{} events, {} files, {:.1} MB",
        table.len(),
        files.len(),
        total as f64 / 1e6
    );
    Ok(())
}

impl Entry {
    fn variant(self) -> Option<Variant> {
        let detail = match self {
            Entry::File(name) => Detail {
                name,
                ..Detail::default()
            },
            Entry::Detailed(detail) => detail,
        };
        let playable = !detail.stream && detail.kind.as_deref().is_none_or(|kind| kind == "file");
        (playable && safe(&detail.name)).then_some(Variant {
            file: detail.name,
            weight: detail.weight,
            volume: detail.volume,
            pitch: detail.pitch,
        })
    }
}

impl Default for Detail {
    fn default() -> Detail {
        Detail {
            name: String::new(),
            volume: 1.0,
            pitch: 1.0,
            weight: 1,
            stream: false,
            kind: None,
        }
    }
}

fn safe(name: &str) -> bool {
    name.split('/').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    })
}

fn trusted(url: &str) -> Result<&str, String> {
    META_HOSTS
        .iter()
        .any(|host| url.starts_with(host))
        .then_some(url)
        .ok_or_else(|| format!("untrusted metadata location {url}"))
}

fn object(agent: &ureq::Agent, hash: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    if hash.len() != 40 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("malformed asset hash {hash}").into());
    }
    let bytes = fetch(agent, &format!("{RESOURCES}/{}/{hash}", &hash[..2]))?;
    if sha1(&bytes) != hash {
        return Err(format!("asset {hash} failed its checksum").into());
    }
    Ok(bytes)
}

fn fetch(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(agent.get(url).call()?.body_mut().read_to_vec()?)
}

fn json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, serde_json::Error> {
    serde_json::from_slice(bytes)
}

fn sha1(bytes: &[u8]) -> String {
    Sha1::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
