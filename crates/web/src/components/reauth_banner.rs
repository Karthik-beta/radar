use domain::api::ScopeState;
use leptos::prelude::*;

use crate::store::use_store;

/// Whether the token needs a new grant or a new sign-in. `Unknown` (GitHub sent
/// no scope header) and a missing answer show nothing, so the banner never cries wolf.
pub fn shows(scope: Option<ScopeState>) -> bool {
    matches!(
        scope,
        Some(ScopeState::MissingNotifications | ScopeState::Rejected)
    )
}

fn wording(scope: Option<ScopeState>) -> (&'static str, &'static str) {
    match scope {
        Some(ScopeState::Rejected) => (
            "GitHub no longer accepts Radar's sign-in, so syncing has stopped. Sign in again to resume.",
            "Sign in again",
        ),
        _ => (
            "Radar cannot read your notifications. Authorize again to grant the notifications scope.",
            "Authorize again",
        ),
    }
}

/// Tells the operator to authorize again when the token cannot read
/// notifications (A5), or to sign in again when GitHub rejects it. It is a
/// plain link and cannot be dismissed.
#[component]
pub fn ReauthBanner() -> impl IntoView {
    let store = use_store();
    let scope = Memo::new(move |_| store.freshness().with(|f| f.data().map(|data| data.scope)));
    view! {
        <Show when=move || shows(scope.get())>
            <div class="reauth" role="alert" data-reauth>
                <p class="reauth-text">{move || wording(scope.get()).0}</p>
                <a class="reauth-link" href="/auth/login" rel="external">
                    {move || wording(scope.get()).1}
                </a>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_shows_only_for_a_missing_scope_or_a_rejected_token() {
        assert!(shows(Some(ScopeState::MissingNotifications)));
        assert!(shows(Some(ScopeState::Rejected)));
        assert!(!shows(Some(ScopeState::Ok)));
        assert!(!shows(Some(ScopeState::Unknown)));
        assert!(!shows(None));
    }

    #[test]
    fn a_rejected_token_asks_for_a_new_sign_in() {
        let (text, link) = wording(Some(ScopeState::Rejected));
        assert!(text.starts_with("GitHub no longer accepts Radar's sign-in"));
        assert_eq!(link, "Sign in again");
        let (text, link) = wording(Some(ScopeState::MissingNotifications));
        assert!(text.starts_with("Radar cannot read your notifications."));
        assert_eq!(link, "Authorize again");
    }
}
