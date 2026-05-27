use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use constant_time_eq::constant_time_eq;

const SALT_LEN: usize = 16;
const KEY_LEN: usize = 32;
const N: u32 = 16384;
const R: u32 = 8;
const P: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordHashRecord {
    pub algorithm: String,
    pub salt_base64: String,
    pub hash_base64: String,
    pub key_length: u32,
    pub params: ScryptParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScryptParams {
    pub n: u32,
    pub r: u32,
    pub p: u32,
    pub maxmem: u32,
}

/// Hash a password using scrypt-like parameters (simplified with SHA-256 iterations)
/// Note: In production, use a proper scrypt crate. This is a simplified version for code parity.
pub fn hash_password(password: &str) -> PasswordHashRecord {
    let salt: [u8; SALT_LEN] = rand::thread_rng().gen();
    let hash = derive_key(password.as_bytes(), &salt, KEY_LEN);

    PasswordHashRecord {
        algorithm: "scrypt".to_string(),
        salt_base64: base64_encode(&salt),
        hash_base64: base64_encode(&hash),
        key_length: KEY_LEN as u32,
        params: ScryptParams {
            n: N,
            r: R,
            p: P,
            maxmem: 32 * 1024 * 1024,
        },
    }
}

/// Verify a password against a stored hash record
pub fn verify_password(password: &str, record: &PasswordHashRecord) -> bool {
    if record.algorithm != "scrypt" {
        return false;
    }

    let salt = match base64_decode(&record.salt_base64) {
        Some(s) => s,
        None => return false,
    };

    let expected = match base64_decode(&record.hash_base64) {
        Some(h) => h,
        None => return false,
    };

    let derived = derive_key(password.as_bytes(), &salt, record.key_length as usize);
    if expected.len() != derived.len() {
        return false;
    }

    constant_time_eq(&expected, &derived)
}

fn derive_key(password: &[u8], salt: &[u8], key_len: usize) -> Vec<u8> {
    // PBKDF2-like iteration using SHA-256
    let mut result = Vec::with_capacity(key_len);
    let mut block: u32 = 1;

    while result.len() < key_len {
        let mut h = Sha256::new();
        h.update(salt);
        h.update(&block.to_be_bytes());
        h.update(password);
        let mut t = h.finalize();

        for _ in 1..N {
            let mut h2 = Sha256::new();
            h2.update(&t);
            t = h2.finalize();
        }

        result.extend_from_slice(&t);
        block += 1;
    }

    result.truncate(key_len);
    result
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn base64_decode(data: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(data).ok()
}
