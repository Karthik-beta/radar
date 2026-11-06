mod app;
mod auth;
mod components;
mod views;

fn main() {
    leptos::mount::mount_to_body(app::App);
}
