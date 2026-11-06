use domain::me::Me;
use gloo_net::http::Request;

/// What the app knows about the visitor. It decides the whole screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthState {
    Loading,
    SignedOut,
    SignedIn(Me),
}

/// Asks the worker who is signed in. Anything but a valid `Me` is signed out,
/// so a broken answer shows the sign-in page rather than an empty shell.
pub async fn load() -> AuthState {
    let Ok(response) = Request::get("/api/me").send().await else {
        return AuthState::SignedOut;
    };
    if response.status() != 200 {
        return AuthState::SignedOut;
    }
    match response.json::<Me>().await {
        Ok(me) => AuthState::SignedIn(me),
        Err(_) => AuthState::SignedOut,
    }
}

/// Why the worker sent the browser back to `/signin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignInError {
    State,
    Cancelled,
    Exchange,
}

impl SignInError {
    pub fn parse(code: &str) -> Option<Self> {
        match code {
            "state" => Some(Self::State),
            "cancelled" => Some(Self::Cancelled),
            "exchange" => Some(Self::Exchange),
            _ => None,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::State => "That sign-in link expired or was already used. Start again.",
            Self::Cancelled => "You cancelled the sign-in on GitHub. Nothing was saved.",
            Self::Exchange => "GitHub did not finish the sign-in. Try again in a minute.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_worker_error_code_has_a_message() {
        for code in ["state", "cancelled", "exchange"] {
            let error = SignInError::parse(code).expect(code);
            assert!(error.message().ends_with('.'));
        }
        assert_eq!(SignInError::parse("other"), None);
        assert_eq!(SignInError::parse(""), None);
    }
}
