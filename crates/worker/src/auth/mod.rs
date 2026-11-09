//! Sign-in with GitHub for the one allowed account: the OAuth flow, the signed
//! session cookie, and the GitHub token sealed at rest.

pub mod cookie;
pub mod exchange;
pub mod flow;
pub mod keys;
pub mod pages;
pub mod route;
pub mod session;
pub mod token_box;

use domain::error::{ErrorBody, ErrorCode};
use domain::ids::{Login, Timestamp};
use domain::me::GithubUserId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use worker::{console_error, console_log, Date, Env, Headers, Request, Response, Result};

use crate::db::auth::{self as rows, AuthRow, EncryptedToken};
use crate::github::{ApiPath, Client, Conditional, Fetched, Token};
use cookie::{Purpose, SESSION, STATE};
use exchange::{AccessToken, Grant};
use flow::{CallbackOutcome, CallbackQuery, Identity};
use keys::AuthConfig;
use pages::Page;
use session::{same_origin, SignOut};

pub use route::Route;
pub use session::{require_session, Session, SessionError};

pub async fn handle(route: Route, req: &Request, env: &Env) -> Result<Response> {
    let result = match route {
        Route::Login => login(req, env),
        Route::Callback => callback(req, env).await,
        Route::Logout => sign_out(SignOut::Logout, req, env).await,
        Route::Disconnect => sign_out(SignOut::Disconnect, req, env).await,
        Route::Me => me(req, env).await,
        Route::WrongMethod => Ok(Response::empty()?.with_status(405)),
    };
    let mut response = result.or_else(|err| {
        console_error!("auth {route:?} failed: {err}");
        match route {
            Route::Me => error(500, ErrorCode::Internal),
            _ => page(Page::Failed, &[STATE.clear()]),
        }
    })?;
    response.headers_mut().set("cache-control", "no-store")?;
    Ok(response)
}

fn login(req: &Request, env: &Env) -> Result<Response> {
    let Some(config) = config(env) else {
        return page(Page::NotConfigured, &[]);
    };
    let redirect_uri = callback_url(req)?;
    let state = flow::new_state().ok_or_else(|| failure("no random bytes for the state"))?;
    let target = flow::authorize_url(&config.web_base, &config.client_id, &redirect_uri, &state);
    let expires_at = Timestamp(now().0 + STATE.max_age_secs);
    let sealed = cookie::seal(&config.session_key, Purpose::State, &state, expires_at);
    redirect(&target, &[STATE.set(&sealed)])
}

async fn callback(req: &Request, env: &Env) -> Result<Response> {
    let clear_state = STATE.clear();
    let Some(config) = config(env) else {
        return page(Page::NotConfigured, &[clear_state]);
    };
    let now = now();
    let query = CallbackQuery::parse(&req.inner().url());
    let cookies = req.headers().get("cookie")?;
    let state_cookie = cookies.as_deref().and_then(|header| STATE.find(header));
    let state = query.state.as_deref().unwrap_or_default();
    let (outcome, signed_in) = match flow::admit(&query, state_cookie, &config.session_key, now) {
        Err(outcome) => (outcome, None),
        Ok(code) => {
            if spend_state(env, state, now).await? {
                let signed_in = identify(&config, &code, &callback_url(req)?).await;
                let epoch = match signed_in {
                    Some(_) => session::current_epoch(env).await?.unwrap_or(0),
                    None => 0,
                };
                let identity = signed_in.as_ref().map(|s| s.identity.clone());
                (
                    flow::decide(identity, config.allowed, epoch, now),
                    signed_in,
                )
            } else {
                (CallbackOutcome::BadState, None)
            }
        }
    };
    console_log!("auth callback outcome={}", outcome.name());
    match (outcome, signed_in) {
        (CallbackOutcome::Signed(session), Some(signed_in)) => {
            store(env, &config, &session, signed_in, now).await?;
            let value = session.cookie_value(&config.session_key);
            redirect("/", &[clear_state, SESSION.set(&value)])
        }
        (CallbackOutcome::Denied { user_id }, _) => {
            console_log!("auth callback denied user_id={user_id}");
            page(Page::Denied, &[clear_state])
        }
        (outcome, _) => {
            let error = outcome.sign_in_error().unwrap_or("exchange");
            redirect(&format!("/signin?error={error}"), &[clear_state])
        }
    }
}

/// What the exchange and `GET /user` returned for a code.
struct SignedIn {
    identity: Identity,
    token: AccessToken,
    scopes: Vec<String>,
}

#[derive(Deserialize)]
struct GithubUser {
    id: u64,
    login: Login,
}

async fn identify(config: &AuthConfig, code: &str, redirect_uri: &str) -> Option<SignedIn> {
    let grant = Grant {
        client_id: &config.client_id,
        client_secret: &config.client_secret.0,
        code,
        redirect_uri,
    };
    let token = exchange::exchange(&config.web_base, &grant)
        .await
        .map_err(|err| console_log!("auth exchange failed: {err:?}"))
        .ok()?;
    let client = Token::new(token.0.clone())
        .and_then(|bearer| Client::new(&config.api_base, bearer))
        .and_then(|client| ApiPath::new("/user").map(|path| (client, path)));
    let (client, path) = client
        .map_err(|err| console_error!("auth user lookup config: {err}"))
        .ok()?;
    match client.get::<GithubUser>(path, Conditional::None).await {
        Ok(Fetched::Modified { body, meta, .. }) => Some(SignedIn {
            identity: Identity {
                user_id: GithubUserId(body.id),
                login: body.login,
            },
            token,
            scopes: meta.scopes.unwrap_or_default(),
        }),
        Ok(Fetched::NotModified { .. }) => None,
        Err(err) => {
            console_log!("auth user lookup failed: {err}");
            None
        }
    }
}

