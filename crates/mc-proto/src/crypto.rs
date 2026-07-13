use aes::Aes128;
use aes::cipher::{BlockEncrypt, KeyInit};
use rsa::pkcs8::{DecodePublicKey, EncodePublicKey};
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey};
use sha1::{Digest, Sha1};

pub use rsa::RsaPublicKey;

use crate::error::{ProtocolError, Result};

const RSA_BITS: usize = 1024;

pub struct ServerKey {
    private_key: RsaPrivateKey,
    pub der_public_key: Vec<u8>,
}

impl ServerKey {
    pub fn generate() -> Self {
        let private_key =
            RsaPrivateKey::new(&mut rand::thread_rng(), RSA_BITS).expect("RSA keypair generation");
        let der_public_key = private_key
            .to_public_key()
            .to_public_key_der()
            .expect("public key encoding")
            .to_vec();
        Self {
            private_key,
            der_public_key,
        }
    }

    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        self.private_key
            .decrypt(Pkcs1v15Encrypt, data)
            .map_err(|e| ProtocolError::Encryption(e.to_string()))
    }
}

pub fn parse_public_key(der: &[u8]) -> Result<RsaPublicKey> {
    RsaPublicKey::from_public_key_der(der).map_err(|e| ProtocolError::Encryption(e.to_string()))
}

pub fn encrypt_with_public_key(public_key: &RsaPublicKey, data: &[u8]) -> Result<Vec<u8>> {
    public_key
        .encrypt(&mut rand::thread_rng(), Pkcs1v15Encrypt, data)
        .map_err(|e| ProtocolError::Encryption(e.to_string()))
}

pub fn server_hash(server_id: &[u8], shared_secret: &[u8], public_key_der: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(server_id);
    hasher.update(shared_secret);
    hasher.update(public_key_der);
    let hash: [u8; 20] = hasher.finalize().into();

    if (hash[0] & 0x80) != 0 {
        let mut carry = true;
        let mut negated = [0u8; 20];
        for i in (0..20).rev() {
            let (val, c) = (!hash[i]).overflowing_add(u8::from(carry));
            negated[i] = val;
            carry = c;
        }
        let hex: String = negated.iter().map(|b| format!("{b:02x}")).collect();
        format!("-{}", hex.trim_start_matches('0'))
    } else {
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        hex.trim_start_matches('0').to_string()
    }
}

pub struct Encryptor {
    cipher: Aes128,
    iv: [u8; 16],
}

impl Encryptor {
    pub fn new(shared_secret: &[u8; 16]) -> Self {
        Self {
            cipher: Aes128::new(shared_secret.into()),
            iv: *shared_secret,
        }
    }

    pub fn encrypt(&mut self, data: &mut [u8]) {
        for byte in data.iter_mut() {
            let mut block = self.iv;
            self.cipher.encrypt_block((&mut block).into());
            *byte ^= block[0];
            self.iv.copy_within(1.., 0);
            self.iv[15] = *byte;
        }
    }
}

pub struct Decryptor {
    cipher: Aes128,
    iv: [u8; 16],
}

impl Decryptor {
    pub fn new(shared_secret: &[u8; 16]) -> Self {
        Self {
            cipher: Aes128::new(shared_secret.into()),
            iv: *shared_secret,
        }
    }

    pub fn decrypt(&mut self, data: &mut [u8]) {
        for byte in data.iter_mut() {
            let mut block = self.iv;
            self.cipher.encrypt_block((&mut block).into());
            let cipher_byte = *byte;
            *byte ^= block[0];
            self.iv.copy_within(1.., 0);
            self.iv[15] = cipher_byte;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfb8_roundtrip() {
        let secret = [0x42u8; 16];
        let mut encryptor = Encryptor::new(&secret);
        let mut decryptor = Decryptor::new(&secret);

        let original = b"Hello, Minecraft!".to_vec();
        let mut data = original.clone();
        encryptor.encrypt(&mut data);
        assert_ne!(data, original);
        decryptor.decrypt(&mut data);
        assert_eq!(data, original);
    }

    #[test]
    fn cfb8_streaming_matches_whole() {
        let secret = [0x42u8; 16];
        let mut streaming = Encryptor::new(&secret);
        let mut whole = Encryptor::new(&secret);

        let mut part1 = b"Hello, ".to_vec();
        let mut part2 = b"Minecraft!".to_vec();
        streaming.encrypt(&mut part1);
        streaming.encrypt(&mut part2);

        let mut combined = b"Hello, Minecraft!".to_vec();
        whole.encrypt(&mut combined);

        let mut joined = part1;
        joined.extend_from_slice(&part2);
        assert_eq!(joined, combined);
    }

    #[test]
    fn rsa_roundtrip() {
        let key = ServerKey::generate();
        let public = parse_public_key(&key.der_public_key).unwrap();
        let secret = b"sixteen byte key";
        let encrypted = encrypt_with_public_key(&public, secret).unwrap();
        assert_eq!(key.decrypt(&encrypted).unwrap(), secret);
    }
}
