use reqlog::backfill::BackfillError;

use crate::error::AdminError;

mod discord_names;
mod index_catchup;
mod subjects;

pub use discord_names::{DISCORD_NAMES_JOB, DiscordNamesJob, missing_discord_names};
pub use index_catchup::{INDEX_CATCHUP_JOB, IndexCatchupJob, missing_indexes};
pub use subjects::{BackfillPlan, SUBJECTS_JOB, SubjectsJob};

impl From<BackfillError> for AdminError {
    fn from(error: BackfillError) -> Self {
        match error {
            BackfillError::Database(error) => error.into(),
            BackfillError::Redis(error) => error.into(),
        }
    }
}
