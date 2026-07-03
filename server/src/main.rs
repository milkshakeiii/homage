//! Dedicated headless server binary. All the actual logic lives in the
//! library (`homage_server::build_server_app`) so integration tests can drive
//! the same app in-process.

fn main() {
    let mut app = homage_server::build_server_app(homage_shared::SERVER_ADDR);
    app.add_plugins(bevy::log::LogPlugin::default());
    app.run();
}
