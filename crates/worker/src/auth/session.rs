use domain::ids::{Login, Timestamp};
use domain::me::{GithubUserId, Me};
use worker::{console_error, Env, Request};

use super::cookie::{self, CookieError, Purpose, SESSION};
use super::keys::{AuthConfig, SessionKey};
use super::now;
use crate::db::auth as rows;
use crate::db::Statement;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub user_id: GithubUserId,
    pub login: Login,
    pub epoch: u32,
    pub expires_at: Timestamp,
}

/// Why a request has no session. `Unavailable` means Radar could not check:
/// a setting is missing or D1 failed. It still rejects the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionError {
    Missing,
    BadSignature,
    Expired,
    WrongUser,
    StaleEpoch,
    Unavailable,
}

impl SessionError {
    pub fn name(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::BadSignature => "bad_signature",
            Self::Expired => "expired",
            Self::WrongUser => "wrong_user",
            Self::StaleEpoch => "stale_epoch",
            Self::Unavailable => "unavailable",
        }
    }
}

impl Session {
    pub fn new(user_id: GithubUserId, login: Login, epoch: u32, now: Timestamp) -> Self {
        Self {
            user_id,
            login,
            epoch,
            expires_at: Timestamp(now.0 + SESSION.max_age_secs),
        }
    }

    pub fn me(&self) -> Me {
        Me {
            login: self.login.clone(),
            user_id: self.user_id,
        }
    }

    pub fn cookie_value(&self, key: &SessionKey) -> String {
        let body = format!("{}.{}.{}", self.user_id, self.epoch, self.login);
        cookie::seal(key, Purpose::Session, &body, self.expires_at)
    }
}

/// The checks that need no database: signature, expiry, and the allowed id.
/// The id is checked on every request, so changing `ALLOWED_USER_ID` locks out
/// an old cookie.
pub fn verify(
    cookie_header: Option<&str>,
    key: &SessionKey,
    allowed: GithubUserId,
    now: Timestamp,
) -> Result<Session, SessionError> {
    let value = cookie_header
        .and_then(|header| SESSION.find(header))
        .filter(|value| !value.is_empty())
        .ok_or(SessionError::Missing)?;
    let opened = cookie::open(key, Purpose::Session, value, now).map_err(|err| match err {
        CookieError::BadSignature => SessionError::BadSignature,
        CookieError::Expired => SessionError::Expired,
    })?;
    let mut fields = opened.body.splitn(3, '.');
    let (Some(user_id), Some(epoch), Some(login)) = (fields.next(), fields.next(), fields.next())
    else {
        return Err(SessionError::BadSignature);
    };
    let user_id = GithubUserId::parse(user_id).ok_or(SessionError::BadSignature)?;
    let epoch = epoch.parse().map_err(|_| SessionError::BadSignature)?;
    let login = Login::parse(login).map_err(|_| SessionError::BadSignature)?;
    if user_id != allowed {
        return Err(SessionError::WrongUser);
    }
    Ok(Session {
        user_id,
        login,
        epoch,
        expires_at: opened.expires_at,
    })
}

/// A disconnect raises the row's epoch, which ends every older cookie. With
/// no row there is nothing a session could stand for.
pub fn check_epoch(session: Session, current: Option<u32>) -> Result<Session, SessionError> {
    match current {
        Some(epoch) if epoch == session.epoch => Ok(session),
        _ => Err(SessionError::StaleEpoch),
    }
}

/// The session behind a request, or why there is none. R12 calls this first.
pub async fn require_session(req: &Request, env: &Env) -> Result<Session, SessionError> {
    let header = req.headers().get("cookie").ok().flatten();
    if header.as_deref().and_then(|h| SESSION.find(h)).is_none() {
        return Err(SessionError::Missing);
    }
    let config = AuthConfig::from_env(env).map_err(|err| {
        console_error!("auth config: {err}");
        SessionError::Unavailable
    })?;
    let session = verify(
        header.as_deref(),
        &config.session_key,
        config.allowed,
        now(),
    )?;
    let current = current_epoch(env).await.map_err(|err| {
        console_error!("auth epoch read failed: {err}");
        SessionError::Unavailable
    })?;
    check_epoch(session, current)
}

pub async fn current_epoch(env: &Env) -> worker::Result<Option<u32>> {
    let db = env.d1("DB")?;
    rows::epoch()
        .prepare(&db)?
        .first::<u32>(Some("session_epoch"))
        .await
}

/// The two ways to end a session. Logout forgets this browser's cookie only.
/// Disconnect drops the stored token and raises the epoch, which ends every
/// session everywhere, so it needs a valid session to ask for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignOut {
    Logout,
    Disconnect,
}

impl SignOut {
    pub fn needs_session(self) -> bool {
        matches!(self, Self::Disconnect)
    }

    pub fn statements(self) -> Vec<Statement> {
        match self {
            Self::Logout => Vec::new(),
            Self::Disconnect => vec![rows::disconnect()],
        }
    }
}

