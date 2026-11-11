use leptos::prelude::*;

pub const SETTINGS_URL: &str = "https://github.com/settings/notifications";

/// Warns that GitHub notification emails stopped while threads kept changing (R19).
#[component]
pub fn EmailBanner(#[prop(into)] words: String) -> impl IntoView {
    view! {
        <div class="reauth email-silent" role="status" data-email-silent>
            <p class="reauth-text">{words}</p>
            <a class="reauth-link" href=SETTINGS_URL target="_blank" rel="noopener noreferrer">
                "Notification settings"
            </a>
        </div>
    }
}
