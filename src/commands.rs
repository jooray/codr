//! Command spelling: exact names, unambiguous prefixes, and bare words in the control chat.

pub(crate) enum Normalized {
    Text(String),
    Ambiguous(Vec<&'static str>),
}

const COMMANDS: &[&str] = &[
    "agent", "allow", "answer", "cancel", "help", "model", "new", "queue", "recent", "reject",
    "status", "sync", "unmute",
];

pub(crate) fn normalize(text: &str, control: bool, menu_open: bool) -> Normalized {
    let trimmed = text.trim();
    let body = match trimmed.strip_prefix('/') {
        Some(body) => body,
        None if control && !menu_open => trimmed,
        None => return Normalized::Text(text.to_owned()),
    };
    let first = body.split_whitespace().next().unwrap_or("");
    if first.is_empty() {
        return Normalized::Text(text.to_owned());
    }
    let lower = first.to_ascii_lowercase();
    let matches: Vec<&'static str> =
        if let Some(exact) = COMMANDS.iter().copied().find(|command| *command == lower) {
            vec![exact]
        } else {
            COMMANDS
                .iter()
                .copied()
                .filter(|command| command.starts_with(&lower))
                .collect()
        };
    match matches.as_slice() {
        [command] => {
            let rest = body.strip_prefix(first).unwrap_or("").trim_start();
            Normalized::Text(if rest.is_empty() {
                format!("/{command}")
            } else {
                format!("/{command} {rest}")
            })
        }
        [] => Normalized::Text(text.to_owned()),
        _ => Normalized::Ambiguous(matches),
    }
}

#[cfg(test)]
mod tests {
    use super::{Normalized, normalize};

    fn text(value: &str, control: bool, menu_open: bool) -> String {
        match normalize(value, control, menu_open) {
            Normalized::Text(text) => text,
            Normalized::Ambiguous(options) => panic!("unexpected ambiguity: {options:?}"),
        }
    }

    #[test]
    fn exact_and_prefixed_commands_share_one_spelling() {
        assert_eq!(text("/new", false, false), "/new");
        assert_eq!(text("/n", false, false), "/new");
        assert_eq!(text("/ne", false, false), "/new");
        assert_eq!(text("/st", false, false), "/status");
        assert_eq!(text("/sy on", false, false), "/sync on");
        assert_eq!(text("/c", false, false), "/cancel");
        assert_eq!(text("/h", false, false), "/help");
        assert_eq!(text("/NEW", false, false), "/new");
    }

    #[test]
    fn bare_words_are_commands_only_in_the_control_chat() {
        assert_eq!(text("new", true, false), "/new");
        assert_eq!(text("n", true, false), "/new");
        assert_eq!(text("status", true, false), "/status");
        assert_eq!(text("new", false, false), "new");
        assert_eq!(text("n", false, false), "n");
        assert_eq!(text("hello", true, false), "hello");
    }

    #[test]
    fn bare_words_stay_menu_input_while_a_launch_menu_is_open() {
        assert_eq!(text("new", true, true), "new");
        assert_eq!(text("n", true, true), "n");
        assert_eq!(text("/n", true, true), "/new");
    }

    #[test]
    fn ambiguous_prefixes_are_reported_and_unknown_text_is_untouched() {
        match normalize("/s", true, false) {
            Normalized::Ambiguous(options) => assert_eq!(options, vec!["status", "sync"]),
            _ => panic!("expected ambiguous /s"),
        }
        match normalize("r", true, false) {
            Normalized::Ambiguous(options) => assert_eq!(options, vec!["recent", "reject"]),
            _ => panic!("expected ambiguous r"),
        }
        assert_eq!(text("/clear-queue", false, false), "/clear-queue");
        assert_eq!(text("/nonsense", false, false), "/nonsense");
    }
}
