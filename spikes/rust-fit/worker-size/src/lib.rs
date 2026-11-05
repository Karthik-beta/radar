use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use worker::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Actor {
    login: String,
    #[serde(rename = "__typename")]
    typename: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pr {
    number: u32,
    title: String,
    is_draft: bool,
    review_decision: Option<String>,
    updated_at: String,
    author: Option<Actor>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
enum Turn {
    Me,
    Them,
}

#[derive(Serialize)]
struct Row {
    number: u32,
    title: String,
    turn: Turn,
    since: String,
}

fn classify(pr: &Pr, me: &str) -> Turn {
    match &pr.author {
        Some(a) if a.typename == "Bot" => Turn::Them,
        Some(a) if a.login == me => Turn::Them,
        _ if pr.review_decision.as_deref() == Some("CHANGES_REQUESTED") => Turn::Me,
        _ => Turn::Me,
    }
}

fn sign(key: &[u8], value: &str) -> String {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("any key length");
    mac.update(value.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

fn decrypt(key: &[u8; 32], nonce: &[u8; 12], data: &[u8]) -> Option<Vec<u8>> {
    Aes256Gcm::new(key.into()).decrypt(Nonce::from_slice(nonce), data).ok()
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    Router::new()
        .get_async("/api/dashboard", |_req, ctx| async move {
            let db = ctx.env.d1("DB")?;
            let rows: Vec<serde_json::Value> = db.prepare("SELECT * FROM items WHERE turn = ?1").bind(&["me".into()])?.all().await?.results()?;
            Response::from_json(&rows)
        })
        .post_async("/sync", |_req, ctx| async move {
            let stub = ctx.env.durable_object("SYNCER")?.id_from_name("me")?.get_stub()?;
            stub.fetch_with_str("https://syncer/kick").await
        })
        .run(req, env)
        .await
}

#[event(scheduled)]
async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    let kick = async {
        let stub = env.durable_object("SYNCER")?.id_from_name("me")?.get_stub()?;
        stub.fetch_with_str("https://syncer/kick").await
    };
    let _: Result<Response> = kick.await;
}

#[event(email)]
async fn email(message: ForwardableEmailMessage, env: Env, _ctx: Context) -> Result<()> {
    let headers = Headers(message.headers());
    let reason = headers.get("x-github-reason")?.unwrap_or_default();
    let repo = headers.get("list-id")?.unwrap_or_default();
    console_log!("trigger {reason} {repo}");
    let stub = env.durable_object("SYNCER")?.id_from_name("me")?.get_stub()?;
    stub.fetch_with_str("https://syncer/kick").await?;
    Ok(())
}

#[durable_object]
pub struct Syncer {
    state: State,
    env: Env,
}

impl DurableObject for Syncer {
    fn new(state: State, env: Env) -> Self {
        let sql = state.storage().sql();
        let _ = sql.exec("CREATE TABLE IF NOT EXISTS work(kind TEXT, key TEXT, due_at INTEGER, attempts INTEGER, PRIMARY KEY(kind, key))", None);
        Self { state, env }
    }

    async fn fetch(&self, _req: Request) -> Result<Response> {
        self.state.storage().set_alarm(Date::now().as_millis() as i64).await?;
        Response::ok("kicked")
    }

    async fn alarm(&self) -> Result<Response> {
        let token = self.env.secret("TOKEN_KEY")?.to_string();
        let key: [u8; 32] = token.as_bytes().get(..32).and_then(|k| k.try_into().ok()).unwrap_or([0; 32]);
        let _plain = decrypt(&key, &[0; 12], b"ciphertext");
        let mut init = RequestInit::new();
        init.with_method(Method::Post).with_body(Some(r#"{"query":"query { viewer { login } }"}"#.into()));
        let mut res = Fetch::Request(Request::new_with_init("https://api.github.com/graphql", &init)?).send().await?;
        let prs: Vec<Pr> = res.json().await?;
        let rows: Vec<Row> = prs
            .iter()
            .map(|p| Row { number: p.number, title: p.title.clone(), turn: classify(p, "me"), since: p.updated_at.clone() })
            .filter(|r| !r.title.is_empty() && r.number > 0)
            .collect();
        let draft = prs.iter().filter(|p| p.is_draft).count();
        let db = self.env.d1("DB")?;
        db.prepare("INSERT INTO items(json) VALUES (?1)").bind(&[serde_json::to_string(&rows)?.into()])?.run().await?;
        Response::ok(format!("{} rows, {draft} drafts, sig {}", rows.len(), sign(&key, "session")))
    }
}
