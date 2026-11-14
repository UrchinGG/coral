use clients::{is_uuid, normalize_uuid};

use crate::error::ApiError;

const VERSION_DIGIT: usize = 12;
const PLAYER_VERSION: u8 = b'4';

pub const NOT_A_PLAYER: &str =
    "not a player UUID; NPCs and nicked players have version 1 or 2 UUIDs and no Hypixel profile";

pub fn is_player_uuid(raw: &str) -> bool {
    is_uuid(raw) && normalize_uuid(raw).as_bytes()[VERSION_DIGIT] == PLAYER_VERSION
}

pub fn player_uuid(raw: &str) -> Result<String, ApiError> {
    if is_player_uuid(raw) {
        Ok(normalize_uuid(raw))
    } else {
        Err(ApiError::BadRequest(format!("{NOT_A_PLAYER}: {raw}")))
    }
}

pub fn player_uuids(raw: &[String]) -> Result<Vec<String>, ApiError> {
    let invalid: Vec<&str> = raw
        .iter()
        .map(String::as_str)
        .filter(|uuid| !is_player_uuid(uuid))
        .collect();
    if !invalid.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "{NOT_A_PLAYER}: {}",
            invalid.join(", ")
        )));
    }
    Ok(raw.iter().map(|uuid| normalize_uuid(uuid)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYER: &str = "069a79f4-44e9-4726-a5be-fca90e38aaf5";
    const NPC: &str = "a4b3083b0ea32fbd9961cfd0590d6a88";
    const NICK: &str = "0c2b8a5e3f4d11ee8c90a2b3c4d5e6f7";

    #[test]
    fn only_version_four_uuids_are_players() {
        assert_eq!(
            player_uuid(PLAYER).unwrap(),
            "069a79f444e94726a5befca90e38aaf5"
        );
        assert!(player_uuid(NPC).is_err());
        assert!(player_uuid(NICK).is_err());
        assert!(player_uuid("Technoblade").is_err());
        assert!(player_uuid("069a79f444e94726a5be").is_err());
    }

    #[test]
    fn one_non_player_rejects_the_whole_batch_and_names_it() {
        let batch = [PLAYER.to_string(), NPC.to_string()];
        let Err(ApiError::BadRequest(message)) = player_uuids(&batch) else {
            panic!("a batch with an NPC was accepted");
        };
        assert!(message.ends_with(NPC), "{message}");
        assert_eq!(player_uuids(&batch[..1]).unwrap().len(), 1);
    }
}
