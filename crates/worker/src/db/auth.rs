use domain::ids::Timestamp;

use super::sql::{Param, Statement};

/// `session_epoch` is left out of the update so a re-login never un-revokes
/// the sessions R07 revoked. `created_at` stays as first written.
const UPSERT: &str = "INSERT INTO auth (id, github_user_id, login, token_cipher, token_nonce, \
     scopes, scopes_checked_at, created_at) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7) \
     ON CONFLICT (id) DO UPDATE SET \
     github_user_id = excluded.github_user_id, login = excluded.login, \
     token_cipher = excluded.token_cipher, token_nonce = excluded.token_nonce, \
     scopes = excluded.scopes, scopes_checked_at = excluded.scopes_checked_at";

const EPOCH: &str = "SELECT session_epoch FROM auth WHERE id = 1";

const SEALED_TOKEN: &str =
    "SELECT github_user_id, token_cipher, token_nonce FROM auth WHERE id = 1";

/// Ends every session and drops the token, keeping the row and its login.
const DISCONNECT: &str = "UPDATE auth SET token_cipher = NULL, token_nonce = NULL, \
     session_epoch = session_epoch + 1 WHERE id = 1";

const PRUNE_STATES: &str = "DELETE FROM used_oauth_states WHERE expires_at <= ?1";

/// Returns the row only when this call inserted it, so a second use of a state
/// returns nothing.
const SPEND_STATE: &str = "INSERT INTO used_oauth_states (state_hash, expires_at) \
     VALUES (?1, ?2) ON CONFLICT (state_hash) DO NOTHING RETURNING 1 AS spent";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedToken {
    pub cipher: Vec<u8>,
    pub nonce: Vec<u8>,
}

/// The one signed-in account. `token` is `None` after a disconnect, which
/// clears the token and keeps the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRow {
    pub github_user_id: i64,
    pub login: String,
    pub token: Option<EncryptedToken>,
    pub scopes: String,
    pub scopes_checked_at: Option<Timestamp>,
    pub created_at: Timestamp,
}

pub fn upsert(row: &AuthRow) -> Statement {
    let (cipher, nonce) = row.token.as_ref().map_or((Param::Null, Param::Null), |t| {
        (Param::Blob(t.cipher.clone()), Param::Blob(t.nonce.clone()))
    });
    Statement {
        sql: UPSERT,
        params: vec![
            Param::Int(row.github_user_id),
            Param::text(&row.login),
            cipher,
            nonce,
            Param::text(&row.scopes),
            Param::opt_ts(row.scopes_checked_at),
            Param::ts(row.created_at),
        ],
    }
}

pub fn epoch() -> Statement {
    Statement {
        sql: EPOCH,
        params: Vec::new(),
    }
}

pub fn sealed_token() -> Statement {
    Statement {
        sql: SEALED_TOKEN,
        params: Vec::new(),
    }
}

pub fn disconnect() -> Statement {
    Statement {
        sql: DISCONNECT,
        params: Vec::new(),
    }
}

