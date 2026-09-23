mod database;
pub mod model;
pub mod storage;
pub mod tailscale;

pub use model::*;
pub use storage::Store;

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
