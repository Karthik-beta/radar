use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use domain::me::GithubUserId;

use super::keys::TokenKey;
use crate::db::auth::EncryptedToken;

const NONCE_LEN: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenBoxError {
    Random,
    Seal,
    Open,
}

/// Seals the GitHub token under AES-256-GCM with a fresh nonce. The user id is
/// bound in as additional data, so a ciphertext copied to another account's
/// row does not open.
pub fn seal(
    key: &TokenKey,
    user: GithubUserId,
    token: &str,
) -> Result<EncryptedToken, TokenBoxError> {
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(|_| TokenBoxError::Random)?;
    let aad = additional_data(user);
    let cipher = cipher(key)
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: token.as_bytes(),
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| TokenBoxError::Seal)?;
    Ok(EncryptedToken {
        cipher,
        nonce: nonce.to_vec(),
    })
}

pub fn open(
    key: &TokenKey,
    user: GithubUserId,
    sealed: &EncryptedToken,
) -> Result<String, TokenBoxError> {
    let nonce: [u8; NONCE_LEN] = sealed
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| TokenBoxError::Open)?;
    let aad = additional_data(user);
    let plain = cipher(key)
        .decrypt(
            &Nonce::from(nonce),
            Payload {
                msg: &sealed.cipher,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| TokenBoxError::Open)?;
    String::from_utf8(plain).map_err(|_| TokenBoxError::Open)
}

fn cipher(key: &TokenKey) -> Aes256Gcm {
    Aes256Gcm::new(&Key::<Aes256Gcm>::from(key.0))
}

fn additional_data(user: GithubUserId) -> String {
    format!("radar-token-v1:{user}")
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const TOKEN: &str = "gho_stubtoken0000";
    const USER: GithubUserId = GithubUserId(1001);

    fn key(byte: u8) -> TokenKey {
        TokenKey([byte; 32])
    }

    #[test]
    fn round_trips() {
        let sealed = seal(&key(1), USER, TOKEN).expect("seal");
        assert_eq!(sealed.nonce.len(), 12);
        assert!(sealed.cipher.len() > TOKEN.len());
        assert!(!sealed
            .cipher
            .windows(TOKEN.len())
            .any(|w| w == TOKEN.as_bytes()));
        assert_eq!(open(&key(1), USER, &sealed).as_deref(), Ok(TOKEN));
    }

    #[test]
    fn wrong_key_fails() {
        let sealed = seal(&key(1), USER, TOKEN).expect("seal");
        assert_eq!(open(&key(2), USER, &sealed), Err(TokenBoxError::Open));
    }

    #[test]
    fn wrong_user_id_in_the_additional_data_fails() {
        let sealed = seal(&key(1), USER, TOKEN).expect("seal");
        assert_eq!(
            open(&key(1), GithubUserId(1002), &sealed),
            Err(TokenBoxError::Open)
        );
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let sealed = seal(&key(1), USER, TOKEN).expect("seal");
        for index in 0..sealed.cipher.len() {
            let mut tampered = sealed.clone();
            tampered.cipher[index] ^= 0x01;
            assert_eq!(
                open(&key(1), USER, &tampered),
                Err(TokenBoxError::Open),
                "{index}"
            );
        }
        let mut nonce = sealed.clone();
        nonce.nonce[0] ^= 0x01;
        assert_eq!(open(&key(1), USER, &nonce), Err(TokenBoxError::Open));
        let mut short = sealed;
        short.nonce.pop();
        assert_eq!(open(&key(1), USER, &short), Err(TokenBoxError::Open));
    }

    #[test]
    fn a_thousand_seals_use_a_thousand_nonces() {
        let nonces: HashSet<Vec<u8>> = (0..1000)
            .map(|_| seal(&key(1), USER, TOKEN).expect("seal").nonce)
            .collect();
        assert_eq!(nonces.len(), 1000);
    }
}
