use herdr_white_noise_bridge::output::claude;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";

fn user(id: &str, parent: Option<&str>, content: Value) -> Value {
    json!({"type":"user","uuid":id,"parentUuid":parent,"sessionId":SESSION,"message":{"role":"user","content":content}})
}
fn assistant(id: &str, parent: &str, message: &str, reason: Value, content: Value) -> Value {
    json!({"type":"assistant","uuid":id,"parentUuid":parent,"sessionId":SESSION,"message":{"id":message,"role":"assistant","stop_reason":reason,"content":content}})
}
fn write_transcript(home: &Path, project: &str, rows: &[Value]) -> PathBuf {
    let directory = home.join("projects").join(project);
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("{SESSION}.jsonl"));
    let text = rows
        .iter()
        .map(|row| serde_json::to_string(row).unwrap() + "\n")
        .collect::<String>();
    fs::write(&path, text).unwrap();
    path
}

#[test]
fn completed_main_reply_combines_only_its_own_text_blocks() {
    let home = tempfile::tempdir().unwrap();
    write_transcript(
        home.path(),
        "project",
        &[
            user("u", None, json!("A prompt")),
            assistant(
                "tool",
                "u",
                "api-1",
                json!("tool_use"),
                json!([{"type":"text","text":"Not the answer"},{"type":"tool_use","input":{"secret":"tool input"}}]),
            ),
            user(
                "result",
                Some("tool"),
                json!([{"type":"tool_result","content":"Do not forward tool output"}]),
            ),
            assistant(
                "a1",
                "result",
                "api-2",
                Value::Null,
                json!([{"type":"thinking","thinking":"private reasoning"},{"type":"text","text":"Hello"}]),
            ),
            assistant(
                "a2",
                "a1",
                "api-2",
                json!("end_turn"),
                json!([{"type":"text","text":"world"}]),
            ),
            json!({"type":"system","subtype":"turn_duration","uuid":"s","parentUuid":"a2","sessionId":SESSION}),
        ],
    );
    let replies = claude::read(home.path(), SESSION).unwrap();
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].text, "Hello\nworld");
    assert_eq!(replies[0].id, "claude:a2");
}

#[test]
fn abandoned_branches_and_subagents_are_not_forwarded() {
    let home = tempfile::tempdir().unwrap();
    let mut side = assistant(
        "side",
        "old",
        "api-side",
        json!("end_turn"),
        json!([{"type":"text","text":"Subagent answer"}]),
    );
    side["isSidechain"] = json!(true);
    write_transcript(
        home.path(),
        "project",
        &[
            user("u", None, json!("Prompt")),
            assistant(
                "old",
                "u",
                "api-old",
                json!("end_turn"),
                json!([{"type":"text","text":"Abandoned answer"}]),
            ),
            assistant(
                "selected",
                "u",
                "api-new",
                json!("end_turn"),
                json!([{"type":"text","text":"Selected answer"}]),
            ),
            json!({"type":"progress","uuid":"progress","parentUuid":"selected","sessionId":SESSION}),
            side,
        ],
    );
    let replies = claude::read(home.path(), SESSION).unwrap();
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].text, "Selected answer");
}

#[test]
fn private_descendants_cannot_hide_the_latest_main_reply() {
    for (flag, value) in [
        ("isSidechain", json!(true)),
        ("isMeta", json!(true)),
        ("teamName", json!("team")),
    ] {
        let home = tempfile::tempdir().unwrap();
        let mut hidden = assistant(
            "hidden",
            "main",
            "private-api",
            json!("end_turn"),
            json!([{"type":"text","text":"Do not forward"}]),
        );
        hidden[flag] = value;
        write_transcript(
            home.path(),
            "project",
            &[
                user("u", None, json!("Prompt")),
                assistant(
                    "main",
                    "u",
                    "main-api",
                    json!("end_turn"),
                    json!([{"type":"text","text":"Main answer"}]),
                ),
                hidden,
            ],
        );
        let replies = claude::read(home.path(), SESSION).unwrap();
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].text, "Main answer");
    }
}

#[test]
fn null_stop_reason_is_not_made_final_by_a_duration_marker_and_can_be_backfilled() {
    let home = tempfile::tempdir().unwrap();
    let pending = assistant(
        "a",
        "u",
        "api-1",
        Value::Null,
        json!([{"type":"text","text":"Pending"}]),
    );
    let path = write_transcript(
        home.path(),
        "project",
        &[
            user("u", None, json!("Prompt")),
            pending.clone(),
            json!({"type":"system","subtype":"turn_duration","uuid":"s","parentUuid":"a","sessionId":SESSION}),
        ],
    );
    assert!(claude::read(home.path(), SESSION).unwrap().is_empty());
    let mut complete = pending;
    complete["message"]["stop_reason"] = json!("end_turn");
    use std::io::Write;
    let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
    writeln!(file, "{}", serde_json::to_string(&complete).unwrap()).unwrap();
    file.write_all(b"{\"unfinished\":").unwrap();
    let replies = claude::read(home.path(), SESSION).unwrap();
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].id, "claude:a");
}

#[test]
fn unknown_session_ambiguous_files_and_nested_subagents_are_rejected() {
    let home = tempfile::tempdir().unwrap();
    let row = assistant(
        "a",
        "u",
        "api",
        json!("end_turn"),
        json!([{"type":"text","text":"Answer"}]),
    );
    write_transcript(
        home.path(),
        "project",
        &[user("u", None, json!("Prompt")), row.clone()],
    );
    let nested = home
        .path()
        .join("projects/project")
        .join(SESSION)
        .join("subagents");
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        nested.join(format!("{SESSION}.jsonl")),
        serde_json::to_string(&row).unwrap() + "\n",
    )
    .unwrap();
    assert_eq!(claude::read(home.path(), SESSION).unwrap().len(), 1);
    write_transcript(home.path(), "other-project", &[row]);
    assert!(claude::read(home.path(), SESSION).is_err());
    assert!(claude::read(home.path(), "../../credentials").is_err());
}
