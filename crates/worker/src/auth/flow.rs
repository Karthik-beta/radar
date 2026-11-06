use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use domain::ids::{Login, Timestamp};
use domain::me::GithubUserId;
use subtle::ConstantTimeEq;
use worker::url::form_urlencoded;

use super::cookie::{self, CookieError, Purpose};
use super::exchange::WebBase;
use super::keys::SessionKey;
use super::session::Session;

pub const SCOPE: &str = "notifications read:user";
const STATE_BYTES: usize = 32;

/// How a callback ends. Only `Signed` sets a session or writes the `auth` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallbackOutcome {
    Signed(Session),
    Denied { user_id: GithubUserId },
    BadState,
    ProviderDenied,
    ExchangeFailed,
}

impl CallbackOutcome {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Signed(_) => "signed",
            Self::Denied { .. } => "denied",
            Self::BadState => "bad_state",
            Self::ProviderDenied => "provider_denied",
            Self::ExchangeFailed => "exchange_failed",
        }
    }

    /// The sign-in page message for an outcome that sends the browser back.
    pub fn sign_in_error(&self) -> Option<&'static str> {
        match self {
            Self::BadState => Some("state"),
            Self::ProviderDenied => Some("cancelled"),
            Self::ExchangeFailed => Some("exchange"),
            Self::Signed(_) | Self::Denied { .. } => None,
        }
    }
}

/// The OAuth state: 32 random bytes, base64url, so it is URL and cookie safe.
pub fn new_state() -> Option<String> {
    let mut bytes = [0u8; STATE_BYTES];
    getrandom::fill(&mut bytes).ok()?;
    Some(URL_SAFE_NO_PAD.encode(bytes))
}

pub fn authorize_url(base: &WebBase, client_id: &str, redirect_uri: &str, state: &str) -> String {
    base.authorize_url(&[
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("state", state),
        ("scope", SCOPE),
    ])
}

/// `scheme://host[:port]` of an absolute request URL. Splitting the string
/// keeps the `url` crate's parser, and its IDNA tables, out of the Worker.
pub fn origin_of(url: &str) -> Option<&str> {
    let host_start = url.find("://")? + 3;
    let host_len = url[host_start..]
        .find(['/', '?', '#'])
        .unwrap_or(url.len() - host_start);
    (host_len > 0).then(|| &url[..host_start + host_len])
}

/// The callback's query string, read once.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CallbackQuery {
    pub state: Option<String>,
    pub code: Option<String>,
    pub error: Option<String>,
}

impl CallbackQuery {
    pub fn parse(url: &str) -> Self {
        let without_fragment = url.split('#').next().unwrap_or_default();
        let raw = without_fragment
            .split_once('?')
            .map_or("", |(_, query)| query);
        let mut query = Self::default();
        for (name, value) in form_urlencoded::parse(raw.as_bytes()) {
            let slot = match name.as_ref() {
                "state" => &mut query.state,
                "code" => &mut query.code,
                "error" => &mut query.error,
                _ => continue,
            };
            if slot.is_none() {
                *slot = Some(value.into_owned());
            }
        }
        query
    }
}

/// The first half of the callback. The state is checked before anything else,
/// so a forged callback never reaches GitHub. Returns the code to exchange.
pub fn admit(
    query: &CallbackQuery,
    state_cookie: Option<&str>,
    key: &SessionKey,
    now: Timestamp,
) -> Result<String, CallbackOutcome> {
    let expected = state_cookie
        .ok_or(CallbackOutcome::BadState)
        .and_then(|value| {
            cookie::open(key, Purpose::State, value, now)
                .map_err(|_: CookieError| CallbackOutcome::BadState)
        })?;
    let presented = query.state.as_deref().ok_or(CallbackOutcome::BadState)?;
    if !bool::from(expected.body.as_bytes().ct_eq(presented.as_bytes())) {
        return Err(CallbackOutcome::BadState);
    }
    if query.error.is_some() {
        return Err(CallbackOutcome::ProviderDenied);
    }
    query
        .code
        .clone()
        .filter(|code| !code.is_empty())
        .ok_or(CallbackOutcome::BadState)
}

