use herdr_white_noise_bridge::output::decision::{self, Decision};
use serde_json::{Value, json};

fn data(status: &str) -> Value {
    json!({"messages":[
        {"info":{"id":"user","role":"user"},"parts":[]},
        {"info":{"id":"assistant","parentID":"user","role":"assistant"},"parts":[
            {"type":"text","text":"PRIVATE REASONING"},
            {"type":"tool","id":"part-a12345","tool":"question","state":{"status":status,"input":{"questions":[{
                "header":"Scope","question":"Which next step?","options":[
                    {"label":"Proceed","description":"Continue with the plan"},
                    {"label":"Review","description":"Review the alternatives"}
                ]
            }]},"output":"PRIVATE TOOL OUTPUT"}}
        ]}
    ]})
}

fn pending() -> Decision {
    decision::from_export(&data("running")).unwrap().unwrap()
}

#[test]
fn only_running_questions_in_the_current_turn_are_selected() {
    let decision = pending();
    assert!(decision.supported());
    assert!(decision.text().contains("1. Proceed"));
    assert!(!decision.text().contains("PRIVATE"));
    assert!(decision::from_export(&data("completed")).unwrap().is_none());
    let mut value = data("running");
    value["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"info":{"id":"new-user","role":"user"},"parts":[]}));
    assert!(decision::from_export(&value).unwrap().is_none());
    let mut value = data("running");
    let duplicate = value["messages"][1]["parts"][1].clone();
    value["messages"][1]["parts"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(decision::from_export(&value).is_err());
}

#[test]
fn a_choice_is_a_listed_number_or_exact_label_not_arbitrary_keys() {
    let decision = pending();
    assert_eq!(decision.choice("1").unwrap(), 1);
    assert_eq!(decision.choice("review").unwrap(), 2);
    assert_eq!(decision.choice("/answer 2").unwrap(), 2);
    for invalid in ["0", "3", "9", "1\nenter", "ctrl+c", "something else", ""] {
        assert!(decision.choice(invalid).is_err(), "{invalid}");
    }
    let mut duplicate = decision.clone();
    duplicate.questions[0].options[1].label = "Proceed".into();
    assert!(duplicate.choice("Proceed").is_err());
    assert_eq!(duplicate.choice("2").unwrap(), 2);
}

#[test]
fn changed_questions_require_their_code() {
    let mut decision = pending();
    decision.requires_token = true;
    assert!(decision.choice("1").is_err());
    assert_eq!(
        decision.choice(&format!("{} 1", decision.token())).unwrap(),
        1
    );
    assert!(decision.choice("qwrong 1").is_err());
    assert!(
        decision.looks_like_answer(" Qold001 1"),
        "An expired question code is still an answer attempt, not a new prompt"
    );
    assert!(!decision.looks_like_answer("éabcde 1"));
}

#[test]
fn only_the_exact_visible_single_choice_dialog_can_receive_a_shortcut() {
    let decision = pending();
    let visible = "Unrelated old output\n│ Which next step?\n│ 1. Proceed\n│ Continue with the plan\n│ 2. Review\n│ Review the alternatives\n│ 3. Type your own answer\n↑↓ select enter submit esc dismiss\n";
    assert!(decision.matches_visible(visible));
    assert!(
        !decision.matches_visible(&visible.replace("Which next step?", "A different question?"))
    );
    assert!(!decision.matches_visible(&visible.replace(
        "↑↓ select enter submit esc dismiss",
        "Allow permission? enter confirm"
    )));
    assert!(
        !decision
            .matches_visible(&visible.replace("↑↓ select", "Custom text being edited\n↑↓ select"))
    );
    let mut multiple = decision;
    multiple.questions[0].multiple = true;
    assert!(!multiple.supported());
    assert!(multiple.choice("1").is_err());
    assert!(!multiple.matches_visible(visible));
}

#[test]
fn a_wide_opencode_sidebar_does_not_hide_the_active_question() {
    let decision = pending();
    let lines = [
        "Unrelated output",
        "Which next step?",
        "1. Proceed",
        "Continue with the plan",
        "2. Review",
        "Review the alternatives",
        "3. Type your own answer",
        "↑↓ select enter submit esc dismiss",
    ];
    let visible = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            format!(
                "{line:<80}  {}",
                if index == lines.len() - 1 {
                    "• OpenCode 1.18.35"
                } else {
                    "sidebar content"
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(decision.matches_visible(&visible));
    assert!(
        !decision.matches_visible(&visible.replace("Which next step?", "A different question?"))
    );
}

#[test]
fn joined_emoji_keep_their_display_width_when_the_sidebar_is_removed() {
    let mut decision = pending();
    decision.questions[0].question = format!("{}?", "👩‍💻".repeat(20));
    let lines = [
        decision.questions[0].question.as_str(),
        "1. Proceed",
        "Continue with the plan",
        "2. Review",
        "Review the alternatives",
        "3. Type your own answer",
        "↑↓ select enter submit esc dismiss",
    ];
    let visible = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            format!(
                "{}{}  {}",
                line,
                " ".repeat(80 - unicode_width::UnicodeWidthStr::width(*line)),
                if index == lines.len() - 1 {
                    "• OpenCode 1.18.35"
                } else {
                    "sidebar"
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(decision.matches_visible(&visible));
}
