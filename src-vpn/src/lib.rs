mod android;
mod dns;
mod ping;
mod virtual_dns;
mod vpn;

pub use ping::start_ping_interval;
pub use vpn::start_run_vpn;
