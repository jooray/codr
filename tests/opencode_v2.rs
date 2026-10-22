use herdr_white_noise_bridge::output::final_messages_v2;
use serde_json::json;

#[test]
fn v2_export_keeps_only_completed_replies_from_stopped_turns() {
    let export = json!({
        "info": {"id": "ses_fixture"},
        "messages": [
            {"type":"user","id":"msg_u1","text":"first","time":{"created":1},"agents":[],"files":[]},
            {"type":"assistant","id":"msg_a1","agent":"build","finish":"tool-calls",
             "time":{"created":2,"completed":3},
             "content":[{"type":"text","text":"intermediate"},
                        {"type":"tool","id":"tool_1","name":"shell","state":{"status":"completed","input":{}}}]},
            {"type":"assistant","id":"msg_a2","agent":"build","finish":"stop",
             "time":{"created":4,"completed":5},
             "content":[{"type":"reasoning","text":"thinking"},{"type":"text","text":"First answer"}]},
            {"type":"synthetic","id":"msg_s1","text":"<subagent/>","description":"child",
             "metadata":{"childID":"ses_child"},"time":{"created":6}},
            {"type":"idle","id":"msg_i1","outcome":"succeeded","time":{"created":7}},
            {"type":"user","id":"msg_u2","text":"second","time":{"created":8},"agents":[],"files":[]},
            {"type":"assistant","id":"msg_a3","agent":"build","finish":"stop",
             "time":{"created":9,"completed":10},
             "content":[{"type":"text","text":"Second"},{"type":"text","text":"answer"}]},
            {"type":"assistant","id":"msg_a4","agent":"build","finish":"stop",
             "time":{"created":11},"content":[{"type":"text","text":"unfinished"}]},
            {"type":"assistant","id":"msg_a5","agent":"build","finish":"error",
             "time":{"created":12,"completed":13},"error":{"message":"failed"},
             "content":[{"type":"text","text":"failed turn"}]}
        ]
    });
    let finals = final_messages_v2(&export).unwrap();
    assert_eq!(finals.len(), 2);
    assert_eq!(finals[0].text, "First answer");
    assert_eq!(finals[0].turn.as_deref(), Some("opencode:msg_u1"));
    assert_eq!(finals[1].text, "Second\nanswer");
    assert_eq!(finals[1].turn.as_deref(), Some("opencode:msg_u2"));
}

#[test]
fn v2_export_without_conversation_messages_has_no_finals() {
    let export = json!({
        "info": {"id": "ses_empty"},
        "messages": [
            {"type":"synthetic","id":"msg_s1","text":"<subagent/>","metadata":{},"time":{"created":1}},
            {"type":"idle","id":"msg_i1","outcome":"succeeded","time":{"created":2}}
        ]
    });
    assert!(final_messages_v2(&export).unwrap().is_empty());
}
