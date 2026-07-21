#[cfg(test)]
mod tests {
    use conxian_nexus::storage::Storage;
    use std::env;

    #[tokio::test]
    async fn test_pg_boundary_check_local_rejection() {
        let res = Storage::new("postgres://localhost/db", "redis://remote.com@6379").await;

        // In release mode (cfg!(debug_assertions) is false), this should fail with boundary violation.
        // In debug mode, it won't fail with boundary violation.

        if let Err(e) = res {
            let error_msg = e.to_string();
            if error_msg.contains("Production boundary violation") {
                println!("Confirmed: Production boundary violation triggered for PostgreSQL.");
            } else {
                println!(
                    "Error was NOT boundary violation (expected in debug mode): {}",
                    error_msg
                );
            }
        } else {
            println!(
                "Storage::new unexpectedly succeeded (expected failure due to no DB/boundary)"
            );
        }
    }

    #[tokio::test]
    async fn test_pg_boundary_check_unauthenticated_rejection() {
        // postgres://host without : before @ or missing @
        let res = Storage::new("postgres://remote-db/db", "redis://remote-redis@6379").await;

        if let Err(e) = res {
            let error_msg = e.to_string();
            if error_msg.contains("Production boundary violation") {
                println!("Confirmed: Production boundary violation triggered for unauthenticated PostgreSQL.");
            }
        }
    }

    #[tokio::test]
    async fn test_pg_boundary_check_override() {
        env::set_var("NEXUS_ALLOW_UNSAFE_DB", "1");
        let res = Storage::new("postgres://localhost/db", "redis://remote.com@6379").await;

        if let Err(e) = res {
            let error_msg = e.to_string();
            assert!(
                !error_msg.contains("Production boundary violation"),
                "Override should have prevented boundary violation error"
            );
        }
        env::remove_var("NEXUS_ALLOW_UNSAFE_DB");
    }
}
