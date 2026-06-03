use std::collections::BTreeSet;

use super::ProviderRecord;

const GITHUB_PR_PREFIX: &str = "https://github.com/";

pub fn append_github_pr_links(
    entry: &mut ProviderRecord,
    seen: &mut BTreeSet<String>,
    text: &str,
    timestamp: Option<&str>,
) {
    for link in extract_github_pr_links(text, timestamp.unwrap_or_default()) {
        if seen.insert(link.1.clone()) {
            entry.pr_links.push(link);
        }
    }
}

pub fn extract_github_pr_links(text: &str, timestamp: &str) -> Vec<(i64, String, String, String)> {
    let mut links = Vec::new();
    let mut start_at = 0usize;
    while let Some(pos) = text[start_at..].find(GITHUB_PR_PREFIX) {
        let absolute = start_at + pos;
        let rest = &text[absolute + GITHUB_PR_PREFIX.len()..];
        if let Some((number, url, repo, consumed)) = parse_pr_path(rest) {
            links.push((number, url, repo, timestamp.to_string()));
            start_at = absolute + GITHUB_PR_PREFIX.len() + consumed;
        } else {
            start_at = absolute + GITHUB_PR_PREFIX.len();
        }
    }
    links
}

pub(crate) fn looks_like_gh_pr_command(text: &str) -> bool {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        return value_looks_like_gh_pr_command(&value, 0);
    }

    let lower = text.to_ascii_lowercase();
    let trimmed = lower.trim();
    if !trimmed.contains("gh pr") || trimmed.contains("\"gh pr\"") || trimmed.contains("'gh pr'") {
        return false;
    }
    !["rg ", "grep ", "sed ", "cat "]
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

fn value_looks_like_gh_pr_command(value: &serde_json::Value, depth: usize) -> bool {
    if depth > 4 {
        return false;
    }
    match value {
        serde_json::Value::String(s) => looks_like_gh_pr_command(s),
        serde_json::Value::Array(items) => {
            let joined = items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            looks_like_gh_pr_command(&joined)
                || items
                    .iter()
                    .any(|item| value_looks_like_gh_pr_command(item, depth + 1))
        }
        serde_json::Value::Object(map) => {
            ["command", "cmd", "parsed_cmd", "arguments", "args", "input"]
                .iter()
                .any(|field| {
                    map.get(*field)
                        .is_some_and(|v| value_looks_like_gh_pr_command(v, depth + 1))
                })
        }
        _ => false,
    }
}

pub(crate) fn looks_like_final_pr_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    text.contains("::git-create-pr")
        || (text.contains("https://github.com/") && lower.contains("opened "))
}

fn parse_pr_path(rest: &str) -> Option<(i64, String, String, usize)> {
    let bytes = rest.as_bytes();
    let owner_end = component_end(bytes, 0)?;
    if owner_end == 0 || bytes.get(owner_end) != Some(&b'/') {
        return None;
    }
    let repo_start = owner_end + 1;
    let repo_end = component_end(bytes, repo_start)?;
    if repo_end == repo_start || bytes.get(repo_end) != Some(&b'/') {
        return None;
    }
    let pull = b"pull/";
    let pull_start = repo_end + 1;
    if bytes.len() < pull_start + pull.len() || &bytes[pull_start..pull_start + pull.len()] != pull
    {
        return None;
    }
    let num_start = pull_start + pull.len();
    let num_end = bytes[num_start..]
        .iter()
        .position(|b| !b.is_ascii_digit())
        .map(|p| num_start + p)
        .unwrap_or(bytes.len());
    if num_end == num_start {
        return None;
    }

    let number = rest[num_start..num_end].parse::<i64>().ok()?;
    let owner = &rest[..owner_end];
    let repo_name = &rest[repo_start..repo_end];
    if is_placeholder_repo(owner, repo_name) {
        return None;
    }
    let repo = format!("{owner}/{repo_name}");
    let url = format!("{GITHUB_PR_PREFIX}{repo}/pull/{number}");
    Some((number, url, repo, num_end))
}

fn is_placeholder_repo(owner: &str, repo: &str) -> bool {
    matches!(
        (
            owner.to_ascii_lowercase().as_str(),
            repo.to_ascii_lowercase().as_str()
        ),
        ("org", "repo") | ("owner", "repo") | ("example", "repo")
    )
}

fn component_end(bytes: &[u8], start: usize) -> Option<usize> {
    bytes[start..]
        .iter()
        .position(|b| !is_repo_component_byte(*b))
        .map(|p| start + p)
        .or(Some(bytes.len()))
}

fn is_repo_component_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_canonicalizes_github_pr_urls() {
        let links = extract_github_pr_links(
            r#"opened "https://github.com/utensils/claudex/pull/38/files"."#,
            "2026-05-30T00:00:00Z",
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].0, 38);
        assert_eq!(links[0].1, "https://github.com/utensils/claudex/pull/38");
        assert_eq!(links[0].2, "utensils/claudex");
        assert_eq!(links[0].3, "2026-05-30T00:00:00Z");
    }

    #[test]
    fn ignores_placeholder_repo_urls() {
        let links = extract_github_pr_links("https://github.com/org/repo/pull/123", "ts");
        assert!(links.is_empty());
    }

    #[test]
    fn gh_pr_command_detection_ignores_search_commands() {
        assert!(looks_like_gh_pr_command("gh pr create --fill"));
        assert!(looks_like_gh_pr_command(
            "gh pr view https://github.com/utensils/claudex/pull/38"
        ));
        assert!(!looks_like_gh_pr_command(
            "rg -n \"gh pr\" ~/.codex/sessions"
        ));
        assert!(!looks_like_gh_pr_command(
            "sed -n '1,20p' docs.md # gh pr view https://github.com/org/repo/pull/123"
        ));
        assert!(looks_like_gh_pr_command(r#"{"cmd":"gh pr create --fill"}"#));
        assert!(!looks_like_gh_pr_command(
            r#"{"cmd":"rg -n \"pull/|gh pr|git-create-pr\" src tests"}"#
        ));
    }
}
