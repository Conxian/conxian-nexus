#[path = "support/conformance_database.rs"]
#[allow(dead_code)]
mod conformance_database;
use conformance_database::validate;

const FIXTURE: &str = "postgres://conformance:fixture-only@127.0.0.1:5433/conformance_test";

#[test]
fn missing_configuration_and_permission_fail_before_connection() {
    for value in [None, Some("")] {
        assert_eq!(
            validate(value, Some("true"), false),
            Err("CONFORMANCE_DATABASE_URL is required")
        );
    }
    for flag in [None, Some(""), Some("false"), Some("TRUE")] {
        assert!(validate(Some(FIXTURE), flag, false).is_err());
    }
}

#[test]
fn only_explicit_loopback_fixture_is_allowed() {
    assert!(validate(Some(FIXTURE), Some("true"), false).is_ok());
    assert!(validate(
        Some("postgresql://conformance:fixture-only@[::1]:5433/conformance_test"),
        Some("true"),
        false
    )
    .is_ok());
    for raw in [
        "not a URL",
        "postgres://conformance:p@db.example:5433/conformance_test",
        "postgres://conformance:p@127.0.0.1:5433/production",
        "postgres://postgres:p@127.0.0.1:5433/conformance_test",
        "postgres://conformance@127.0.0.1:5433/conformance_test",
        "postgres://conformance:p@127.0.0.1/conformance_test",
        "postgres://conformance:p@127.0.0.1:0/conformance_test",
        "postgres://conformance:p@localhost:5433/conformance_test",
        "postgres://conformance:p@127.0.0.1:5433/%63onformance_test",
        "postgres://conformance:p@127.0.0.1:5433/conformance_test?host=db.example",
        "postgres://conformance:p@127.0.0.1:5433/conformance_test?options=-csearch_path=public",
        "postgres://conformance:p@127.0.0.1:5433/conformance_test#ignored",
        " postgres://conformance:p@127.0.0.1:5433/conformance_test",
        "postgres://conformance:p@127.0.0.1:5433/conformance_test\n",
    ] {
        assert!(validate(Some(raw), Some("true"), false).is_err());
    }
}

#[test]
fn inherited_postgres_settings_are_rejected() {
    assert!(validate(Some(FIXTURE), Some("true"), true).is_err());
}

#[test]
fn validation_errors_do_not_echo_credentials() {
    let raw = "postgres://conformance:synthetic-sensitive-marker@db.example:5433/production";
    let error = validate(Some(raw), Some("true"), false).unwrap_err();
    assert!(!error.contains("synthetic-sensitive-marker"));
    assert!(!error.contains("db.example"));
}
