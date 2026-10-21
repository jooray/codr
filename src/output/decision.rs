use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use unicode_width::UnicodeWidthStr;

fn custom_default() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct OptionItem {
    pub label: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Question {
    pub question: String,
    #[serde(default)]
    pub header: String,
    pub options: Vec<OptionItem>,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default = "custom_default")]
    pub custom: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Decision {
    pub id: String,
    pub questions: Vec<Question>,
    #[serde(default)]
    pub announced: bool,
    #[serde(default)]
    pub submitted: bool,
    #[serde(default)]
    pub requires_token: bool,
}

impl Decision {
    pub fn same_request(&self, other: &Self) -> bool {
        self.id == other.id && self.questions == other.questions
    }

    pub fn token(&self) -> String {
        let tail: String = self.id.chars().rev().take(6).collect();
        format!("q{}", tail.chars().rev().collect::<String>().to_lowercase())
    }

    pub fn supported(&self) -> bool {
        self.questions.len() == 1
            && !self.questions[0].multiple
            && (1..=9).contains(&self.questions[0].options.len())
    }

    pub fn text(&self) -> String {
        let mut text = format!("Decision needed [{}]", self.token());
        for (index, question) in self.questions.iter().enumerate() {
            if self.questions.len() > 1 {
                text.push_str(&format!("\n\nQuestion {}: {}", index + 1, question.header));
            } else if !question.header.is_empty() {
                text.push_str(&format!(": {}", question.header));
            }
            text.push_str(&format!("\n\n{}", question.question));
            for (index, option) in question.options.iter().enumerate() {
                text.push_str(&format!("\n\n{}. {}", index + 1, option.label));
                if !option.description.is_empty() {
                    text.push_str(&format!("\n{}", option.description));
                }
            }
        }
        if self.submitted {
            text.push_str("\n\nAn answer was already submitted. Check the terminal if this question is still waiting.");
        } else if !self.supported() {
            text.push_str("\n\nAnswer this multi-question or multi-select dialog in the terminal.");
        } else if self.requires_token {
            text.push_str(&format!(
                "\n\nThe question changed. Reply with {} and your choice, for example: {} 1",
                self.token(),
                self.token()
            ));
        } else {
            text.push_str("\n\nReply with the option number or its label.");
            if self.questions[0].custom {
                text.push_str(" Use the terminal for a custom answer.");
            }
        }
        text
    }

    pub fn choice(&self, text: &str) -> Result<usize> {
        ensure!(
            self.supported(),
            "This dialog needs an answer in the terminal."
        );
        let text = text.trim();
        let text = text.strip_prefix("/answer ").unwrap_or(text).trim();
        let mut words = text.splitn(2, char::is_whitespace);
        let first = words.next().unwrap_or("").trim_end_matches(':');
        let token_present = first.eq_ignore_ascii_case(&self.token());
        let answer = if token_present {
            words.next().unwrap_or("").trim()
        } else {
            text
        };
        ensure!(
            !self.requires_token || token_present,
            "The question changed. Include its question code with your answer."
        );
        if !answer.is_empty()
            && answer.bytes().all(|byte| byte.is_ascii_digit())
            && let Ok(index) = answer.parse::<usize>()
            && (1..=self.questions[0].options.len()).contains(&index)
        {
            return Ok(index);
        }
        let matches: Vec<_> = self.questions[0]
            .options
            .iter()
            .enumerate()
            .filter(|(_, option)| option.label.to_lowercase() == answer.to_lowercase())
            .collect();
        if matches.len() == 1 {
            return Ok(matches[0].0 + 1);
        }
        bail!(
            "Reply with a listed option number or its label. Use the terminal for a custom answer."
        )
    }

