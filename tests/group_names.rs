use herdr_white_noise_bridge::{
    herdr::{self, Agent},
    wire::{Socket, read_line},
};
use serde_json::json;
use tokio::{
    io::{AsyncWriteExt, BufReader},
    net::UnixListener,
};

#[test]
fn group_name_uses_the_herdr_tab_label_instead_of_repository_or_topic() {
    let agent: Agent = serde_json::from_value(json!({
        "pane_id":"w1:p1", "terminal_id":"term1", "agent":"opencode", "agent_status":"idle",
        "tab_id":"w1:t1", "tab_label":"review", "cwd":"/repos/example-project",
        "foreground_cwd":"/repos/other-project", "terminal_title_stripped":"OC | A chat topic"
    }))
    .unwrap();
    assert_eq!(agent.group_name(), "HDR | review");
}

#[test]
fn old_agent_records_without_labels_fall_back_to_the_tab_id() {
    let agent: Agent = serde_json::from_value(json!({
        "pane_id":"w1:p1", "terminal_id":"term1", "agent":"opencode", "agent_status":"idle",
        "tab_id":"w1:t2", "cwd":"/repos/weather"
    }))
    .unwrap();
    assert_eq!(agent.group_name(), "HDR | w1:t2");
}

#[test]
fn tab_names_are_control_free_and_fit_the_mdk_utf8_byte_limit() {
    let agent: Agent = serde_json::from_value(json!({
        "pane_id":"w1:p1", "terminal_id":"term1", "agent":"opencode", "agent_status":"idle",
        "tab_label":format!("  🐇{}\n\t  ", "🐇".repeat(100))
    }))
    .unwrap();
    let name = agent.group_name();
    assert!(name.starts_with("HDR | 🐇"));
    assert!(name.len() <= 256);
    assert!(!name.chars().any(char::is_control));
}

#[tokio::test]
async fn snapshot_joins_each_agent_with_its_own_tab_label() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("herdr.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(stream);
        let request = read_line(&mut reader).await.unwrap();
        let response = json!({"id":request["id"],"result":{"snapshot":{
            "agents":[{"pane_id":"w1:p2","terminal_id":"term2","agent":"opencode","agent_status":"idle","tab_id":"w1:t2","cwd":"/repos/ignored"}],
            "tabs":[{"tab_id":"w1:t1","label":"Other tab","number":1},{"tab_id":"w1:t2","label":"Build","number":2}]
        }}});
        let mut bytes = serde_json::to_vec(&response).unwrap();
        bytes.push(b'\n');
        reader.get_mut().write_all(&bytes).await.unwrap();
    });
    let agents = herdr::snapshot(&Socket { path }).await.unwrap();
    assert_eq!(agents[0].group_name(), "HDR | Build");
    server.await.unwrap();
}
