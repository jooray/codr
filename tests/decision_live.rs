use herdr_white_noise_bridge::{BridgeConfig, herdr::Session, output};

#[tokio::test]
#[ignore = "requires CODR_TEST_CONFIG and CODR_TEST_DECISION_SESSION with a pending OpenCode question; read-only"]
async fn installed_pending_question_is_available_without_answering_it() {
    let config = BridgeConfig::load(std::path::Path::new(
        &std::env::var("CODR_TEST_CONFIG").unwrap(),
    ))
    .unwrap();
    let session = Session {
        source: "herdr:opencode".into(),
        agent: "opencode".into(),
        kind: "id".into(),
        value: std::env::var("CODR_TEST_DECISION_SESSION").unwrap(),
    };
    let decision = output::decision::read(&config, &session)
        .await
        .unwrap()
        .expect("Expected a pending live question");
    assert!(decision.supported());
    assert!(!decision.submitted);
}
