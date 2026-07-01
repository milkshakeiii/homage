//! Code shared between the homage client and server: the network protocol
//! (components, inputs) and the ship simulation that must run identically on
//! both sides for prediction to work.

pub mod protocol;
pub mod ship;

use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use core::time::Duration;

pub const FIXED_TIMESTEP_HZ: f64 = 64.0;
pub const SERVER_PORT: u16 = 5888;
// Bind/connect on loopback for now; switch to 0.0.0.0 when hosting for LAN/WAN.
pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), SERVER_PORT);

/// How often the server sends replication updates to clients.
pub const SEND_INTERVAL: Duration = Duration::from_millis(50);

pub const PROTOCOL_ID: u64 = 0x484f_4d41_4745; // "HOMAGE"
pub const PRIVATE_KEY: [u8; 32] = [0; 32];
