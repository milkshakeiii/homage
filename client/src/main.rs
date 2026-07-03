//! Windowed client binary. All the actual logic lives in the library
//! (`homage_client::build_client_app`) so integration tests can drive
//! headless clients in-process.
//!
//! Run with `cargo run -p homage_client -- <client_id>`. The id must be unique
//! per connected client; it defaults to the process id so that launching
//! several clients without arguments still works. Pass `bot` as a second
//! argument for a self-driving client (constant thrust + turn + fire).

use homage_client::{build_client_app, ClientConfig};

fn main() {
    let client_id: u64 = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or_else(|| std::process::id() as u64);
    let bot = std::env::args().nth(2).is_some_and(|arg| arg == "bot");

    build_client_app(ClientConfig {
        client_id,
        server_addr: homage_shared::SERVER_ADDR,
        bot,
        headless: false,
    })
    .run();
}
