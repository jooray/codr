pub mod logging;
/// Like `eprintln!`, for lines that recur on every poll: each distinct line prints once per ten minutes.
macro_rules! eprintln_throttled {
    ($($arg:tt)*) => { $crate::logging::throttled(format!($($arg)*)) };
}
pub mod activity;
pub mod bridge;
pub mod catalogue;
mod commands;
pub mod config;
pub mod herdr;
pub mod launch;
pub mod launch_flow;
pub mod media;
pub mod outbound_images;
pub mod output;
mod reads;
pub mod state;
pub mod update;
pub mod wire;
pub use config::BridgeConfig;
pub use state::BridgeState;
pub mod forwarding;
pub mod porter;
