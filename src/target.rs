//! Sort a clicked string into a link peek fetches (GitHub PR) or a file path.
use regex::Regex;
use std::sync::LazyLock;

static GITHUB_PR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^https://github\.com/([^/\s]+/[^/\s]+)/pull/([0-9]+)").unwrap());

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// `repo` is `owner/name`.
    GithubPr { repo: String, number: u64 },
    /// `text` is normalized (`resolve::normalize`); a web address that is not a GitHub PR stays a `Path` and
    /// starts with `http://` or `https://`.
    Path { text: String, line: Option<u32> },
}

impl Target {
    /// The web address of a link target; `None` for a path.
    pub fn url(&self) -> Option<String> {
        match self {
            Target::GithubPr { repo, number } => Some(format!("https://github.com/{repo}/pull/{number}")),
            Target::Path { .. } => None,
        }
    }
}

pub fn classify(text: &str) -> Target {
    let (t, line) = crate::resolve::split_line(text);
    if let Some(m) = GITHUB_PR.captures(&t) {
        if let Ok(number) = m[2].parse() {
            return Target::GithubPr { repo: m[1].to_string(), number };
        }
    }
    Target::Path { text: t, line }
}