    pub fn looks_like_answer(&self, text: &str) -> bool {
        let text = text.trim();
        let first = text
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches(':');
        let coded = first.len() == 7
            && first.as_bytes()[0].eq_ignore_ascii_case(&b'q')
            && first.as_bytes()[1..]
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
        (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
            || text.starts_with("/answer ")
            || first.eq_ignore_ascii_case(&self.token())
            || coded
            || self
                .questions
                .iter()
                .flat_map(|question| &question.options)
                .any(|option| option.label.to_lowercase() == text.to_lowercase())
    }

    pub fn matches_visible(&self, visible: &str) -> bool {
        if !self.supported() {
            return false;
        }
        let question = &self.questions[0];
        let mut expected = question.question.clone();
        for (index, option) in question.options.iter().enumerate() {
            expected.push_str(&format!(
                "{}.{}{}",
                index + 1,
                option.label,
                option.description
            ));
        }
        if question.custom {
            expected.push_str(&format!(
                "{}.Type your own answer",
                question.options.len() + 1
            ));
        }
        expected.push_str("↑↓ select enter submit esc dismiss");
        compact(&main_column(visible)).ends_with(&compact(&expected))
    }
}

fn main_column(visible: &str) -> String {
    // OpenCode's default wide sidebar has two padding cells before its version footer.
    // Slice by display cells, not UTF-8 offsets, so wrapped Unicode options still match.
    let boundary = visible.lines().rev().find_map(|line| {
        let prefix = &line[..line.rfind("• OpenCode ")?];
        if !prefix.ends_with("  ") {
            return None;
        }
        UnicodeWidthStr::width(prefix)
            .checked_sub(2)
            .filter(|column| *column >= 20)
    });
    let Some(boundary) = boundary else {
        return visible.into();
    };
    visible
        .lines()
        .map(|line| {
            if UnicodeWidthStr::width(line) <= boundary {
                return line.to_string();
            }
            // Whole-prefix widths preserve joined emoji while trimming the small sidebar.
            let end = line
                .char_indices()
                .rev()
                .find_map(|(end, _)| {
                    (UnicodeWidthStr::width(&line[..end]) <= boundary).then_some(end)
                })
                .unwrap_or(0);
            line[..end].to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control() && !matches!(c, '│' | '┃' | '║' | '|'))
        .collect()
}

pub fn from_export(value: &Value) -> Result<Option<Decision>> {
    from_data(serde_json::from_value(value.clone())?)
}

fn from_data(export: super::Export) -> Result<Option<Decision>> {
    let latest_user = export
        .messages
        .iter()
        .rposition(|message| message.info.role == "user");
    let mut found = None;
    for (index, message) in export.messages.into_iter().enumerate() {
        if latest_user.is_some_and(|user| index < user)
            || message.info.role != "assistant"
            || message.info.error.is_some()
            || message.info.summary == true
        {
            continue;
        }
        for part in message.parts {
            if part.kind != "tool"
                || part.tool.as_deref() != Some("question")
                || part.state["status"] != "running"
            {
                continue;
            }
            let questions: Vec<Question> =
                serde_json::from_value(part.state["input"]["questions"].clone())?;
            ensure!(
                !questions.is_empty() && questions.len() <= 8,
                "Invalid pending question count"
            );
            let mut size = 0;
            for question in &questions {
                ensure!(
                    !question.question.trim().is_empty()
                        && question.question.len() <= 4000
                        && question.header.len() <= 200
                        && !question.options.is_empty()
                        && question.options.len() <= 16,
                    "Invalid pending question"
                );
                size += question.question.len() + question.header.len();
                for option in &question.options {
                    ensure!(
                        !option.label.trim().is_empty()
                            && option.label.len() <= 200
                            && option.description.len() <= 1000,
                        "Invalid question option"
                    );
                    size += option.label.len() + option.description.len();
                }
            }
            ensure!(size <= 12000, "Pending question is too large");
            let id = part
                .id
                .filter(|id| !id.is_empty())
                .ok_or_else(|| anyhow::anyhow!("Pending question has no identity"))?;
            ensure!(found.is_none(), "Multiple pending questions are ambiguous");
            found = Some(Decision {
                id: format!("{}:{id}", message.info.id),
                questions,
                announced: false,
                submitted: false,
                requires_token: false,
            });
        }
    }
    Ok(found)
}

pub async fn read(
    config: &crate::BridgeConfig,
    session: &crate::herdr::Session,
) -> Result<Option<Decision>> {
    if session.agent != "opencode" || session.kind != "id" {
        return Ok(None);
    }
    from_data(super::export_data(&config.opencode_bin, &session.value).await?)
}
