import re

def improve_coverage():
    path = 'src/api/rest.rs'
    with open(path, 'r') as f:
        content = f.read()

    # Add a test that covers the Err branch of event feed
    # By using a router that has no DB (or an invalid one)
    # But test_router uses Storage::for_tests() which might be enough if we can force a failure.

    # Let's add a test for verify_state which is also in src/api/rest.rs but maybe not in scope?
    # No, SCOPED_LINE_RANGES for rest.rs are:
    # (133, 146), (230, 270), (432, 457), (458, 500), ...

    # Lines 458-500 is get_event_feed.
    # Current aggregate is 193/213 (90.61%). We need about 10 more lines.

    new_tests = """
    #[tokio::test]
    async fn test_get_mmr_proof_missing_all_params() {
        let app = test_router();
        let response = app.oneshot(Request::builder().uri("/v1/mmr-proof").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
"""
    # Just adding more tests to mod tests
    content = content.rstrip()
    if content.endswith('}'):
        content = content[:-1] + new_tests + '}\n'

    with open(path, 'w') as f:
        f.write(content)

improve_coverage()
