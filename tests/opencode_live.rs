#[tokio::test]
#[ignore = "requires a local OpenCode installation and OPENCODE_TEST_SESSION"]
async fn installed_opencode_export_extracts_completed_replies_without_model_calls() {
    let session = std::env::var("OPENCODE_TEST_SESSION").expect("set OPENCODE_TEST_SESSION");
    let bin = std::env::var("OPENCODE_TEST_BIN").unwrap_or_else(|_| "opencode".into());
    let finals = herdr_white_noise_bridge::output::export(std::path::Path::new(&bin), &session)
        .await
        .unwrap();
    assert!(!finals.is_empty());
    println!(
        "Extracted {} completed final replies; transcript text omitted",
        finals.len()
    );
}
