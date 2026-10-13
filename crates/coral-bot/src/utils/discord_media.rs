use serde_json::Value;
use serenity::all::*;
use serenity::http::{LightMethod, Request, Route};

const MEDIA_GALLERY: u64 = 12;
const THUMBNAIL: u64 = 11;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageMedia {
    pub attachment_id: AttachmentId,
    pub filename: String,
    pub url: String,
    pub in_gallery: bool,
}

#[derive(Debug)]
pub enum MediaReadError {
    Http(serenity::Error),
    MissingAttachmentId(String),
}

impl std::fmt::Display for MediaReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(e) => write!(f, "could not fetch message: {e}"),
            Self::MissingAttachmentId(url) => write!(f, "media item has no attachment id: {url}"),
        }
    }
}

impl std::error::Error for MediaReadError {}

pub async fn message_media(
    http: &Http,
    channel_id: GenericChannelId,
    message_id: MessageId,
) -> Result<Vec<MessageMedia>, MediaReadError> {
    let raw: Value = http
        .fire(Request::new(
            Route::ChannelMessage {
                channel_id,
                message_id,
            },
            LightMethod::Get,
        ))
        .await
        .map_err(MediaReadError::Http)?;
    media_from_components(raw.get("components").unwrap_or(&Value::Null))
}

pub fn media_from_components(components: &Value) -> Result<Vec<MessageMedia>, MediaReadError> {
    let mut found = Vec::new();
    for component in components.as_array().into_iter().flatten() {
        collect_media(component, &mut found)?;
    }
    Ok(found)
}

fn collect_media(component: &Value, found: &mut Vec<MessageMedia>) -> Result<(), MediaReadError> {
    match component.get("type").and_then(Value::as_u64) {
        Some(MEDIA_GALLERY) => {
            for item in component
                .get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                found.push(read_media(item.get("media").unwrap_or(&Value::Null), true)?);
            }
        }
        Some(THUMBNAIL) => {
            found.push(read_media(
                component.get("media").unwrap_or(&Value::Null),
                false,
            )?);
        }
        _ => {}
    }
    for child in component
        .get("components")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        collect_media(child, found)?;
    }
    if let Some(accessory) = component.get("accessory") {
        collect_media(accessory, found)?;
    }
    Ok(())
}

fn read_media(media: &Value, in_gallery: bool) -> Result<MessageMedia, MediaReadError> {
    let url = media
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let attachment_id = media
        .get("attachment_id")
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse::<u64>().ok())
        .ok_or_else(|| MediaReadError::MissingAttachmentId(url.clone()))?;
    Ok(MessageMedia {
        attachment_id: AttachmentId::new(attachment_id),
        filename: filename_from_url(&url),
        url,
        in_gallery,
    })
}

fn filename_from_url(url: &str) -> String {
    url.split('?')
        .next()
        .unwrap_or_default()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_gallery_items_and_thumbnails_in_order() {
        let components = json!([{
            "type": 17,
            "components": [
                {"type": 9, "components": [{"type": 10, "content": "x"}],
                 "accessory": {"type": 11, "media": {
                     "url": "https://cdn.discordapp.com/attachments/1/7/face_a.png?ex=1",
                     "attachment_id": "7"}}},
                {"type": 12, "items": [
                    {"media": {"url": "https://cdn.discordapp.com/attachments/1/8/Hexze_1.mp4?ex=2",
                               "attachment_id": "8"}},
                    {"media": {"url": "https://cdn.discordapp.com/attachments/1/9/Hexze_2.png",
                               "attachment_id": "9"}}
                ]}
            ]
        }]);
        let media = media_from_components(&components).unwrap();
        let ids: Vec<u64> = media.iter().map(|m| m.attachment_id.get()).collect();
        assert_eq!(ids, vec![7, 8, 9]);
        assert_eq!(media[1].filename, "Hexze_1.mp4");
        assert!(!media[0].in_gallery && media[1].in_gallery);
    }

    #[test]
    fn an_item_without_an_attachment_id_is_an_error() {
        let components = json!([{"type": 12, "items": [{"media": {"url": "https://x/y.png"}}]}]);
        assert!(matches!(
            media_from_components(&components),
            Err(MediaReadError::MissingAttachmentId(_))
        ));
    }
}
