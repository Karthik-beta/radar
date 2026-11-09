mod api;
mod app;
mod auth;
mod components;
mod heat;
mod keys;
mod stage_label;
mod store;
mod sync_label;
mod url;
mod views;

fn main() {
    leptos::mount::mount_to_body(app::App);
}