/// Marks the state used. A replayed callback finds it spent and stops before
/// the code reaches GitHub.
async fn spend_state(env: &Env, state: &str, now: Timestamp) -> Result<bool> {
    #[derive(Deserialize)]
    struct Spent {}
    let hash: [u8; 32] = Sha256::digest(state.as_bytes()).into();
    let expires_at = Timestamp(now.0 + STATE.max_age_secs);
    let db = env.d1("DB")?;
    let results = crate::db::run(&db, rows::spend_state(&hash, expires_at, now).into()).await?;
    let spent = match results.get(1) {
        Some(result) => result.results::<Spent>()?.len(),
        None => 0,
    };
    Ok(spent == 1)
}

async fn store(
    env: &Env,
    config: &AuthConfig,
    session: &Session,
    signed_in: SignedIn,
    now: Timestamp,
) -> Result<()> {
    let sealed = token_box::seal(&config.token_key, session.user_id, &signed_in.token.0)
        .map_err(|err| failure(&format!("token seal {err:?}")))?;
    let row = AuthRow {
        github_user_id: i64::try_from(session.user_id.0).map_err(|_| failure("user id range"))?,
        login: session.login.to_string(),
        token: Some(sealed),
        scopes: signed_in.scopes.join(","),
        scopes_checked_at: Some(now),
        created_at: now,
    };
    crate::db::run(&env.d1("DB")?, vec![rows::upsert(&row)]).await?;
    Ok(())
}

async fn sign_out(kind: SignOut, req: &Request, env: &Env) -> Result<Response> {
    let presented = req.headers().get("origin")?;
    if !same_origin(presented.as_deref(), &origin(req)?) {
        console_log!("auth {kind:?} rejected reason=foreign_origin");
        return error(403, ErrorCode::Forbidden);
    }
    if kind.needs_session() {
        if let Err(err) = require_session(req, env).await {
            console_log!("auth {kind:?} rejected reason={}", err.name());
            return error(401, ErrorCode::Unauthorized);
        }
    }
    let statements = kind.statements();
    if !statements.is_empty() {
        crate::db::run(&env.d1("DB")?, statements).await?;
    }
    console_log!("auth {kind:?} done");
    redirect_with("/", 303, &[SESSION.clear()])
}

async fn me(req: &Request, env: &Env) -> Result<Response> {
    match require_session(req, env).await {
        Ok(session) => json(200, &session.me()),
        Err(SessionError::Unavailable) => error(500, ErrorCode::Internal),
        Err(err) => {
            console_log!("auth session rejected reason={}", err.name());
            error(401, ErrorCode::Unauthorized)
        }
    }
}

/// The GitHub token for background sync, or `None` when Radar is signed out,
/// disconnected, or cannot open the sealed token.
pub async fn load_token(env: &Env) -> Option<Token> {
    #[derive(Deserialize)]
    struct SealedRow {
        github_user_id: u64,
        token_cipher: Option<Vec<u8>>,
        token_nonce: Option<Vec<u8>>,
    }
    let config = config(env)?;
    let db = env.d1("DB").ok()?;
    let row = match rows::sealed_token()
        .prepare(&db)
        .ok()?
        .first::<SealedRow>(None)
        .await
    {
        Ok(row) => row?,
        Err(err) => {
            console_error!("auth token read failed: {err}");
            return None;
        }
    };
    let (Some(cipher), Some(nonce)) = (row.token_cipher, row.token_nonce) else {
        return None;
    };
    let sealed = EncryptedToken { cipher, nonce };
    let user = GithubUserId(row.github_user_id);
    let plain = token_box::open(&config.token_key, user, &sealed)
        .map_err(|err| console_error!("auth token open failed: {err:?}"))
        .ok()?;
    Token::new(plain).ok()
}

fn config(env: &Env) -> Option<AuthConfig> {
    AuthConfig::from_env(env)
        .map_err(|err| console_error!("auth config: {err}"))
        .ok()
}

pub(crate) fn origin(req: &Request) -> Result<String> {
    let url = req.inner().url();
    flow::origin_of(&url)
        .map(str::to_owned)
        .ok_or_else(|| failure("request url has no origin"))
}

fn callback_url(req: &Request) -> Result<String> {
    Ok(format!("{}/auth/callback", origin(req)?))
}

pub(crate) fn now() -> Timestamp {
    Timestamp(i64::try_from(Date::now().as_millis() / 1000).unwrap_or(i64::MAX))
}

fn failure(what: &str) -> worker::Error {
    worker::Error::RustError(what.to_owned())
}

fn redirect(location: &str, cookies: &[String]) -> Result<Response> {
    redirect_with(location, 302, cookies)
}

fn redirect_with(location: &str, status: u16, cookies: &[String]) -> Result<Response> {
    let headers = with_cookies(cookies)?;
    headers.set("location", location)?;
    Ok(Response::empty()?.with_status(status).with_headers(headers))
}

fn page(page: Page, cookies: &[String]) -> Result<Response> {
    let headers = with_cookies(cookies)?;
    headers.set("content-type", "text/html; charset=utf-8")?;
    Ok(Response::ok(page.html())?
        .with_status(page.status())
        .with_headers(headers))
}

fn json<T: Serialize>(status: u16, body: &T) -> Result<Response> {
    Ok(Response::from_json(body)?.with_status(status))
}

fn error(status: u16, code: ErrorCode) -> Result<Response> {
    json(status, &ErrorBody { error: code })
}

fn with_cookies(cookies: &[String]) -> Result<Headers> {
    let headers = Headers::new();
    for cookie in cookies {
        headers.append("set-cookie", cookie)?;
    }
    Ok(headers)
}
