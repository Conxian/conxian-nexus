pub mod admin;
pub mod analytics;
pub mod billing;
pub mod canonical_bitvm;
pub mod capabilities;
pub mod dlc;
pub mod erp;
pub mod grpc;
pub mod identity;
pub mod rest;
pub mod security;
pub mod services;
pub mod settlement;
pub mod zkml;

use chrono::{DateTime, Utc};
use std::sync::OnceLock;
use std::time::Instant;

pub static START_TIME: OnceLock<Instant> = OnceLock::new();

/// Constant-time equality for secret/token comparison (Nexus #588, alert #58).
///
/// Comparing a caller-supplied bearer token with `==` short-circuits on the
/// first differing byte, which leaks a prefix-match timing signal. This folds
/// over every byte of the longer input so the work is independent of where the
/// inputs first differ (and of their lengths, up to the longer input).
pub fn constant_time_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let max = left.len().max(right.len());
    // Length inequality is folded in as a difference so equal-length work is
    // still performed; an empty-vs-nonempty pair never compares equal.
    let mut difference = (left.len() ^ right.len()) as u8;
    for index in 0..max {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        difference |= a ^ b;
    }
    difference == 0
}

pub static START_TIME_UTC: OnceLock<DateTime<Utc>> = OnceLock::new();

pub fn init_start_time() {
    START_TIME_UTC.get_or_init(Utc::now);
    START_TIME.get_or_init(Instant::now);
}

pub fn get_start_time_utc() -> Option<DateTime<Utc>> {
    START_TIME_UTC.get().cloned()
}

pub fn get_uptime() -> u64 {
    START_TIME.get().map(|t| t.elapsed().as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn equal_strings_compare_equal() {
        assert!(constant_time_eq("s3cret-token", "s3cret-token"));
    }

    #[test]
    fn differing_strings_compare_unequal() {
        assert!(!constant_time_eq("s3cret-token", "s3cret-tokeX"));
        assert!(!constant_time_eq("s3cret-token", "s3cret"));
        assert!(!constant_time_eq("", "nonempty"));
        assert!(!constant_time_eq("nonempty", ""));
    }

    #[test]
    fn empty_equals_empty() {
        assert!(constant_time_eq("", ""));
    }
}
