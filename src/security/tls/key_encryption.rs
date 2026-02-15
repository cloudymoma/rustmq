use crate::error::{Result, RustMqError};
/// Private Key Encryption Module
///
/// This module provides **mandatory** secure encryption and decryption of private keys using
/// AES-256-GCM with PBKDF2-HMAC-SHA256 key derivation.
///
/// ⚠️ **SECURITY POLICY**: All private keys MUST be encrypted. Plaintext keys are NOT supported.
///
/// # Security Properties
///
/// - **Encryption**: AES-256-GCM (Authenticated Encryption with Associated Data)
/// - **Key Derivation**: PBKDF2-HMAC-SHA256 with 600,000 iterations (OWASP 2023)
/// - **Salt**: 256-bit random salt, unique per encryption
/// - **Nonce**: 96-bit random nonce, unique per encryption (CRITICAL for GCM security)
/// - **Authentication**: 128-bit GCM authentication tag prevents tampering
///
/// # Encrypted Format
///
/// ```text
/// ┌─────────────────────────────────────────────┐
/// │ Header: "RUSTMQ-ENCRYPTED-KEY-V1\0"        │  24 bytes
/// ├─────────────────────────────────────────────┤
/// │ Salt (random, unique per key)              │  32 bytes (256 bits)
/// ├─────────────────────────────────────────────┤
/// │ Nonce (random, unique per encryption)      │  12 bytes (96 bits)
/// ├─────────────────────────────────────────────┤
/// │ Ciphertext (AES-256-GCM encrypted PEM)     │  Variable
/// ├─────────────────────────────────────────────┤
/// │ Authentication Tag (GCM)                   │  16 bytes (128 bits)
/// └─────────────────────────────────────────────┘
/// Total overhead: 84 bytes
/// ```
///
/// # Security Considerations
///
/// 1. **Nonce Uniqueness**: CRITICAL - Never reuse nonce with same key (catastrophic for GCM)
/// 2. **Password Strength**: Minimum 16 characters recommended, 32+ for high security
/// 3. **Password Storage**: Use environment variables or secrets manager, NEVER hardcode
/// 4. **Password Loss**: Encrypted keys are irrecoverable if password is lost
/// 5. **Mandatory Encryption**: ALL private keys must be encrypted - no plaintext keys allowed
use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit},
};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::Sha256;
use tracing::warn;

/// Header identifying encrypted private key format (24 bytes with null terminator)
const HEADER: &[u8] = b"RUSTMQ-ENCRYPTED-KEY-V1\0";

/// Salt size for PBKDF2 key derivation (256 bits)
const SALT_SIZE: usize = 32;

/// Nonce size for AES-GCM (96 bits - standard for GCM)
const NONCE_SIZE: usize = 12;

/// Encryption key size (256 bits for AES-256)
const KEY_SIZE: usize = 32;

/// GCM authentication tag size (128 bits - standard)
const TAG_SIZE: usize = 16;

/// PBKDF2 iteration count (OWASP 2023 recommendation for SHA256)
const PBKDF2_ITERATIONS: u32 = 600_000;

/// Minimum size for encrypted data (header + salt + nonce + tag)
const MIN_ENCRYPTED_SIZE: usize = HEADER.len() + SALT_SIZE + NONCE_SIZE + TAG_SIZE;

