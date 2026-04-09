use aes::Aes256;
use cbc::cipher::{BlockEncryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;


pub fn sign_unlock_key(
    discord_id: i64,
    hwid: &str,
    issued_at: u64,
    expires_at: u64,
    hmac_key: &[u8; 32],
) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(hmac_key).expect("HMAC accepts any key size");
    mac.update(&discord_id.to_le_bytes());
    mac.update(hwid.as_bytes());
    mac.update(&issued_at.to_le_bytes());
    mac.update(&expires_at.to_le_bytes());
    mac.finalize().into_bytes().into()
}


pub fn generate_session_token() -> String {
    let bytes: [u8; 32] = rand::random();
    hex::encode(bytes)
}


pub fn generate_refresh_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 48];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}


pub fn hash_refresh_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    hex::encode(digest)
}


pub fn derive_core_data_key(session_token: &str, hwid: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(session_token.as_bytes());
    hasher.update(hwid.as_bytes());
    hasher.update(b"STARFISH_CORE_V2");
    hasher.finalize().into()
}


pub fn encrypt_core_data(tables_bytes: &[u8], session_token: &str, hwid: &str) -> Vec<u8> {
    let key = derive_core_data_key(session_token, hwid);
    let mut mac = HmacSha256::new_from_slice(&key).expect("HMAC accepts any key size");
    mac.update(tables_bytes);
    let hmac: [u8; 32] = mac.finalize().into_bytes().into();

    let mut plaintext = Vec::with_capacity(32 + tables_bytes.len());
    plaintext.extend_from_slice(&hmac);
    plaintext.extend_from_slice(tables_bytes);
    encrypt_aes256_cbc(&plaintext, &key)
}



pub fn ed25519_sign(payload: &[u8], signing_key: &ed25519_dalek::SigningKey) -> [u8; 64] {
    use ed25519_dalek::Signer;
    signing_key.sign(payload).to_bytes()
}


pub fn build_attestation_payload(
    session_token: &str,
    discord_id: i64,
    hwid: &str,
    issued_at: u64,
    expires_at: u64,
    core_data_hash: &[u8; 32],
) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(session_token.as_bytes());
    payload.extend_from_slice(&discord_id.to_le_bytes());
    payload.extend_from_slice(hwid.as_bytes());
    payload.extend_from_slice(&issued_at.to_le_bytes());
    payload.extend_from_slice(&expires_at.to_le_bytes());
    payload.extend_from_slice(core_data_hash);
    payload
}


pub fn hash_for_attestation(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}


pub fn base64_encode(data: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data)
}


pub fn decrypt_aes256_cbc(ciphertext: &[u8], key: &[u8; 32]) -> Option<Vec<u8>> {
    use cbc::cipher::BlockDecryptMut;
    type Aes256CbcDec = cbc::Decryptor<Aes256>;

    if ciphertext.len() < 32 || ciphertext.len() % 16 != 0 { return None; }

    let (iv, encrypted) = ciphertext.split_at(16);
    let mut buf = encrypted.to_vec();

    let mut cipher = Aes256CbcDec::new(key.into(), iv.into());
    for chunk in buf.chunks_exact_mut(16) {
        cipher.decrypt_block_mut(aes::Block::from_mut_slice(chunk));
    }

    let padding_len = *buf.last()? as usize;
    if padding_len == 0 || padding_len > 16 { return None; }
    if buf.len() < padding_len { return None; }
    buf.truncate(buf.len() - padding_len);
    Some(buf)
}