/// Who GitHub says signed in, read from `GET /user` after the exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub user_id: GithubUserId,
    pub login: Login,
}

/// The second half of the callback. `epoch` is the `auth` row's current
/// `session_epoch`, which a sign-in never changes.
pub fn decide(
    identity: Option<Identity>,
    allowed: GithubUserId,
    epoch: u32,
    now: Timestamp,
) -> CallbackOutcome {
    let Some(identity) = identity else {
        return CallbackOutcome::ExchangeFailed;
    };
    if identity.user_id != allowed {
        return CallbackOutcome::Denied {
            user_id: identity.user_id,
        };
    }
    CallbackOutcome::Signed(Session::new(identity.user_id, identity.login, epoch, now))
}

#[cfg(test)]
mod tests {
    use worker::Url;

    use super::*;
    use crate::auth::cookie::STATE;
    use crate::auth::keys::SessionKey;

    const NOW: Timestamp = Timestamp(1_793_900_000);
    const ALLOWED: GithubUserId = GithubUserId(1001);

    fn key() -> SessionKey {
        SessionKey::from_bytes(&[3; 32]).expect("key")
    }

    fn state_cookie(state: &str) -> String {
        cookie::seal(
            &key(),
            Purpose::State,
            state,
            Timestamp(NOW.0 + STATE.max_age_secs),
        )
    }

    fn query(state: Option<&str>, code: Option<&str>, error: Option<&str>) -> CallbackQuery {
        CallbackQuery {
            state: state.map(str::to_owned),
            code: code.map(str::to_owned),
            error: error.map(str::to_owned),
        }
    }

    fn identity(id: u64, login: &str) -> Option<Identity> {
        Some(Identity {
            user_id: GithubUserId(id),
            login: Login::parse(login).expect("login"),
        })
    }

