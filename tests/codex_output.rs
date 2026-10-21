use herdr_white_noise_bridge::output::codex;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";

fn write_rollout(home: &Path, rows: &[Value], archived: bool) -> PathBuf {
    let directory = home.join(if archived {
        "archived_sessions"
    } else {
        "sessions/2026/01/01"
    });
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("rollout-2026-01-01T00-00-00-{SESSION}.jsonl"));
    let mut data =
        serde_json::to_string(&json!({"type":"session_meta","payload":{"id":SESSION}})).unwrap();
    data.push('\n');
    for row in rows {
        data.push_str(&serde_json::to_string(row).unwrap());
        data.push('\n');
    }
    fs::write(&path, data).unwrap();
    path
}

#[test]
fn only_successful_completed_codex_turns_are_forwarded_once() {
    let home = tempfile::tempdir().unwrap();
    let final_row = json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":"Final answer"}});
    write_rollout(
        home.path(),
        &[
            json!({"type":"response_item","payload":{"type":"reasoning","content":"Do not forward reasoning"}}),
            json!({"type":"response_item","payload":{"type":"message","role":"assistant","phase":"commentary","content":[{"type":"output_text","text":"Working commentary"}]}}),
            json!({"type":"event_msg","payload":{"type":"agent_message","phase":"final_answer","message":"Not completed yet"}}),
            final_row.clone(),
            final_row,
            json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"failed","last_agent_message":"Not a successful answer","error":{"message":"failure"}}}),
            json!({"type":"event_msg","payload":{"type":"turn_complete","turn_id":"turn-2","last_agent_message":"Another final"}}),
        ],
        false,
    );
    let replies = codex::read(home.path(), SESSION).unwrap();
    assert_eq!(
        replies.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
        vec!["Final answer", "Another final"]
    );
    assert_eq!(replies[0].id, "codex:turn-1");
}

#[test]
fn partial_tail_is_ignored_and_archived_rollouts_remain_readable() {
    let home = tempfile::tempdir().unwrap();
    let path = write_rollout(
        home.path(),
        &[
            json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":"Completed"}}),
        ],
        true,
    );
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(b"{\"type\":\"event_msg\"")
        .unwrap();
    assert_eq!(
        codex::read(home.path(), SESSION).unwrap()[0].text,
        "Completed"
    );
}

#[test]
fn ambiguous_or_wrong_session_files_and_path_traversal_are_rejected() {
    let home = tempfile::tempdir().unwrap();
    let path = write_rollout(home.path(), &[], false);
    fs::write(&path, "{\"type\":\"session_meta\",\"payload\":{\"id\":\"22222222-2222-4222-8222-222222222222\"}}\n").unwrap();
    assert!(codex::read(home.path(), SESSION).is_err());
    write_rollout(home.path(), &[], false);
    write_rollout(home.path(), &[], true);
    assert!(codex::read(home.path(), SESSION).is_err());
    assert!(codex::read(home.path(), "../../credentials").is_err());
}
