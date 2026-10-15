use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::coverage::{Coverage, CoverageCache};
use crate::error::AdminError;
use crate::extensions::ExtensionSummary;
use crate::{serde_id, serde_ts};

pub const BUILD_ENV: &str = "CORAL_BUILD";

const HEAD_CLOSE: &str = "</head>";
const BOOT_OPEN: &str = "<script id=\"boot\" type=\"application/json\">";
const BOOT_CLOSE: &str = "</script>";
const ESCAPED_LESS_THAN: &str = "\\u003c";

#[derive(Clone)]
pub struct BootSource {
    build: Option<String>,
    coverage: CoverageCache,
    extensions: Vec<ExtensionSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Boot {
    #[serde(serialize_with = "serde_id::discord_id")]
    pub discord_id: i64,
    pub build: Option<String>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub now: DateTime<Utc>,
    pub extensions: Vec<ExtensionSummary>,
    pub coverage: Coverage,
}

impl BootSource {
    pub fn new(
        build: Option<String>,
        coverage: CoverageCache,
        extensions: Vec<ExtensionSummary>,
    ) -> Self {
        Self {
            build,
            coverage,
            extensions,
        }
    }

    pub fn from_env(coverage: CoverageCache, extensions: Vec<ExtensionSummary>) -> Self {
        let build = std::env::var(BUILD_ENV)
            .ok()
            .filter(|build| !build.is_empty());
        Self::new(build, coverage, extensions)
    }

    pub fn boot(&self, discord_id: i64) -> Boot {
        Boot {
            discord_id,
            build: self.build.clone(),
            now: Utc::now(),
            extensions: self.extensions.clone(),
            coverage: self.coverage.current(),
        }
    }
}

pub fn inline_into(index: &str, boot: &Boot) -> Result<String, AdminError> {
    let json = serde_json::to_string(boot)
        .map_err(|error| AdminError::Internal(format!("encode the boot payload: {error}")))?;
    let payload = json.replace('<', ESCAPED_LESS_THAN);
    if !index.contains(HEAD_CLOSE) {
        return Err(AdminError::Internal(
            "the embedded index.html has no </head> to carry the boot payload".into(),
        ));
    }
    Ok(index.replacen(
        HEAD_CLOSE,
        &format!("{BOOT_OPEN}{payload}{BOOT_CLOSE}{HEAD_CLOSE}"),
        1,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = "<html><head><title>Coral Admin</title></head><body></body></html>";

    fn boot() -> Boot {
        let extensions = vec![ExtensionSummary {
            id: "upstream",
            title: "Upstream",
        }];
        let source = BootSource::new(
            Some("abc123".into()),
            CoverageCache::fixed(Coverage::default()),
            extensions,
        );
        source.boot(1_547_000_000_000_000_000)
    }

    fn payload(html: &str) -> serde_json::Value {
        let start = html.find(BOOT_OPEN).unwrap() + BOOT_OPEN.len();
        let end = start + html[start..].find(BOOT_CLOSE).unwrap();
        serde_json::from_str(&html[start..end]).unwrap()
    }

    #[test]
    fn boot_is_inlined_as_a_json_data_block_in_the_head() {
        let html = inline_into(INDEX, &boot()).unwrap();
        assert!(html.contains("</script></head>"));
        let value = payload(&html);
        assert_eq!(value["discord_id"], "1547000000000000000");
        assert_eq!(value["build"], "abc123");
        assert_eq!(
            value["extensions"],
            serde_json::json!([{ "id": "upstream", "title": "Upstream" }])
        );
        assert!(value["coverage"]["gaps"].is_array());
    }

    #[test]
    fn markup_in_the_payload_cannot_close_the_script() {
        let mut boot = boot();
        boot.build = Some("</script><script>alert(1)</script>".into());
        let html = inline_into(INDEX, &boot).unwrap();
        assert_eq!(html.matches("</script>").count(), 1);
        assert_eq!(
            payload(&html)["build"],
            "</script><script>alert(1)</script>"
        );
    }

    #[test]
    fn an_index_without_a_head_is_an_error() {
        let error = inline_into("<html><body></body></html>", &boot()).unwrap_err();
        assert_eq!(error.code(), "internal");
    }
}