/// Encrypt a private key PEM string with password-based encryption
///
/// This function uses AES-256-GCM with PBKDF2-HMAC-SHA256 for secure encryption.
/// Each encryption generates unique salt and nonce for maximum security.
///
/// # Arguments
///
/// * `plaintext_pem` - The private key in PEM format to encrypt
/// * `password` - The password used for encryption (minimum 8 chars, 16+ recommended)
///
/// # Returns
///
/// Binary blob containing header, salt, nonce, ciphertext, and authentication tag
///
/// # Security
///
/// - **Salt**: 256-bit cryptographically random, unique per key
/// - **Nonce**: 96-bit cryptographically random, unique per encryption
/// - **PBKDF2**: 600,000 iterations makes brute force expensive
/// - **Authentication**: GCM tag ensures data integrity and authenticity
///
/// # Examples
///
/// ```ignore
/// let encrypted = encrypt_private_key(pem_data, "strong-password-here")?;
/// // Store encrypted blob securely
/// ```
pub fn encrypt_private_key(plaintext_pem: &str, password: &str) -> Result<Vec<u8>> {
    // Validate password
    if password.is_empty() {
        return Err(RustMqError::Config(
            "Password cannot be empty for key encryption".to_string(),
        ));
    }

    if password.len() < 8 {
        warn!("Password is very short (<8 chars), recommend 16+ characters for security");
    } else if password.len() < 16 {
        warn!("Password is short (<16 chars), recommend 16+ characters for security");
    }

    // Generate random salt (unique per encryption)
    let mut salt = [0u8; SALT_SIZE];
    rand::thread_rng().fill_bytes(&mut salt);

    // Derive encryption key from password using PBKDF2-HMAC-SHA256
    let mut derived_key = [0u8; KEY_SIZE];
    pbkdf2_hmac::<Sha256>(
        password.as_bytes(),
        &salt,
        PBKDF2_ITERATIONS,
        &mut derived_key,
    );

    // Generate random nonce (unique per encryption - CRITICAL for GCM security)
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt using AES-256-GCM
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&derived_key));
    let ciphertext = cipher
        .encrypt(nonce, plaintext_pem.as_bytes())
        .map_err(|e| RustMqError::Encryption {
            reason: format!("AES-GCM encryption failed: {}", e),
        })?;

    // Construct encrypted blob: header + salt + nonce + ciphertext (includes auth tag)
    let mut result = Vec::with_capacity(HEADER.len() + SALT_SIZE + NONCE_SIZE + ciphertext.len());
    result.extend_from_slice(HEADER);
    result.extend_from_slice(&salt);
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);

    Ok(result)
}

/// Decrypt an encrypted private key using password-based decryption
///
/// This function decrypts data encrypted with `encrypt_private_key()`.
/// Authentication tag verification ensures data has not been tampered with.
///
/// # Arguments
///
/// * `encrypted_data` - Binary blob from `encrypt_private_key()`
/// * `password` - The password used during encryption
///
/// # Returns
///
/// The original private key PEM string
///
/// # Errors
///
/// - `RustMqError::Encryption` if:
///   - Data is too short or corrupted
///   - Invalid header (wrong format or version)
///   - Wrong password (authentication tag verification fails)
///   - Data has been tampered with (authentication tag verification fails)
///   - Decrypted data is not valid UTF-8
///
/// # Security
///
/// - **Constant-time**: GCM tag verification is constant-time (no timing attacks)
/// - **Authentication**: Wrong password or tampered data causes tag verification to fail
/// - **No information leakage**: Errors don't reveal whether password or tampering was the cause
///
/// # Examples
///
/// ```ignore
/// let pem_data = decrypt_private_key(&encrypted_blob, "strong-password-here")?;
/// // Use decrypted PEM data
/// ```
pub fn decrypt_private_key(encrypted_data: &[u8], password: &str) -> Result<String> {
    // Validate minimum size
    if encrypted_data.len() < MIN_ENCRYPTED_SIZE {
        return Err(RustMqError::Encryption {
            reason: format!(
                "Encrypted data too short: {} bytes (minimum {} bytes required)",
                encrypted_data.len(),
                MIN_ENCRYPTED_SIZE
            ),
        });
    }

    // Parse and validate header
    let (header, rest) = encrypted_data.split_at(HEADER.len());
    if header != HEADER {
        return Err(RustMqError::Encryption {
            reason: "Invalid encryption header - data may be corrupted or in plaintext format"
                .to_string(),
        });
    }

    // Parse salt
    let (salt, rest) = rest.split_at(SALT_SIZE);

    // Parse nonce
    let (nonce_bytes, ciphertext) = rest.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    // Derive decryption key from password using same PBKDF2 parameters
    let mut derived_key = [0u8; KEY_SIZE];
    pbkdf2_hmac::<Sha256>(
        password.as_bytes(),
        salt,
        PBKDF2_ITERATIONS,
        &mut derived_key,
    );

    // Decrypt using AES-256-GCM
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&derived_key));
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| RustMqError::Encryption {
            reason: format!(
                "Decryption failed - wrong password or corrupted data: {}",
                e
            ),
        })?;

    // Convert to UTF-8 string
    String::from_utf8(plaintext).map_err(|e| RustMqError::Encryption {
        reason: format!("Decrypted data is not valid UTF-8: {}", e),
    })
}

