use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub const SESSION_COOKIE_NAME: &str = "coral_admin_session";
pub const SESSION_TTL: Duration = Duration::hours(12);

type HmacSha256 = Hmac<Sha256>;

pub fn issue(discord_id: i64, secret: &[u8; 32]) -> String {
    let expires_at = (Utc::now() + SESSION_TTL).timestamp();
    let payload = format!("{discord_id}.{expires_at}");
    let signature = sign(&payload, secret);
    format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(&payload),
        hex::encode(signature)
    )
}

pub fn verify(token: &str, secret: &[u8; 32]) -> Option<i64> {
    let (encoded_payload, signature_hex) = token.rsplit_once('.')?;
    let payload = String::from_utf8(URL_SAFE_NO_PAD.decode(encoded_payload).ok()?).ok()?;
    let signature = hex::decode(signature_hex).ok()?;

    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key size");
    mac.update(payload.as_bytes());
    mac.verify_slice(&signature).ok()?;

    let (discord_id_str, expires_at_str) = payload.split_once('.')?;
    let discord_id: i64 = discord_id_str.parse().ok()?;
    let expires_at: i64 = expires_at_str.parse().ok()?;
    (expires_at > Utc::now().timestamp()).then_some(discord_id)
}

fn sign(payload: &str, secret: &[u8; 32]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key size");
    mac.update(payload.as_bytes());
    mac.finalize().into_bytes().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_token_verifies_to_same_discord_id() {
        let secret = [7u8; 32];
        let token = issue(12345, &secret);
        assert_eq!(verify(&token, &secret), Some(12345));
    }

    #[test]
    fn tampered_payload_fails_verification() {
        let secret = [7u8; 32];
        let token = issue(12345, &secret);
        let (_, sig) = token.rsplit_once('.').unwrap();
        let forged_payload = URL_SAFE_NO_PAD.encode("99999.9999999999");
        let forged = format!("{forged_payload}.{sig}");
        assert_eq!(verify(&forged, &secret), None);
    }

    #[test]
    fn wrong_secret_fails_verification() {
        let secret = [7u8; 32];
        let other = [8u8; 32];
        let token = issue(12345, &secret);
        assert_eq!(verify(&token, &other), None);
    }

    #[test]
    fn expired_token_fails_verification() {
        let secret = [7u8; 32];
        let expired_at = Utc::now().timestamp() - 10;
        let payload = format!("12345.{expired_at}");
        let signature = sign(&payload, &secret);
        let token = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(&payload),
            hex::encode(signature)
        );
        assert_eq!(verify(&token, &secret), None);
    }

    #[test]
    fn malformed_token_fails_verification() {
        let secret = [7u8; 32];
        assert_eq!(verify("not-a-valid-token", &secret), None);
        assert_eq!(verify("", &secret), None);
    }
}
