use std::sync::Arc;
use std::time::Duration;

use clients::{Extension, ExtensionFlag, FlagLevel};
use futures_util::future::join_all;

use super::{Flag, FlagKind};

pub const EXTENSION_FLAGS_TIMEOUT: Duration = Duration::from_secs(2);

pub async fn flags(extensions: &[Arc<dyn Extension>]) -> Vec<Flag> {
    join_all(
        extensions
            .iter()
            .map(|extension| flags_of(extension.as_ref())),
    )
    .await
    .into_iter()
    .flatten()
    .collect()
}

async fn flags_of(extension: &dyn Extension) -> Vec<Flag> {
    let reported = tokio::time::timeout(EXTENSION_FLAGS_TIMEOUT, extension.flags()).await;
    let failure = match reported {
        Ok(Ok(flags)) => {
            return flags
                .into_iter()
                .map(|flag| extension_flag(extension, flag))
                .collect();
        }
        Ok(Err(error)) => error.to_string(),
        Err(_) => format!("no answer within {} s", EXTENSION_FLAGS_TIMEOUT.as_secs()),
    };
    vec![unavailable_flag(extension, &failure)]
}

fn extension_flag(extension: &dyn Extension, flag: ExtensionFlag) -> Flag {
    Flag {
        extension: Some(extension.id()),
        ..Flag::new(
            FlagKind::Extension,
            flag.level,
            format!("extension:{}:{}", extension.id(), flag.key),
            flag.summary,
        )
    }
}

fn unavailable_flag(extension: &dyn Extension, failure: &str) -> Flag {
    Flag {
        extension: Some(extension.id()),
        ..Flag::new(
            FlagKind::Extension,
            FlagLevel::Warning,
            format!("extension:{}:unavailable", extension.id()),
            format!(
                "{} could not report its flags: {failure}",
                extension.title()
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use clients::view::ExtensionView;
    use clients::{ActionOutcome, ExtensionError};
    use serde_json::Value;

    use super::*;

    struct Fixed(Result<Vec<ExtensionFlag>, String>);

    #[async_trait]
    impl Extension for Fixed {
        fn id(&self) -> &'static str {
            "sample"
        }

        fn title(&self) -> &'static str {
            "Sample"
        }

        async fn view(&self) -> Result<ExtensionView, ExtensionError> {
            Err(ExtensionError::Failed("unused".into()))
        }

        async fn act(&self, action: &str, _input: Value) -> Result<ActionOutcome, ExtensionError> {
            Err(ExtensionError::UnknownAction(action.into()))
        }

        async fn flags(&self) -> Result<Vec<ExtensionFlag>, ExtensionError> {
            self.0.clone().map_err(ExtensionError::Failed)
        }
    }

    #[tokio::test]
    async fn extension_flags_are_namespaced_by_their_extension() {
        let extension: Arc<dyn Extension> = Arc::new(Fixed(Ok(vec![ExtensionFlag {
            key: "hold".into(),
            level: FlagLevel::Info,
            summary: "One entry is on hold".into(),
        }])));
        let flags = flags(&[extension]).await;
        assert_eq!(flags[0].key, "extension:sample:hold");
        assert_eq!(flags[0].extension, Some("sample"));
        assert_eq!(flags[0].level, FlagLevel::Info);
    }

    #[tokio::test]
    async fn a_failing_extension_becomes_a_warning_that_names_it() {
        let extension: Arc<dyn Extension> = Arc::new(Fixed(Err("store offline".into())));
        let flags = flags(&[extension]).await;
        assert_eq!(flags[0].key, "extension:sample:unavailable");
        assert_eq!(flags[0].level, FlagLevel::Warning);
        assert_eq!(
            flags[0].summary,
            "Sample could not report its flags: store offline"
        );
    }
}
