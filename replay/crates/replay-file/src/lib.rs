mod mcpr;
mod urp;

pub struct Recording {
    stream: Vec<u8>,
    metadata: Metadata,
}

pub struct Packet<'a> {
    pub time: u32,
    pub id: i32,
    pub body: &'a [u8],
}

pub struct Metadata {
    pub duration_ms: u32,
    pub server_name: Option<String>,
    pub game: Option<String>,
    pub mode: Option<String>,
    pub map: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a .mcpr or URP1 recording")]
    UnknownFormat,
    #[error("unreadable .mcpr: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("unreadable recording: {0}")]
    Io(#[from] std::io::Error),
    #[error("unreadable recording metadata: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported URP1 archive version")]
    UnsupportedArchive,
    #[error("corrupt URP1 archive")]
    Corrupt,
    #[error("URP1 archive keeps its map in the server world store; export it with inline sections")]
    NeedsWorldStore,
}

impl Recording {
    pub fn open(bytes: &[u8]) -> Result<Recording, Error> {
        let (stream, metadata) = match bytes {
            [b'P', b'K', 3, 4, ..] => mcpr::read(bytes)?,
            [b'U', b'R', b'P', b'1', ..] => urp::read(bytes)?,
            _ => return Err(Error::UnknownFormat),
        };
        Ok(Recording { stream, metadata })
    }

    pub fn packets(&self) -> impl Iterator<Item = Packet<'_>> {
        let mut rest = self.stream.as_slice();
        std::iter::from_fn(move || {
            loop {
                let (time, tail) = rest.split_first_chunk()?;
                let (length, tail) = tail.split_first_chunk()?;
                let length = u32::from_be_bytes(*length) as usize;
                let (data, tail) = tail.split_at_checked(length)?;
                rest = tail;
                let mut body = replay_protocol::Reader::new(data);
                if let Some(id) = body.varint() {
                    return Some(Packet {
                        time: u32::from_be_bytes(*time),
                        id,
                        body: body.rest(),
                    });
                }
            }
        })
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }
}
