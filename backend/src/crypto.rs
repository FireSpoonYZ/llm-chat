use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use rand::RngCore;

/// Encrypt a plaintext string using AES-256-GCM.
///
/// `key_hex` must be a 64-character hex string representing 32 bytes.
/// Returns a base64-encoded string containing the 12-byte nonce
/// prepended to the ciphertext.
pub fn encrypt(plaintext: &str, key_hex: &str) -> Result<String, String> {
    let key_bytes = hex::decode(key_hex)
        .map_err(|e| format!("invalid hex key: {e}"))?;
    if key_bytes.len() != 32 {
        return Err(format!(
            "encryption key must be 32 bytes, got {}",
            key_bytes.len()
        ));
    }

    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| format!("failed to create cipher: {e}"))?;

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("encryption failed: {e}"))?;

    // Prepend the nonce to the ciphertext, then base64-encode.
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(BASE64.encode(&combined))
}

/// Decrypt a base64-encoded ciphertext that was produced by [`encrypt`].
///
/// The first 12 bytes of the decoded payload are the nonce; the rest is
/// the AES-256-GCM ciphertext (including the authentication tag).
pub fn decrypt(encrypted: &str, key_hex: &str) -> Result<String, String> {
    let key_bytes = hex::decode(key_hex)
        .map_err(|e| format!("invalid hex key: {e}"))?;
    if key_bytes.len() != 32 {
        return Err(format!(
            "encryption key must be 32 bytes, got {}",
            key_bytes.len()
        ));
    }

    let combined = BASE64
        .decode(encrypted)
        .map_err(|e| format!("invalid base64: {e}"))?;
    if combined.len() < 12 {
        return Err("ciphertext too short to contain nonce".into());
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| format!("failed to create cipher: {e}"))?;

    let plaintext_bytes = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("decryption failed: {e}"))?;

    String::from_utf8(plaintext_bytes)
        .map_err(|e| format!("decrypted data is not valid UTF-8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        // 32 bytes = 64 hex chars
        let key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let plaintext = "sk-ant-api03-secret-key-value";
        let encrypted = encrypt(plaintext, key).unwrap();
        let decrypted = decrypt(&encrypted, key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_key_fails() {
        let key1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let key2 = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let encrypted = encrypt("secret", key1).unwrap();
        assert!(decrypt(&encrypted, key2).is_err());
    }

    #[test]
    fn invalid_key_length() {
        assert!(encrypt("hello", "aabb").is_err());
    }
}