/// Prunes spent states that have expired, then spends this one. Run as one
/// batch; the second statement returns a row only on first use.
pub fn spend_state(state_hash: &[u8; 32], expires_at: Timestamp, now: Timestamp) -> [Statement; 2] {
    [
        Statement {
            sql: PRUNE_STATES,
            params: vec![Param::ts(now)],
        },
        Statement {
            sql: SPEND_STATE,
            params: vec![Param::Blob(state_hash.to_vec()), Param::ts(expires_at)],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::{count_rows, exec, memory_db, query_json};

    fn auth(login: &str) -> AuthRow {
        AuthRow {
            github_user_id: 583_231,
            login: login.to_owned(),
            token: Some(EncryptedToken {
                cipher: vec![1, 2, 3],
                nonce: vec![9; 12],
            }),
            scopes: "read:user,notifications,repo".to_owned(),
            scopes_checked_at: Some(Timestamp(1000)),
            created_at: Timestamp(500),
        }
    }

    #[test]
    fn second_auth_row_is_rejected() {
        let conn = memory_db();
        exec(&conn, &upsert(&auth("karthik-beta"))).expect("first");
        let err = conn
            .execute(
                "INSERT INTO auth (id, github_user_id, login, scopes, created_at) \
                 VALUES (2, 1, 'intruder', '', 0)",
                [],
            )
            .expect_err("id = 2 must fail");
        assert!(err.to_string().contains("CHECK"), "{err}");
        assert_eq!(count_rows(&conn, "auth"), 1);
    }

    #[test]
    fn relogin_updates_the_row_and_keeps_the_revoked_epoch() {
        let conn = memory_db();
        exec(&conn, &upsert(&auth("karthik-beta"))).expect("first");
        conn.execute("UPDATE auth SET session_epoch = 4", [])
            .expect("revoke");
        let disconnected = AuthRow {
            token: None,
            created_at: Timestamp(9999),
            ..auth("karthik-beta")
        };
        exec(&conn, &upsert(&disconnected)).expect("disconnect");
        let rows = query_json(
            &conn,
            "SELECT session_epoch, created_at, token_cipher, token_nonce FROM auth",
            &[],
        );
        assert_eq!(rows[0]["session_epoch"], 4);
        assert_eq!(rows[0]["created_at"], 500);
        assert!(rows[0]["token_cipher"].is_null() && rows[0]["token_nonce"].is_null());
        assert_eq!(count_rows(&conn, "auth"), 1);
    }

    fn with_states() -> rusqlite::Connection {
        let conn = memory_db();
        conn.execute_batch(include_str!(
            "../../../../migrations/0002_used_oauth_states.sql"
        ))
        .expect("0002 applies");
        conn
    }

    fn spend(conn: &rusqlite::Connection, hash: u8, expires: i64, now: i64) -> usize {
        let [prune, spend] = spend_state(&[hash; 32], Timestamp(expires), Timestamp(now));
        exec(conn, &prune).expect("prune");
        query_json(conn, spend.sql, &spend.params).len()
    }

    #[test]
    fn a_state_spends_once() {
        let conn = with_states();
        assert_eq!(spend(&conn, 1, 700, 100), 1);
        assert_eq!(spend(&conn, 1, 700, 101), 0);
        assert_eq!(spend(&conn, 2, 700, 102), 1);
        assert_eq!(count_rows(&conn, "used_oauth_states"), 2);
    }

    #[test]
    fn expired_spent_states_are_pruned() {
        let conn = with_states();
        assert_eq!(spend(&conn, 1, 700, 100), 1);
        assert_eq!(spend(&conn, 2, 1300, 700), 1);
        assert_eq!(count_rows(&conn, "used_oauth_states"), 1);
    }

    #[test]
    fn epoch_and_sealed_token_read_the_one_row() {
        let conn = memory_db();
        assert!(query_json(&conn, EPOCH, &[]).is_empty());
        exec(&conn, &upsert(&auth("karthik-beta"))).expect("first");
        assert_eq!(query_json(&conn, EPOCH, &[])[0]["session_epoch"], 0);
        let sealed = query_json(&conn, SEALED_TOKEN, &[]);
        assert_eq!(sealed[0]["github_user_id"], 583_231);
        assert_eq!(sealed[0]["token_nonce"].as_array().map(Vec::len), Some(12));
        exec(&conn, &disconnect()).expect("disconnect");
        assert_eq!(query_json(&conn, EPOCH, &[])[0]["session_epoch"], 1);
        assert!(query_json(&conn, SEALED_TOKEN, &[])[0]["token_cipher"].is_null());
    }

    #[test]
    fn cipher_without_nonce_is_rejected() {
        let conn = memory_db();
        let mut stmt = upsert(&auth("karthik-beta"));
        stmt.params[3] = Param::Null;
        assert!(exec(&conn, &stmt).is_err());
    }
}
