use herdr_white_noise_bridge::herdr::Agent;
use serde_json::json;
use std::{
    fs,
    process::{Command, Stdio},
};

#[test]
fn group_name_uses_the_repository_root_instead_of_the_chat_topic_or_subdirectory() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("codr");
    let nested = repo.join("src");
    fs::create_dir_all(&nested).unwrap();
    assert!(
        Command::new("git")
            .arg("init")
            .arg(&repo)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    let a: Agent = serde_json::from_value(json!({
        "pane_id":"w1:p1", "terminal_id":"term1", "agent":"opencode", "agent_status":"idle",
        "cwd":dir.path().join("other-project"), "foreground_cwd":nested,
        "terminal_title_stripped":"OC | Initialise remote messaging"
    }))
    .unwrap();
    assert_eq!(a.group_name(), "HDR | codr");
}

#[test]
fn group_name_falls_back_to_the_working_directory_without_git() {
    let a: Agent = serde_json::from_value(json!({
        "pane_id":"w1:p1", "terminal_id":"term1", "agent":"opencode", "agent_status":"idle",
        "cwd":"/nonexistent/path/weather", "terminal_title_stripped":"OC | Forecast redesign"
    }))
    .unwrap();
    assert_eq!(a.group_name(), "HDR | weather");
}
