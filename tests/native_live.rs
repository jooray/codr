use herdr_white_noise_bridge::output::codex;
use std::path::PathBuf;

#[test]
#[ignore = "requires a local Codex rollout and CODR_TEST_CODEX_SESSION"]
fn installed_codex_transcript_yields_completed_replies_without_model_calls() {
    let session = std::env::var("CODR_TEST_CODEX_SESSION").expect("set CODR_TEST_CODEX_SESSION");
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".codex"));
    let replies = codex::read(&home, &session).unwrap();
    assert!(!replies.is_empty());
    println!(
        "Read {} completed Codex replies; transcript text omitted",
        replies.len()
    );
}

#[test]
#[ignore = "requires a Claude transcript and CODR_TEST_CLAUDE_SESSION"]
fn installed_claude_transcript_yields_completed_replies_without_model_calls() {
    let session = std::env::var("CODR_TEST_CLAUDE_SESSION").expect("set CODR_TEST_CLAUDE_SESSION");
    let home = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".claude"));
    let replies = herdr_white_noise_bridge::output::claude::read(&home, &session).unwrap();
    assert!(!replies.is_empty());
    println!(
        "Read {} completed Claude replies; transcript text omitted",
        replies.len()
    );
}
