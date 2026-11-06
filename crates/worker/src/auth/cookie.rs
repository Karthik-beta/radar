use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use domain::ids::Timestamp;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::keys::SessionKey;

/// What a signed value is for. The purpose is part of the signed bytes, so a
/// state cookie never verifies as a session cookie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    State,
    Session,
}

impl Purpose {
    fn tag(self) -> &'static str {
        match self {
            Self::State => "state",
            Self::Session => "session",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieError {
    BadSignature,
    Expired,
}

/// `<expires_at>.<body>.<mac>`, where the MAC covers `<purpose>|<expires_at>.<body>`.
pub fn seal(key: &SessionKey, purpose: Purpose, body: &str, expires_at: Timestamp) -> String {
    let signed = format!("{}.{body}", expires_at.0);
    let mac = URL_SAFE_NO_PAD.encode(mac(key, purpose, &signed).finalize().into_bytes());
    format!("{signed}.{mac}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opened<'a> {
    pub body: &'a str,
    pub expires_at: Timestamp,
}

/// The body of a value this key sealed for this purpose, if it is still valid
/// at `now`. Expiry is exclusive: a value is dead at its `expires_at` second.
pub fn open<'a>(
    key: &SessionKey,
    purpose: Purpose,
    value: &'a str,
    now: Timestamp,
) -> Result<Opened<'a>, CookieError> {
    let (signed, tag) = value.rsplit_once('.').ok_or(CookieError::BadSignature)?;
    let tag = URL_SAFE_NO_PAD
        .decode(tag)
        .map_err(|_| CookieError::BadSignature)?;
    mac(key, purpose, signed)
        .verify_slice(&tag)
        .map_err(|_| CookieError::BadSignature)?;
    let (expires_at, body) = signed.split_once('.').ok_or(CookieError::BadSignature)?;
    let expires_at: i64 = expires_at.parse().map_err(|_| CookieError::BadSignature)?;
    if now.0 >= expires_at {
        return Err(CookieError::Expired);
    }
    Ok(Opened {
        body,
        expires_at: Timestamp(expires_at),
    })
}

fn mac(key: &SessionKey, purpose: Purpose, signed: &str) -> Hmac<Sha256> {
    let mut mac = key.0.clone();
    mac.update(purpose.tag().as_bytes());
    mac.update(b"|");
    mac.update(signed.as_bytes());
    mac
}

/// A cookie Radar sets, with the attributes every write and clear must repeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CookieSpec {
    pub name: &'static str,
    pub path: &'static str,
    pub max_age_secs: i64,
}

pub const SESSION: CookieSpec = CookieSpec {
    name: "radar_session",
    path: "/",
    max_age_secs: 30 * 86_400,
};

pub const STATE: CookieSpec = CookieSpec {
    name: "radar_oauth_state",
    path: "/auth",
    max_age_secs: 600,
};

impl CookieSpec {
    pub fn set(self, value: &str) -> String {
        self.header(value, self.max_age_secs)
    }

    pub fn clear(self) -> String {
        self.header("", 0)
    }

    fn header(self, value: &str, max_age: i64) -> String {
        format!(
            "{}={value}; Max-Age={max_age}; Path={}; HttpOnly; Secure; SameSite=Lax",
            self.name, self.path
        )
    }

