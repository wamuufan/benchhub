mod app;
mod components;
mod i18n;
mod tauri_bridge;
mod theme;

use app::App;

fn main() {
    leptos::mount::mount_to_body(App);
}
