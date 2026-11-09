use domain::api::ScopeState;
use leptos::prelude::*;

use crate::store::use_store;

/// Whether the token needs a new grant. `Unknown` (GitHub sent no scope
/// header) and a missing answer show nothing, so the banner never cries wolf.
pub fn shows(scope: Option<ScopeState>) -> bool {
    scope == Some(ScopeState::MissingNotifications)
}

/// Tells the operator to authorize again when the token cannot read
/// notifications (A5). It is a plain link and cannot be dismissed.
#[component]
pub fn ReauthBanner() -> impl IntoView {
    let store = use_store();
    let missing =
        Memo::new(move |_| shows(store.freshness().with(|f| f.data().map(|data| data.scope))));
    view! {
        <Show when=move || missing.get()>
            <div class="reauth" role="alert" data-reauth>
                <p class="reauth-text">
                    "Radar cannot read your notifications. Authorize again to grant the notifications scope."
                </p>
                <a class="reauth-link" href="/auth/login" rel="external">
                    "Authorize again"
                </a>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_shows_only_for_a_missing_notifications_scope() {
        assert!(shows(Some(ScopeState::MissingNotifications)));
        assert!(!shows(Some(ScopeState::Ok)));
        assert!(!shows(Some(ScopeState::Unknown)));
        assert!(!shows(None));
    }
}