    /// The first cookie with this name in a `Cookie` request header.
    pub fn find(self, header: &str) -> Option<&str> {
        header.split(';').find_map(|pair| {
            let (name, value) = pair.trim().split_once('=')?;
            (name == self.name).then_some(value)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: Timestamp = Timestamp(1_793_900_000);

    fn key(byte: u8) -> SessionKey {
        SessionKey::from_bytes(&[byte; 32]).expect("32-byte key")
    }

    #[test]
    fn round_trips() {
        let sealed = seal(
            &key(1),
            Purpose::Session,
            "1001.0.karthik",
            Timestamp(NOW.0 + 60),
        );
        assert_eq!(
            open(&key(1), Purpose::Session, &sealed, NOW),
            Ok(Opened {
                body: "1001.0.karthik",
                expires_at: Timestamp(NOW.0 + 60)
            })
        );
    }

    #[test]
    fn a_flipped_character_fails() {
        let sealed = seal(
            &key(1),
            Purpose::Session,
            "1001.0.karthik",
            Timestamp(NOW.0 + 60),
        );
        for index in 0..sealed.len() {
            let mut bytes = sealed.clone().into_bytes();
            bytes[index] = if bytes[index] == b'A' { b'B' } else { b'A' };
            let Ok(flipped) = String::from_utf8(bytes) else {
                continue;
            };
            assert_eq!(
                open(&key(1), Purpose::Session, &flipped, NOW),
                Err(CookieError::BadSignature),
                "flipped byte {index} of {sealed}"
            );
        }
    }

    #[test]
    fn a_state_cookie_is_not_a_session() {
        let state = seal(&key(1), Purpose::State, "abc", Timestamp(NOW.0 + 60));
        assert_eq!(
            open(&key(1), Purpose::Session, &state, NOW),
            Err(CookieError::BadSignature)
        );
        let session = seal(&key(1), Purpose::Session, "abc", Timestamp(NOW.0 + 60));
        assert_eq!(
            open(&key(1), Purpose::State, &session, NOW),
            Err(CookieError::BadSignature)
        );
    }

    #[test]
    fn expiry_is_exclusive_at_the_boundary() {
        let sealed = seal(&key(1), Purpose::State, "abc", NOW);
        assert_eq!(
            open(&key(1), Purpose::State, &sealed, Timestamp(NOW.0 - 1)),
            Ok(Opened {
                body: "abc",
                expires_at: NOW
            })
        );
        assert_eq!(
            open(&key(1), Purpose::State, &sealed, NOW),
            Err(CookieError::Expired)
        );
        assert_eq!(
            open(&key(1), Purpose::State, &sealed, Timestamp(NOW.0 + 1)),
            Err(CookieError::Expired)
        );
    }

    #[test]
    fn a_different_key_fails() {
        let sealed = seal(&key(1), Purpose::Session, "x", Timestamp(NOW.0 + 60));
        assert_eq!(
            open(&key(2), Purpose::Session, &sealed, NOW),
            Err(CookieError::BadSignature)
        );
    }

    #[test]
    fn an_extended_expiry_breaks_the_signature() {
        let sealed = seal(&key(1), Purpose::Session, "x", Timestamp(NOW.0 + 60));
        let (_, rest) = sealed.split_once('.').expect("dot");
        let forged = format!("{}.{rest}", NOW.0 + 999_999);
        assert_eq!(
            open(&key(1), Purpose::Session, &forged, NOW),
            Err(CookieError::BadSignature)
        );
    }

    #[test]
    fn garbage_is_a_bad_signature() {
        for junk in ["", ".", "..", "abc", "1.2.3", "1.x.!!!"] {
            assert_eq!(
                open(&key(1), Purpose::Session, junk, NOW),
                Err(CookieError::BadSignature),
                "{junk:?}"
            );
        }
    }

    #[test]
    fn set_cookie_carries_every_attribute() {
        assert_eq!(
            SESSION.set("v"),
            "radar_session=v; Max-Age=2592000; Path=/; HttpOnly; Secure; SameSite=Lax"
        );
        assert_eq!(
            STATE.clear(),
            "radar_oauth_state=; Max-Age=0; Path=/auth; HttpOnly; Secure; SameSite=Lax"
        );
    }

    #[test]
    fn find_reads_one_cookie_by_exact_name() {
        let header = "other=1; radar_session_x=2; radar_session=abc.def; radar_oauth_state=s";
        assert_eq!(SESSION.find(header), Some("abc.def"));
        assert_eq!(STATE.find(header), Some("s"));
        assert_eq!(SESSION.find("other=1"), None);
    }
}
