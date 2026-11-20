#[cfg(target_arch = "wasm32")]
mod ui;

fn main() {
    #[cfg(target_arch = "wasm32")]
    leptos::mount::mount_to_body(ui::app);
}
