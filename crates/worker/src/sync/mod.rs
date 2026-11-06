use worker::{console_log, durable_object, DurableObject, Env, Request, Response, Result, State};

#[durable_object]
pub struct Syncer {
    _state: State,
}

impl DurableObject for Syncer {
    fn new(state: State, _env: Env) -> Self {
        Self { _state: state }
    }

    async fn fetch(&self, _req: Request) -> Result<Response> {
        console_log!("syncer kick");
        Response::ok("")
    }
}

pub async fn kick(env: &Env, reason: &str) -> Result<()> {
    let stub = env
        .durable_object("SYNCER")?
        .id_from_name("syncer")?
        .get_stub()?;
    stub.fetch_with_str(&format!("https://syncer/kick?reason={reason}"))
        .await?;
    Ok(())
}
