use leptos::prelude::*;

use super::reauth_banner::ReauthBanner;

/// The slot above every view for a page-wide notice. It holds the
/// re-authorize banner and shows nothing otherwise.
#[component]
pub fn Banner() -> impl IntoView {
    view! {
        <div class="banner-slot" data-banner>
            <ReauthBanner/>
        </div>
    }
}
