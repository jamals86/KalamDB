//! Proxy-based integration tests that simulate real network outages
//! by routing the client through a local TCP proxy.
//!
//! **IMPORTANT**: These tests require a running KalamDB server (auto-started).

mod common;

#[path = "proxied/helpers.rs"]
mod helpers;
#[path = "proxied/server_down_connecting.rs"]
mod server_down_connecting;
#[path = "proxied/server_down_initial_load.rs"]
mod server_down_initial_load;
#[path = "proxied/live_updates_resume.rs"]
mod live_updates_resume;
#[path = "proxied/double_outage.rs"]
mod double_outage;
#[path = "proxied/multi_sub_bounce.rs"]
mod multi_sub_bounce;
#[path = "proxied/socket_drop_resume.rs"]
mod socket_drop_resume;
