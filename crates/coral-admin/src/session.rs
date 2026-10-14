use axum::http::{HeaderMap, header};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::Sha256;
use subtle::ConstantTimeEq;

pub const SESSION_COOKIE: &str = "__Host-coral_admin_session";
pub const OAUTH_STATE_COOKIE: &str = "__Host-coral_admin_oauth";
pub const SESSION_TTL: Duration = Duration::hours(12);
pub const OAUTH_STATE_TTL: Duration = Duration::minutes(10);

const SESSION_KEY_INFO: &[u8] = b"coral-admin/session";
const OAUTH_STATE_KEY_INFO: &[u8] = b"coral-admin/oauth-state";
const MAC_KEY_LEN: usize = 32;
const NONCE_LEN: usize = 32;

type HmacSha256 = Hmac<Sha256>;

pub struct AuthKeys {
    pub session: MacKey,
    pub oauth_state: MacKey,
}

pub struct MacKey([u8; MAC_KEY_LEN]);

#[derive(Debug, PartialEq, Eq)]
pub struct Session {
    pub discord_id: i64,
    pub expires_at: i64,
    pub epoch: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct OAuthState {
    pub nonce: String,
    pub expires_at: i64,
    pub next: String,
}

impl AuthKeys {
    pub fn derive(secret: &[u8]) -> Self {
        let hkdf = Hkdf::<Sha256>::new(None, secret);
        Self {
            session: MacKey::expand(&hkdf, SESSION_KEY_INFO),
            oauth_state: MacKey::expand(&hkdf, OAUTH_STATE_KEY_INFO),
        }
    }
}

impl MacKey {
    fn expand(hkdf: &Hkdf<Sha256>, info: &[u8]) -> Self {
        let mut key = [0u8; MAC_KEY_LEN];
        hkdf.expand(info, &mut key)
            .expect("the MAC key length is a valid HKDF-SHA256 output length");
        Self(key)
    }

    pub fn seal(&self, payload: &str) -> String {
        let signature = self.mac(payload).finalize().into_bytes();
        format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(payload),
            hex::encode(signature)
        )
    }

    pub fn open(&self, token: &str) -> Option<String> {
        let (encoded_payload, signature_hex) = token.rsplit_once('.')?;
        let payload = String::from_utf8(URL_SAFE_NO_PAD.decode(encoded_payload).ok()?).ok()?;
        let signature = hex::decode(signature_hex).ok()?;
        self.mac(&payload).verify_slice(&signature).ok()?;
        Some(payload)
    }

    fn mac(&self, payload: &str) -> HmacSha256 {
        let mut mac = HmacSha256::new_from_slice(&self.0).expect("HMAC accepts any key size");
        mac.update(payload.as_bytes());
        mac
    }
}

impl Session {
    pub fn start(discord_id: i64, epoch: u64) -> Self {
        Self {
            discord_id,
            expires_at: (Utc::now() + SESSION_TTL).timestamp(),
            epoch,
        }
    }

    pub fn seal(&self, key: &MacKey) -> String {
        key.seal(&format!(
            "{}.{}.{}",
            self.discord_id, self.expires_at, self.epoch
        ))
    }

    pub fn open(token: &str, key: &MacKey) -> Option<Self> {
        let payload = key.open(token)?;
        let fields: Vec<&str> = payload.split('.').collect();
        let [discord_id, expires_at, epoch] = fields.as_slice() else {
            return None;
        };
        let session = Self {
            discord_id: discord_id.parse().ok()?,
            expires_at: expires_at.parse().ok()?,
            epoch: epoch.parse().ok()?,
        };
        is_unexpired(session.expires_at).then_some(session)
    }
}

impl OAuthState {
    pub fn start(next: String) -> Self {
        let mut nonce = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        Self {
            nonce: hex::encode(nonce),
            expires_at: (Utc::now() + OAUTH_STATE_TTL).timestamp(),
            next,
        }
    }

    pub fn seal(&self, key: &MacKey) -> String {
        key.seal(&format!("{}.{}.{}", self.nonce, self.expires_at, self.next))
    }

    pub fn open(token: &str, key: &MacKey) -> Option<Self> {
        let payload = key.open(token)?;
        let mut fields = payload.splitn(3, '.');
        let state = Self {
            nonce: fields.next()?.to_string(),
            expires_at: fields.next()?.parse().ok()?,
            next: fields.next()?.to_string(),
        };
        is_unexpired(state.expires_at).then_some(state)
    }

    pub fn matches(&self, returned_state: &str) -> bool {
        self.nonce
            .as_bytes()
            .ct_eq(returned_state.as_bytes())
            .into()
    }
}

pub fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(name)?.strip_prefix('='))
}

pub fn set_cookie(name: &str, value: &str, max_age: Duration) -> String {
    format!(
        "{name}={value}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        max_age.num_seconds()
    )
}

pub fn clear_cookie(name: &str) -> String {
    set_cookie(name, "", Duration::zero())
}