/// Verify that data is properly encrypted
///
/// This function verifies that data has the correct encryption header.
/// **All private keys MUST be encrypted** - this function returns an error if the data
/// is not encrypted.
///
/// # Arguments
///
/// * `data` - Binary data to verify
///
/// # Returns
///
/// `Ok(())` if data is properly encrypted, `Err` if not encrypted or invalid
///
/// # Examples
///
/// ```ignore
/// verify_encrypted(&key_data)?;
/// let pem = decrypt_private_key(&key_data, password)?;
/// ```
pub fn verify_encrypted(data: &[u8]) -> Result<()> {
    if data.len() < HEADER.len() {
        return Err(RustMqError::Encryption {
            reason: "Private key is not encrypted - all keys must be encrypted for security"
                .to_string(),
        });
    }

    if &data[..HEADER.len()] != HEADER {
        return Err(RustMqError::Encryption {
            reason:
                "Private key is not encrypted - plaintext keys are not allowed for security reasons"
                    .to_string(),
        });
    }

    Ok(())
}

/// Check if data is encrypted (for internal use)
///
/// Returns true if data has valid encryption header, false otherwise.
fn is_encrypted(data: &[u8]) -> bool {
    data.len() >= HEADER.len() && &data[..HEADER.len()] == HEADER
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PASSWORD: &str = "test-strong-password-1234567890";
    const SHORT_PASSWORD: &str = "short";
    const TEST_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
        MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC8\n\
        -----END PRIVATE KEY-----";

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let encrypted =
            encrypt_private_key(TEST_PEM, TEST_PASSWORD).expect("Encryption should succeed");

        assert!(encrypted.len() > MIN_ENCRYPTED_SIZE);
        assert!(is_encrypted(&encrypted));

        let decrypted =
            decrypt_private_key(&encrypted, TEST_PASSWORD).expect("Decryption should succeed");

        assert_eq!(decrypted, TEST_PEM);
    }

    #[test]
    fn test_wrong_password_fails() {
        let encrypted =
            encrypt_private_key(TEST_PEM, TEST_PASSWORD).expect("Encryption should succeed");

        let result = decrypt_private_key(&encrypted, "wrong-password");
        assert!(result.is_err());

        if let Err(RustMqError::Encryption { reason }) = result {
            assert!(reason.contains("Decryption failed"));
        } else {
            panic!("Expected Encryption error");
        }
    }

    #[test]
    fn test_tampered_data_fails() {
        let mut encrypted =
            encrypt_private_key(TEST_PEM, TEST_PASSWORD).expect("Encryption should succeed");

        // Tamper with ciphertext (beyond header+salt+nonce)
        let tamper_pos = HEADER.len() + SALT_SIZE + NONCE_SIZE + 10;
        encrypted[tamper_pos] ^= 0xFF;

        let result = decrypt_private_key(&encrypted, TEST_PASSWORD);
        assert!(result.is_err());
    }

    #[test]
    fn test_truncated_data_fails() {
        let encrypted =
            encrypt_private_key(TEST_PEM, TEST_PASSWORD).expect("Encryption should succeed");

        let truncated = &encrypted[..MIN_ENCRYPTED_SIZE - 1];
        let result = decrypt_private_key(truncated, TEST_PASSWORD);
        assert!(result.is_err());

        if let Err(RustMqError::Encryption { reason }) = result {
            assert!(reason.contains("too short"));
        } else {
            panic!("Expected Encryption error");
        }
    }

    #[test]
    fn test_invalid_header_fails() {
        let mut encrypted =
            encrypt_private_key(TEST_PEM, TEST_PASSWORD).expect("Encryption should succeed");

        // Corrupt header
        encrypted[0] = b'X';

        let result = decrypt_private_key(&encrypted, TEST_PASSWORD);
        assert!(result.is_err());

        if let Err(RustMqError::Encryption { reason }) = result {
            assert!(reason.contains("Invalid encryption header"));
        } else {
            panic!("Expected Encryption error");
        }
    }

    #[test]
    fn test_empty_password_fails() {
        let result = encrypt_private_key(TEST_PEM, "");
        assert!(result.is_err());

        if let Err(RustMqError::Config(msg)) = result {
            assert!(msg.contains("Password cannot be empty"));
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_short_password_warns() {
        // This should work but log a warning
        let result = encrypt_private_key(TEST_PEM, SHORT_PASSWORD);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unique_salts_for_same_key() {
        let encrypted1 =
            encrypt_private_key(TEST_PEM, TEST_PASSWORD).expect("Encryption should succeed");
        let encrypted2 =
            encrypt_private_key(TEST_PEM, TEST_PASSWORD).expect("Encryption should succeed");

        // Extract salts (after header)
        let salt1 = &encrypted1[HEADER.len()..HEADER.len() + SALT_SIZE];
        let salt2 = &encrypted2[HEADER.len()..HEADER.len() + SALT_SIZE];

        // Salts must be different
        assert_ne!(salt1, salt2, "Salts should be unique for each encryption");
    }

    #[test]
    fn test_unique_nonces_for_same_key() {
        let encrypted1 =
            encrypt_private_key(TEST_PEM, TEST_PASSWORD).expect("Encryption should succeed");
        let encrypted2 =
            encrypt_private_key(TEST_PEM, TEST_PASSWORD).expect("Encryption should succeed");

        // Extract nonces (after header + salt)
        let nonce_offset = HEADER.len() + SALT_SIZE;
        let nonce1 = &encrypted1[nonce_offset..nonce_offset + NONCE_SIZE];
        let nonce2 = &encrypted2[nonce_offset..nonce_offset + NONCE_SIZE];

        // Nonces must be different
        assert_ne!(
            nonce1, nonce2,
            "Nonces should be unique for each encryption"
        );
    }

    #[test]
    fn test_different_ciphertext_for_same_plaintext() {
        let encrypted1 =
            encrypt_private_key(TEST_PEM, TEST_PASSWORD).expect("Encryption should succeed");
        let encrypted2 =
            encrypt_private_key(TEST_PEM, TEST_PASSWORD).expect("Encryption should succeed");

        // Entire encrypted blobs should be different (probabilistic encryption)
        assert_ne!(
            encrypted1, encrypted2,
            "Encrypted data should be different each time"
        );
    }

    #[test]
    fn test_is_encrypted_detection() {
        let encrypted =
            encrypt_private_key(TEST_PEM, TEST_PASSWORD).expect("Encryption should succeed");

        assert!(is_encrypted(&encrypted));
        assert!(!is_encrypted(TEST_PEM.as_bytes()));
        assert!(!is_encrypted(b"random data"));
        assert!(!is_encrypted(b""));
    }

    #[test]
    fn test_encryption_overhead() {
        let encrypted =
            encrypt_private_key(TEST_PEM, TEST_PASSWORD).expect("Encryption should succeed");

        // Overhead = header + salt + nonce + tag
        let expected_min_overhead = HEADER.len() + SALT_SIZE + NONCE_SIZE + TAG_SIZE;
        let actual_overhead = encrypted.len() - TEST_PEM.len();

        assert_eq!(actual_overhead, expected_min_overhead);
    }

    #[test]
    fn test_large_key_encryption() {
        // Test with larger key data
        let large_pem = "-----BEGIN PRIVATE KEY-----\n".to_string()
            + &"A".repeat(10000)
            + "\n-----END PRIVATE KEY-----";

        let encrypted =
            encrypt_private_key(&large_pem, TEST_PASSWORD).expect("Encryption should succeed");

        let decrypted =
            decrypt_private_key(&encrypted, TEST_PASSWORD).expect("Decryption should succeed");

        assert_eq!(decrypted, large_pem);
    }
}