    #[test]
    fn allowed_id_signs_in() {
        match decide(identity(1001, "karthik"), ALLOWED, 3, NOW) {
            CallbackOutcome::Signed(session) => {
                assert_eq!(session.user_id, ALLOWED);
                assert_eq!(session.login.as_str(), "karthik");
                assert_eq!(session.epoch, 3);
                assert_eq!(session.expires_at, Timestamp(NOW.0 + 30 * 86_400));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn other_id_is_denied() {
        assert_eq!(
            decide(identity(2002, "someone-else"), ALLOWED, 0, NOW),
            CallbackOutcome::Denied {
                user_id: GithubUserId(2002)
            }
        );
    }

    #[test]
    fn id_is_compared_as_a_number() {
        let padded = GithubUserId::parse("01001").expect("id");
        assert!(matches!(
            decide(identity(1001, "karthik"), padded, 0, NOW),
            CallbackOutcome::Signed(_)
        ));
        assert_eq!(
            decide(identity(2002, "1001"), ALLOWED, 0, NOW),
            CallbackOutcome::Denied {
                user_id: GithubUserId(2002)
            }
        );
    }

    #[test]
    fn a_failed_exchange_or_user_lookup_is_exchange_failed() {
        assert_eq!(
            decide(None, ALLOWED, 0, NOW),
            CallbackOutcome::ExchangeFailed
        );
    }

    #[test]
    fn matching_state_and_code_admit_the_code() {
        let cookie = state_cookie("s1");
        assert_eq!(
            admit(
                &query(Some("s1"), Some("c"), None),
                Some(&cookie),
                &key(),
                NOW
            ),
            Ok("c".to_owned())
        );
    }

    #[test]
    fn missing_code_is_bad_state() {
        let cookie = state_cookie("s1");
        for code in [None, Some("")] {
            assert_eq!(
                admit(&query(Some("s1"), code, None), Some(&cookie), &key(), NOW),
                Err(CallbackOutcome::BadState)
            );
        }
    }

    #[test]
    fn state_mismatch_is_bad_state() {
        let cookie = state_cookie("s1");
        let cases = [
            (query(Some("s2"), Some("c"), None), Some(cookie.as_str())),
            (query(Some("s"), Some("c"), None), Some(cookie.as_str())),
            (query(None, Some("c"), None), Some(cookie.as_str())),
            (query(Some("s1"), Some("c"), None), None),
            (query(Some("s1"), Some("c"), None), Some("s1")),
            (query(Some("s1"), Some("c"), Some("access_denied")), None),
        ];
        for (q, cookie) in cases {
            assert_eq!(
                admit(&q, cookie, &key(), NOW),
                Err(CallbackOutcome::BadState),
                "{q:?} {cookie:?}"
            );
        }
        let expired = Timestamp(NOW.0 + STATE.max_age_secs);
        assert_eq!(
            admit(
                &query(Some("s1"), Some("c"), None),
                Some(&cookie),
                &key(),
                expired
            ),
            Err(CallbackOutcome::BadState)
        );
    }

    #[test]
    fn provider_error_is_provider_denied() {
        let cookie = state_cookie("s1");
        assert_eq!(
            admit(
                &query(Some("s1"), None, Some("access_denied")),
                Some(&cookie),
                &key(),
                NOW
            ),
            Err(CallbackOutcome::ProviderDenied)
        );
    }

    #[test]
    fn authorize_url_asks_for_exactly_notifications_and_read_user() {
        let base = WebBase::parse("https://github.com").expect("base");
        let state = new_state().expect("state");
        let url = authorize_url(
            &base,
            "stub-client",
            "https://radar.karthik.nz/auth/callback",
            &state,
        );
        let url = Url::parse(&url).expect("url");
        assert_eq!(url.origin().ascii_serialization(), "https://github.com");
        assert_eq!(url.path(), "/login/oauth/authorize");
        let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        assert_eq!(
            pairs,
            vec![
                ("client_id".into(), "stub-client".into()),
                (
                    "redirect_uri".into(),
                    "https://radar.karthik.nz/auth/callback".into()
                ),
                ("state".into(), state.clone()),
                ("scope".into(), "notifications read:user".into()),
            ]
        );
        assert_eq!(state.len(), 43);
        assert!(state
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        assert!(url.as_str().contains(&format!("state={state}&")));
    }

    #[test]
    fn states_do_not_repeat() {
        let a = new_state().expect("a");
        let b = new_state().expect("b");
        assert_ne!(a, b);
    }

    #[test]
    fn the_callback_query_keeps_the_first_of_each_parameter() {
        assert_eq!(
            CallbackQuery::parse("http://x/auth/callback?state=a&code=b&state=evil&error=e&x=1"),
            query(Some("a"), Some("b"), Some("e"))
        );
        assert_eq!(
            CallbackQuery::parse("http://x/auth/callback?code=a%2Bb+c#state=frag"),
            query(None, Some("a+b c"), None)
        );
        assert_eq!(
            CallbackQuery::parse("http://x/auth/callback"),
            CallbackQuery::default()
        );
    }

    #[test]
    fn origin_of_matches_the_url_crate() {
        for raw in [
            "https://radar.karthik.nz/auth/callback?code=1",
            "http://127.0.0.1:8807/auth/logout",
            "http://localhost:8807?x=1",
            "http://[::1]:8807#frag",
            "https://radar.karthik.nz",
        ] {
            let expected = Url::parse(raw).expect(raw).origin().ascii_serialization();
            assert_eq!(origin_of(raw), Some(expected.as_str()), "{raw}");
        }
        assert_eq!(origin_of("/auth/callback"), None);
        assert_eq!(origin_of("https:///x"), None);
    }
}
