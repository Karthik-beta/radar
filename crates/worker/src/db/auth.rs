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

    #[test]
    fn cipher_without_nonce_is_rejected() {
        let conn = memory_db();
        let mut stmt = upsert(&auth("karthik-beta"));
        stmt.params[3] = Param::Null;
        assert!(exec(&conn, &stmt).is_err());
    }
}
