use herdr_white_noise_bridge::output;
use std::os::unix::fs::PermissionsExt;

/// A fake CLI that ends its document early on the first `truncated` invocations.
fn export_script(dir: &std::path::Path, truncated: usize) -> std::path::PathBuf {
    let path = dir.join("opencode");
    let body = format!(
        r#"#!/bin/sh
count="$(dirname "$0")/attempts"
n=$(cat "$count" 2>/dev/null || echo 0)
n=$((n + 1))
printf '%s' "$n" > "$count"
if [ "$n" -le {truncated} ]; then
    printf '%s' '{{"messages":[{{"info":{{"role":"user"'
    exit 0
fi
if [ "$1" = "--pure" ]; then
    printf '%s' '{{"messages":[{{"info":{{"role":"user"}},"parts":[{{"type":"text","text":"hello"}}]}}]}}'
else
    printf '%s' '{{"messages":[{{"type":"user","id":"msg_1","text":"hello"}}]}}'
fi
"#
    );
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path
}

fn attempts(dir: &std::path::Path) -> usize {
    std::fs::read_to_string(dir.join("attempts"))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

#[tokio::test]
async fn incomplete_export_is_retried_until_the_document_completes() {
    let dir = tempfile::tempdir().unwrap();
    let bin = export_script(dir.path(), 2);
    output::export(&bin, "ses_1").await.unwrap();
    assert_eq!(
        attempts(dir.path()),
        3,
        "two short reads then a complete one"
    );
}

#[tokio::test]
async fn incomplete_export_stays_bounded_and_is_reported_as_incomplete() {
    let dir = tempfile::tempdir().unwrap();
    let bin = export_script(dir.path(), 999_999);
    let error = output::export(&bin, "ses_1").await.unwrap_err();
    assert_eq!(
        output::read_error_message(&error),
        "OpenCode export stayed incomplete after retries"
    );
    // Both API shapes are tried, and each one only retries its own bounded count.
    assert_eq!(attempts(dir.path()), 6);
}
