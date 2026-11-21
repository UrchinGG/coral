use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Textures {
    pub skin_url: Option<String>,
    pub slim: bool,
    pub profile_name: Option<String>,
    pub profile_id: Option<u128>,
}

pub fn decode_textures(base64: &str) -> Option<Textures> {
    let json: Value = serde_json::from_slice(&decode_base64(base64)?).ok()?;
    let text = |pointer| json.pointer(pointer).and_then(Value::as_str);
    Some(Textures {
        skin_url: text("/textures/SKIN/url").map(str::to_owned),
        slim: text("/textures/SKIN/metadata/model") == Some("slim"),
        profile_name: text("/profileName").map(str::to_owned),
        profile_id: text("/profileId").and_then(|id| u128::from_str_radix(id, 16).ok()),
    })
}

fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let (mut buffer, mut bits) = (0u32, 0);
    for symbol in text.trim_end_matches('=').bytes() {
        let value = match symbol {
            b'A'..=b'Z' => symbol - b'A',
            b'a'..=b'z' => symbol - b'a' + 26,
            b'0'..=b'9' => symbol - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        buffer = buffer << 6 | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((buffer >> bits) as u8);
        }
    }
    Some(bytes)
}
