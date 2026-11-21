use std::io::{Cursor, Read};

use serde::Deserialize;

use crate::{Error, Metadata};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    duration: u32,
    server_name: Option<String>,
}

pub fn read(bytes: &[u8]) -> Result<(Vec<u8>, Metadata), Error> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut entry = |name| -> Result<Vec<u8>, Error> {
        let mut data = Vec::new();
        archive.by_name(name)?.read_to_end(&mut data)?;
        Ok(data)
    };
    let meta: Meta = serde_json::from_slice(&entry("metaData.json")?)?;
    let metadata = Metadata {
        duration_ms: meta.duration,
        server_name: meta.server_name,
        game: None,
        mode: None,
        map: None,
    };
    Ok((entry("recording.tmcpr")?, metadata))
}