fn encrypt_aes256_cbc(data: &[u8], key: &[u8; 32]) -> Vec<u8> {
    type Aes256CbcEnc = cbc::Encryptor<Aes256>;

    let iv: [u8; 16] = rand::random();
    let padding_len = 16 - (data.len() % 16);
    let mut padded = data.to_vec();
    padded.extend(std::iter::repeat(padding_len as u8).take(padding_len));

    let mut cipher = Aes256CbcEnc::new(key.into(), &iv.into());
    for chunk in padded.chunks_exact_mut(16) {
        cipher.encrypt_block_mut(aes::Block::from_mut_slice(chunk));
    }

    let mut result = iv.to_vec();
    result.extend_from_slice(&padded);
    result
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 32];
        let plaintext = b"hello starfish";
        let ciphertext = encrypt_aes256_cbc(plaintext, &key);
        let decrypted = decrypt_aes256_cbc(&ciphertext, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_decrypt_exact_block_size() {
        let key = [0x01u8; 32];
        let plaintext = [0xABu8; 16];
        let ciphertext = encrypt_aes256_cbc(&plaintext, &key);
        let decrypted = decrypt_aes256_cbc(&ciphertext, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_decrypt_empty_plaintext() {
        let key = [0xFFu8; 32];
        let ciphertext = encrypt_aes256_cbc(b"", &key);
        let decrypted = decrypt_aes256_cbc(&ciphertext, &key).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn encrypt_decrypt_large_payload() {
        let key = [0x77u8; 32];
        let plaintext = vec![0xCDu8; 1024];
        let ciphertext = encrypt_aes256_cbc(&plaintext, &key);
        let decrypted = decrypt_aes256_cbc(&ciphertext, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let key = [0x42u8; 32];
        let wrong_key = [0x43u8; 32];
        let ciphertext = encrypt_aes256_cbc(b"secret", &key);
        match decrypt_aes256_cbc(&ciphertext, &wrong_key) {
            None => {}
            Some(decrypted) => assert_ne!(decrypted, b"secret"),
        }
    }

    #[test]
    fn decrypt_truncated_ciphertext_fails() {
        assert!(decrypt_aes256_cbc(&[0u8; 15], &[0u8; 32]).is_none());
        assert!(decrypt_aes256_cbc(&[], &[0u8; 32]).is_none());
    }

    #[test]
    fn each_encryption_produces_different_ciphertext() {
        let key = [0x42u8; 32];
        let plaintext = b"same input";
        let c1 = encrypt_aes256_cbc(plaintext, &key);
        let c2 = encrypt_aes256_cbc(plaintext, &key);
        assert_ne!(c1, c2, "random IV should produce different ciphertexts");
        assert_eq!(decrypt_aes256_cbc(&c1, &key).unwrap(), plaintext);
        assert_eq!(decrypt_aes256_cbc(&c2, &key).unwrap(), plaintext);
    }

    #[test]
    fn sign_is_deterministic() {
        let key = [0x99u8; 32];
        let s1 = sign_unlock_key(12345, "myhwid", 1000, 2000, &key);
        let s2 = sign_unlock_key(12345, "myhwid", 1000, 2000, &key);
        assert_eq!(s1, s2);
    }

    #[test]
    fn sign_differs_for_different_inputs() {
        let key = [0x99u8; 32];
        let s1 = sign_unlock_key(12345, "myhwid", 1000, 2000, &key);
        let s2 = sign_unlock_key(99999, "myhwid", 1000, 2000, &key);
        let s3 = sign_unlock_key(12345, "other", 1000, 2000, &key);
        assert_ne!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn session_tokens_are_unique() {
        let t1 = generate_session_token();
        let t2 = generate_session_token();
        assert_ne!(t1, t2);
        assert_eq!(t1.len(), 64);
    }

    #[test]
    fn refresh_tokens_are_unique() {
        let t1 = generate_refresh_token();
        let t2 = generate_refresh_token();
        assert_ne!(t1, t2);
        assert_eq!(t1.len(), 96);
    }

    #[test]
    fn refresh_token_hash_is_deterministic() {
        let token = "my_refresh_token_value";
        let h1 = hash_refresh_token(token);
        let h2 = hash_refresh_token(token);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn refresh_token_hash_differs_for_different_tokens() {
        assert_ne!(hash_refresh_token("token_a"), hash_refresh_token("token_b"));
    }

    #[test]
    fn base64_encode_roundtrip() {
        use base64::Engine;
        let data = vec![0x01, 0x02, 0x03, 0xFF];
        let encoded = base64_encode(&data);
        let decoded = base64::engine::general_purpose::STANDARD.decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn core_data_roundtrip() {
        let tables_bytes = b"fake bincode serialized core tables data here";
        let session_token = "abc123session";
        let hwid = "ff".repeat(32);

        let encrypted = encrypt_core_data(tables_bytes, session_token, &hwid);
        let key = derive_core_data_key(session_token, &hwid);
        let plaintext = decrypt_aes256_cbc(&encrypted, &key).unwrap();
        assert!(plaintext.len() >= 32);

        let (hmac_bytes, data) = plaintext.split_at(32);
        let mut mac = HmacSha256::new_from_slice(&key).unwrap();
        mac.update(data);
        assert!(mac.verify_slice(hmac_bytes).is_ok());
        assert_eq!(data, tables_bytes);
    }

    #[test]
    fn core_data_wrong_session_fails() {
        let tables_bytes = b"secret tables";
        let encrypted = encrypt_core_data(tables_bytes, "session_a", "hwid_a");
        let wrong_key = derive_core_data_key("session_b", "hwid_a");
        let result = decrypt_aes256_cbc(&encrypted, &wrong_key);
        match result {
            None => {}
            Some(plaintext) => {
                if plaintext.len() >= 32 {
                    let (hmac_bytes, data) = plaintext.split_at(32);
                    let mut mac = HmacSha256::new_from_slice(&wrong_key).unwrap();
                    mac.update(data);
                    assert!(mac.verify_slice(hmac_bytes).is_err());
                }
            }
        }
    }

    #[test]
    fn core_data_wrong_hwid_fails() {
        let tables_bytes = b"secret tables";
        let encrypted = encrypt_core_data(tables_bytes, "session", "hwid_a");
        let wrong_key = derive_core_data_key("session", "hwid_b");
        let result = decrypt_aes256_cbc(&encrypted, &wrong_key);
        match result {
            None => {}
            Some(plaintext) => {
                if plaintext.len() >= 32 {
                    let (hmac_bytes, data) = plaintext.split_at(32);
                    let mut mac = HmacSha256::new_from_slice(&wrong_key).unwrap();
                    mac.update(data);
                    assert!(mac.verify_slice(hmac_bytes).is_err());
                }
            }
        }
    }

    #[test]
    fn core_data_per_session_unique() {
        let tables = b"same tables";
        let c1 = encrypt_core_data(tables, "session_1", "hwid");
        let c2 = encrypt_core_data(tables, "session_2", "hwid");
        assert_ne!(c1, c2);
    }
}
