use conxian_nexus::executor::canonical_bitvm::{
    StacksBitcoinHeightProvider, TrustedBitcoinHeightProvider,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn height_from_response(status: &str, body: &str) -> anyhow::Result<u64> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let size = stream.read(&mut request).await.unwrap();
        assert!(std::str::from_utf8(&request[..size])
            .unwrap()
            .starts_with("GET /v2/info "));
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let result = StacksBitcoinHeightProvider::new(format!("http://{address}/"))
        .current_height()
        .await;
    server.await?;
    Ok(result?)
}

#[tokio::test]
async fn uses_bitcoin_height_even_when_stacks_tip_is_millions_of_blocks_higher() {
    let height = height_from_response(
        "200 OK",
        r#"{"stacks_tip_height":9404337,"burn_block_height":973055}"#,
    )
    .await
    .unwrap();
    assert_eq!(height, 973055);
}

#[tokio::test]
async fn unavailable_malformed_and_zero_height_sources_cannot_clear_safety() {
    for (status, body) in [
        ("503 Service Unavailable", r#"{"burn_block_height":973055}"#),
        ("200 OK", r#"{"stacks_tip_height":9404337}"#),
        ("200 OK", r#"{"burn_block_height":0}"#),
        ("200 OK", r#"{"burn_block_height":-1}"#),
        ("200 OK", r#"{"burn_block_height":"973055"}"#),
        ("200 OK", "not-json"),
    ] {
        assert!(height_from_response(status, body).await.is_err());
    }
}
