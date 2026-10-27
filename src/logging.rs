//! Rate-limited stderr lines for conditions that recur on every poll.
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

const REPEAT_AFTER: Duration = Duration::from_secs(600);
/// Bound on distinct remembered lines, so varying text cannot grow the table without limit.
const MAX_LINES: usize = 256;

struct Seen {
    printed: Instant,
    suppressed: u64,
}

static SEEN: Mutex<Option<HashMap<String, Seen>>> = Mutex::new(None);

/// Print `line` unless the same line was printed in the last ten minutes.
/// The next print after that reports how many repeats were left out.
pub fn throttled(line: String) {
    if let Some(text) = admit(&line, Instant::now()) {
        eprintln!("{text}");
    }
}

fn admit(line: &str, at: Instant) -> Option<String> {
    let mut guard = SEEN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let seen = guard.get_or_insert_with(HashMap::new);
    if seen.len() >= MAX_LINES && !seen.contains_key(line) {
        seen.retain(|_, entry| at.duration_since(entry.printed) < REPEAT_AFTER);
        if seen.len() >= MAX_LINES {
            return None;
        }
    }
    match seen.get_mut(line) {
        Some(entry) if at.duration_since(entry.printed) < REPEAT_AFTER => {
            entry.suppressed += 1;
            None
        }
        Some(entry) => {
            let text = if entry.suppressed == 0 {
                line.to_string()
            } else {
                format!("{line} (repeated {} more times)", entry.suppressed)
            };
            *entry = Seen {
                printed: at,
                suppressed: 0,
            };
            Some(text)
        }
        None => {
            seen.insert(
                line.to_string(),
                Seen {
                    printed: at,
                    suppressed: 0,
                },
            );
            Some(line.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeated_line_prints_once_per_window_with_a_count() {
        let start = Instant::now();
        let line = "unit-test line";
        assert_eq!(admit(line, start).as_deref(), Some(line));
        assert_eq!(admit(line, start + Duration::from_secs(1)), None);
        assert_eq!(admit(line, start + Duration::from_secs(2)), None);
        assert_eq!(
            admit(line, start + REPEAT_AFTER + Duration::from_secs(1)).as_deref(),
            Some("unit-test line (repeated 2 more times)")
        );
        assert_eq!(admit("another line", start).as_deref(), Some("another line"));
    }
}