fn is_unexpired(expires_at: i64) -> bool {
    expires_at > Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    const SECRET: [u8; 32] = [7u8; 32];
    const OWNER: i64 = 12345;

    fn keys() -> AuthKeys {
        AuthKeys::derive(&SECRET)
    }

    #[test]
    fn sealed_session_opens_to_the_same_session() {
        let keys = keys();
        let session = Session::start(OWNER, 4);
        let opened = Session::open(&session.seal(&keys.session), &keys.session);
        assert_eq!(opened, Some(session));
    }

    #[test]
    fn tampered_session_payload_is_rejected() {
        let keys = keys();
        let token = Session::start(OWNER, 0).seal(&keys.session);
        let (_, signature) = token.rsplit_once('.').unwrap();
        let forged_payload = URL_SAFE_NO_PAD.encode("99999.9999999999.0");
        let forged = format!("{forged_payload}.{signature}");
        assert_eq!(Session::open(&forged, &keys.session), None);
    }

    #[test]
    fn session_sealed_with_another_secret_is_rejected() {
        let other = AuthKeys::derive(&[8u8; 32]);
        let token = Session::start(OWNER, 0).seal(&other.session);
        assert_eq!(Session::open(&token, &keys().session), None);
    }

    #[test]
    fn expired_session_is_rejected() {
        let keys = keys();
        let expired = Session {
            discord_id: OWNER,
            expires_at: Utc::now().timestamp() - 10,
            epoch: 0,
        };
        assert_eq!(
            Session::open(&expired.seal(&keys.session), &keys.session),
            None
        );
    }

    #[test]
    fn session_with_extra_fields_is_rejected() {
        let keys = keys();
        let expires_at = Utc::now().timestamp() + 60;
        let token = keys.session.seal(&format!("{OWNER}.{expires_at}.0.1"));
        assert_eq!(Session::open(&token, &keys.session), None);
    }

    #[test]
    fn session_without_an_epoch_is_rejected() {
        let keys = keys();
        let expires_at = Utc::now().timestamp() + 60;
        let token = keys.session.seal(&format!("{OWNER}.{expires_at}"));
        assert_eq!(Session::open(&token, &keys.session), None);
    }

    #[test]
    fn malformed_tokens_are_rejected() {
        let keys = keys();
        for token in ["", "not-a-valid-token", ".", "abc.zz", "%%%.00"] {
            assert_eq!(keys.session.open(token), None, "{token:?}");
        }
    }

    #[test]
    fn derived_keys_are_separate_from_each_other_and_the_secret() {
        let keys = keys();
        assert_ne!(keys.session.0, keys.oauth_state.0);
        assert_ne!(keys.session.0, SECRET);
        assert_ne!(keys.oauth_state.0, SECRET);
    }

    #[test]
    fn derivation_is_deterministic() {
        assert_eq!(keys().session.0, keys().session.0);
        assert_eq!(keys().oauth_state.0, keys().oauth_state.0);
    }

    #[test]
    fn payload_sealed_with_the_session_key_does_not_open_as_oauth_state() {
        let keys = keys();
        let state = OAuthState::start("/".into());
        let token = keys.session.seal(&format!(
            "{}.{}.{}",
            state.nonce, state.expires_at, state.next
        ));
        assert_eq!(OAuthState::open(&token, &keys.oauth_state), None);
    }

    #[test]
    fn oauth_state_round_trips_a_next_containing_dots() {
        let keys = keys();
        let state = OAuthState::start("/players/a.b?tab=x.y".into());
        let opened = OAuthState::open(&state.seal(&keys.oauth_state), &keys.oauth_state);
        assert_eq!(opened, Some(state));
    }

    #[test]
    fn expired_oauth_state_is_rejected() {
        let keys = keys();
        let expired = OAuthState {
            nonce: "ab".into(),
            expires_at: Utc::now().timestamp() - 1,
            next: "/".into(),
        };
        assert_eq!(
            OAuthState::open(&expired.seal(&keys.oauth_state), &keys.oauth_state),
            None
        );
    }

    #[test]
    fn oauth_state_matches_only_its_own_nonce() {
        let state = OAuthState::start("/".into());
        let nonce = state.nonce.clone();
        assert!(state.matches(&nonce));
        assert!(!state.matches(&nonce[1..]));
        assert!(!state.matches(""));
        assert!(!state.matches(&OAuthState::start("/".into()).nonce));
    }

    #[test]
    fn nonces_are_unique_and_full_length() {
        let first = OAuthState::start("/".into());
        let second = OAuthState::start("/".into());
        assert_ne!(first.nonce, second.nonce);
        assert_eq!(first.nonce.len(), NONCE_LEN * 2);
    }

    #[test]
    fn cookie_value_finds_the_exact_cookie_across_headers() {
        let mut headers = HeaderMap::new();
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("a=1; __Host-coral_admin_sessionx=2"),
        );
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("__Host-coral_admin_session=3"),
        );
        assert_eq!(cookie_value(&headers, SESSION_COOKIE), Some("3"));
        assert_eq!(cookie_value(&headers, OAUTH_STATE_COOKIE), None);
    }

    #[test]
    fn cookies_carry_host_prefix_attributes() {
        assert_eq!(
            set_cookie(SESSION_COOKIE, "v", SESSION_TTL),
            "__Host-coral_admin_session=v; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=43200"
        );
        assert_eq!(
            clear_cookie(OAUTH_STATE_COOKIE),
            "__Host-coral_admin_oauth=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0"
        );
    }
}
