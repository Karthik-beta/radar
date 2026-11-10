use leptos::ev;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlAnchorElement, MouseEvent, Url, UrlSearchParams};

use crate::components::tabs::Tab;

/// What the address bar asks the signed-in shell to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Tab(Tab),
    SignIn,
    NotFound,
}

impl Route {
    pub fn parse(path: &str) -> Self {
        if let Some(tab) = Tab::from_path(path) {
            return Self::Tab(tab);
        }
        match path.trim_end_matches('/') {
            "/signin" => Self::SignIn,
            _ => Self::NotFound,
        }
    }
}

/// The address bar's path, shared through context. Radar's own links change
/// it in place, and back and forward buttons change it through `popstate`.
#[derive(Clone, Copy)]
pub struct Location {
    path: RwSignal<String>,
}

/// Starts client-side navigation for links to Radar's own pages. A link that
/// opens a new tab, downloads, says `rel="external"`, or leaves the origin
/// loads as the browser would.
pub fn provide_location() -> Location {
    let location = Location {
        path: RwSignal::new(current_path()),
    };
    let click = window_event_listener(ev::click, move |event| {
        if let Some(to) = in_app_target(&event) {
            event.prevent_default();
            location.navigate(&to);
        }
    });
    let popstate = window_event_listener(ev::popstate, move |_| location.path.set(current_path()));
    on_cleanup(move || {
        click.remove();
        popstate.remove();
    });
    provide_context(location);
    location
}

pub fn use_location() -> Location {
    expect_context::<Location>()
}

impl Location {
    pub fn path(&self) -> Signal<String> {
        self.path.into()
    }

    /// Pushes `to` onto the history unless the address bar already shows it,
    /// then scrolls to its `#hash` target or to the top.
    pub fn navigate(&self, to: &str) {
        let location = window().location();
        let part = |part: Result<String, wasm_bindgen::JsValue>| part.unwrap_or_default();
        let full = part(location.pathname()) + &part(location.search()) + &part(location.hash());
        if full != to {
            if let Ok(history) = window().history() {
                let _ = history.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(to));
            }
        }
        self.path.set(current_path());
        scroll_to_hash_or_top();
    }

    /// Like `navigate`, but replaces the current history entry.
    pub fn replace(&self, to: &str) {
        if let Ok(history) = window().history() {
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(to));
        }
        self.path.set(current_path());
        scroll_to_hash_or_top();
    }
}

/// One value from the address bar's query string.
pub fn query(name: &str) -> Option<String> {
    let search = window().location().search().ok()?;
    UrlSearchParams::new_with_str(&search).ok()?.get(name)
}

fn current_path() -> String {
    window().location().pathname().unwrap_or_default()
}

fn in_app_target(event: &MouseEvent) -> Option<String> {
    if event.default_prevented()
        || event.button() != 0
        || event.meta_key()
        || event.alt_key()
        || event.ctrl_key()
        || event.shift_key()
    {
        return None;
    }
    let anchor = event
        .target()?
        .dyn_into::<Element>()
        .ok()?
        .closest("a")
        .ok()??
        .dyn_into::<HtmlAnchorElement>()
        .ok()?;
    let href = anchor.href();
    let external = anchor
        .get_attribute("rel")
        .is_some_and(|rel| rel.split([' ', '\t']).any(|part| part == "external"));
    if !anchor.target().is_empty()
        || href.is_empty()
        || external
        || anchor.has_attribute("download")
    {
        return None;
    }
    let url = Url::new(&href).ok()?;
    if url.origin() != window().location().origin().ok()? {
        return None;
    }
    Some([url.pathname(), url.search(), url.hash()].concat())
}

fn scroll_to_hash_or_top() {
    let hash = window().location().hash().unwrap_or_default();
    if let Some(id) = hash.strip_prefix('#').filter(|id| !id.is_empty()) {
        let id = js_sys::decode_uri(id)
            .ok()
            .and_then(|decoded| decoded.as_string())
            .unwrap_or_else(|| id.to_owned());
        if let Some(el) = document().get_element_by_id(&id) {
            el.scroll_into_view();
            return;
        }
    }
    window().scroll_to_with_x_and_y(0.0, 0.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_maps_paths_to_routes() {
        assert_eq!(Route::parse("/"), Route::Tab(Tab::YourTurn));
        assert_eq!(Route::parse("/theirs"), Route::Tab(Tab::TheirTurn));
        assert_eq!(Route::parse("/pipeline/"), Route::Tab(Tab::Pipeline));
        assert_eq!(Route::parse("/merged"), Route::Tab(Tab::Merged));
        assert_eq!(Route::parse("/signin"), Route::SignIn);
        assert_eq!(Route::parse("/zzz"), Route::NotFound);
        assert_eq!(Route::parse("/pipeline/extra"), Route::NotFound);
    }
}