/// State-changing auth requests must come from Radar's own pages. A missing
/// `Origin` fails too, since every browser sends one on a POST.
pub fn same_origin(origin_header: Option<&str>, own_origin: &str) -> bool {
    origin_header.is_some_and(|origin| origin == own_origin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::auth::{upsert, AuthRow, EncryptedToken};
    use crate::db::testing::{exec, memory_db, query_json};

    const NOW: Timestamp = Timestamp(1_793_900_000);
    const ALLOWED: GithubUserId = GithubUserId(1001);

    fn key(byte: u8) -> SessionKey {
        SessionKey::from_bytes(&[byte; 32]).expect("key")
    }

    fn session(id: u64, epoch: u32) -> Session {
        Session::new(
            GithubUserId(id),
            Login::parse("karthik").expect("login"),
            epoch,
            NOW,
        )
    }

    fn header_for(session: &Session, key: &SessionKey) -> String {
        format!("theme=dark; radar_session={}", session.cookie_value(key))
    }

    #[test]
    fn a_valid_cookie_round_trips() {
        let original = session(1001, 2);
        let header = header_for(&original, &key(1));
        assert_eq!(verify(Some(&header), &key(1), ALLOWED, NOW), Ok(original));
    }

    #[test]
    fn missing_cookie_is_missing() {
        assert_eq!(
            verify(None, &key(1), ALLOWED, NOW),
            Err(SessionError::Missing)
        );
        assert_eq!(
            verify(Some("theme=dark"), &key(1), ALLOWED, NOW),
            Err(SessionError::Missing)
        );
        assert_eq!(
            verify(Some("radar_session="), &key(1), ALLOWED, NOW),
            Err(SessionError::Missing)
        );
    }

    #[test]
    fn a_changed_character_or_other_key_is_a_bad_signature() {
        let header = header_for(&session(1001, 0), &key(1));
        for (from, to) in [(".karthik.", ".karthic."), (".1001.0.", ".1001.1.")] {
            let tampered = header.replacen(from, to, 1);
            assert_ne!(tampered, header);
            assert_eq!(
                verify(Some(&tampered), &key(1), ALLOWED, NOW),
                Err(SessionError::BadSignature)
            );
        }
        assert_eq!(
            verify(Some(&header), &key(2), ALLOWED, NOW),
            Err(SessionError::BadSignature)
        );
    }

    #[test]
    fn a_state_cookie_in_the_session_slot_is_a_bad_signature() {
        let state = cookie::seal(
            &key(1),
            Purpose::State,
            "1001.0.karthik",
            Timestamp(NOW.0 + 60),
        );
        let header = format!("radar_session={state}");
        assert_eq!(
            verify(Some(&header), &key(1), ALLOWED, NOW),
            Err(SessionError::BadSignature)
        );
    }

    #[test]
    fn an_old_cookie_is_expired() {
        let header = header_for(&session(1001, 0), &key(1));
        let later = Timestamp(NOW.0 + SESSION.max_age_secs);
        assert_eq!(
            verify(Some(&header), &key(1), ALLOWED, later),
            Err(SessionError::Expired)
        );
    }

    #[test]
    fn a_cookie_for_another_user_id_is_wrong_user() {
        let header = header_for(&session(1002, 0), &key(1));
        assert_eq!(
            verify(Some(&header), &key(1), ALLOWED, NOW),
            Err(SessionError::WrongUser)
        );
        let header = header_for(&session(1001, 0), &key(1));
        assert_eq!(
            verify(Some(&header), &key(1), GithubUserId(1002), NOW),
            Err(SessionError::WrongUser)
        );
    }

    #[test]
    fn a_cookie_with_an_old_epoch_is_stale() {
        assert_eq!(
            check_epoch(session(1001, 1), Some(2)),
            Err(SessionError::StaleEpoch)
        );
        assert_eq!(
            check_epoch(session(1001, 0), None),
            Err(SessionError::StaleEpoch)
        );
        assert_eq!(check_epoch(session(1001, 2), Some(2)), Ok(session(1001, 2)));
    }

    #[test]
    fn same_origin_guard_rejects_a_foreign_origin_and_a_missing_one() {
        let own = "https://radar.karthik.nz";
        assert!(same_origin(Some(own), own));
        assert!(!same_origin(Some("https://evil.example"), own));
        assert!(!same_origin(
            Some("https://radar.karthik.nz.evil.example"),
            own
        ));
        assert!(!same_origin(Some("http://radar.karthik.nz"), own));
        assert!(!same_origin(Some("null"), own));
        assert!(!same_origin(None, own));
    }

    fn signed_in_row() -> AuthRow {
        AuthRow {
            github_user_id: 1001,
            login: "karthik".into(),
            token: Some(EncryptedToken {
                cipher: vec![1; 33],
                nonce: vec![2; 12],
            }),
            scopes: "notifications, read:user".into(),
            scopes_checked_at: Some(NOW),
            created_at: NOW,
        }
    }

    fn auth_state(conn: &rusqlite::Connection) -> (i64, bool) {
        let rows = query_json(conn, "SELECT session_epoch, token_cipher FROM auth", &[]);
        let epoch = rows[0]["session_epoch"].as_i64().expect("epoch");
        (epoch, !rows[0]["token_cipher"].is_null())
    }

    #[test]
    fn logout_leaves_the_epoch_unchanged() {
        let conn = memory_db();
        exec(&conn, &upsert(&signed_in_row())).expect("sign in");
        for stmt in SignOut::Logout.statements() {
            exec(&conn, &stmt).expect("logout");
        }
        assert_eq!(auth_state(&conn), (0, true));
    }

    #[test]
    fn disconnect_bumps_the_epoch_and_clears_the_token() {
        let conn = memory_db();
        exec(&conn, &upsert(&signed_in_row())).expect("sign in");
        for stmt in SignOut::Disconnect.statements() {
            exec(&conn, &stmt).expect("disconnect");
        }
        assert_eq!(auth_state(&conn), (1, false));
        let rows = query_json(&conn, "SELECT token_nonce FROM auth", &[]);
        assert!(rows[0]["token_nonce"].is_null());
        exec(&conn, &upsert(&signed_in_row())).expect("sign in again");
        assert_eq!(auth_state(&conn), (1, true));
    }
}
