pub mod analytics;
pub mod billing;
pub mod dlc;
pub mod erp;
pub mod grpc;
pub mod identity;
pub mod rest;
pub mod services;
pub mod settlement;
pub mod zkml;

use std::sync::OnceLock;
use std::time::Instant;

pub static START_TIME: OnceLock<Instant> = OnceLock::new();

pub fn init_start_time() {
    START_TIME.get_or_init(Instant::now);
}

pub fn get_uptime() -> u64 {
    START_TIME.get().map(|t| t.elapsed().as_secs()).unwrap_or(0)
}
