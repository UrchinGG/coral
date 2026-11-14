use uuid::Uuid;

use crate::error::AdminError;

mod accounts;
mod batches;
mod blocks;
#[cfg(test)]
mod fixtures;
mod forms;
mod lookups;
mod params;
mod profile;
mod requester;
mod requesters;
mod resolved;
mod snapshots;
mod stream;
mod tagged;
mod tags;

pub use blocks::{block, unblock};
pub use lookups::PlayerLookups;
pub use params::{RequestersParams, StreamParams};
pub use profile::{PlayerCore, read_core};
pub use requesters::{Requesters, read_requesters};
pub use snapshots::{
    SnapshotAt, SnapshotAtQuery, SnapshotPage, SnapshotQuery, read_snapshot_at, read_snapshots,
};
pub use stream::{PlayerRequestRow, read_stream};
pub use tagged::{RecentlyTagged, read_recently_tagged};
pub use tags::{NewLock, NewTag, PlayerTags, add_tag, lock, read_tags, remove_tag, unlock};

pub fn parse_player(raw: &str) -> Result<Uuid, AdminError> {
    reqlog::parse_uuid(raw.trim())
        .ok_or_else(|| AdminError::BadRequest(format!("{raw:?} is not a player UUID")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OJUGG: &str = "94589c87999b4a7ab3add2ae50711e64";

    #[test]
    fn players_are_addressed_by_either_uuid_form_in_any_case() {
        let ojugg = Uuid::parse_str(OJUGG).unwrap();
        for raw in [
            OJUGG,
            "94589c87-999b-4a7a-b3ad-d2ae50711e64",
            "94589C87999B4A7AB3ADD2AE50711E64",
        ] {
            assert_eq!(parse_player(raw).unwrap(), ojugg, "{raw}");
        }
        for raw in ["oJugg", "", "94589c87"] {
            assert_eq!(
                parse_player(raw).unwrap_err().code(),
                "bad_request",
                "{raw}"
            );
        }
    }
}
