pub mod grpc;
pub mod rest;
pub mod billing;
pub mod services;

use std::sync::OnceLock;
use std::time::Instant;

pub static START_TIME: OnceLock<Instant> = OnceLock::new();

pub fn init_start_time() {
    START_TIME.get_or_init(Instant::now);
}

pub fn get_uptime() -> u64 {
    START_TIME.get().map(|t| t.elapsed().as_secs()).unwrap_or(0)
}
