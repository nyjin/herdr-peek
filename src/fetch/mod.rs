//! GitHub PR content through the GitHub REST API, with a token from a `TokenStore` (`peek auth github`), cached
//! under `~/.cache/peek/links/`. Jira and Confluence were removed on 2026-11-18, to be reviewed again later (see the
//! design doc).
//!
//! `Fetched.title` is plain text; `Fetched.html` is a body fragment for peek's page template, which shows the title
//! and the fetch time itself. Every request goes to a configured base (`Endpoints`) plus a path; a URL found in
//! fetched content never reaches the HTTP client, so the token stays on its own site.
use crate::target::Target;
use std::io::{self, BufRead, Read, Write};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub mod cache;
pub mod github;

#[derive(Clone, Debug, PartialEq)]
pub struct Fetched {
    pub title: String,
    /// Body HTML (what goes inside peek's page template).
    pub html: String,
    pub url: String,
    /// RFC 3339.
    pub fetched_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FetchError {
    /// No token stored; `hint` is the `peek auth ...` command to run.
    NoToken { hint: String },
    Http { status: u16, url: String },
    Timeout,
    Other(String),
}

/// Key `github` holds a GitHub token.
pub trait TokenStore {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str) -> io::Result<()>;
}

#[derive(Clone, Debug)]
pub struct Endpoints {
    /// `https://api.github.com` in use (tests point it at a local server).
    pub github_api: String,
    pub timeout: Duration,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints { github_api: "https://api.github.com".into(), timeout: Duration::from_secs(20) }
    }
}

/// The page content for a link target; the cached copy unless `fresh`.
pub fn fetch(target: &Target, tokens: &dyn TokenStore, cache: &Path, fresh: bool, ep: &Endpoints) -> Result<Fetched, FetchError> {
    let url = target.url()
        .ok_or_else(|| FetchError::Other(format!("{} is not a GitHub PR link.", name(target))))?;
    if !fresh {
        if let Some(page) = cache::load(cache, &url) {
            return Ok(page);
        }
    }
    let (title, html) = match target {
        Target::GithubPr { repo, number } => github::fetch(&github_client(tokens, ep)?, repo, *number)?,
        Target::Path { .. } => unreachable!("a path has no web address"),
    };
    let page = Fetched { title, html, url, fetched_at: now() };
    // A cache that cannot be written only costs the next fetch; the page itself is fine.
    let _ = cache::store(cache, &page.url, &page);
    Ok(page)
}

/// A page that says what went wrong and how to fix it.
pub fn error_page(err: &FetchError, target: &Target) -> Fetched {
    let hint = auth_hint(target).unwrap_or_default();
    let (cause, fix) = match err {
        FetchError::NoToken { hint } => (
            "peek has no token for this service yet.".to_string(),
            format!("Run <code>{}</code> in a terminal, then press <kbd>r</kbd> here.", esc(hint)),
        ),
        FetchError::Http { status: status @ (401 | 403), .. } => (
            format!("The service refused the token (HTTP {status})."),
            format!(
                "The token may have expired or lack access. Register a new one with <code>{}</code>, then press <kbd>r</kbd>.",
                esc(&hint)
            ),
        ),
        FetchError::Http { status: 404, .. } => (
            "Not found (HTTP 404).".to_string(),
            format!(
                "Check the link. The service also answers 404 when the account behind the token cannot see it; \
                 register another with <code>{}</code> if so.",
                esc(&hint)
            ),
        ),
        FetchError::Http { status, url } => (
            format!("The service answered HTTP {status} for {url}."),
            "Press <kbd>r</kbd> to try again later.".to_string(),
        ),
        FetchError::Timeout => (
            "The service did not answer in time.".to_string(),
            "Check the network or VPN, then press <kbd>r</kbd> to try again.".to_string(),
        ),
        FetchError::Other(msg) => (msg.clone(), "Press <kbd>r</kbd> to try again.".to_string()),
    };
    let url = target.url().unwrap_or_default();
    let mut html = format!("<p>{}</p>\n<p>{fix}</p>\n", esc(&cause));
    if !url.is_empty() {
        let u = esc(&url);
        html += &format!("<p>Link: <a href=\"{u}\">{u}</a> (press <kbd>o</kbd> to open it in the browser)</p>\n");
    }
    Fetched { title: format!("Could not fetch {}", name(target)), html, url, fetched_at: now() }
}

/// `peek auth github`: the token comes from stdin.
pub fn run_auth(args: &[String], tokens: &dyn TokenStore) -> io::Result<()> {
    let usage = || io::Error::new(io::ErrorKind::InvalidInput, "Usage: peek auth github");
    let missing = || io::Error::new(io::ErrorKind::InvalidInput, "nothing was entered; nothing was saved");
    match args {
        [service] if service == "github" => {
            let token = read_secret("GitHub token with read access to the repositories: ")?;
            if token.is_empty() {
                return Err(missing());
            }
            tokens.set("github", &token)?;
            eprintln!("Saved the GitHub token.");
        }
        _ => return Err(usage()),
    }
    Ok(())
}

// ---- targets ----------------------------------------------------------------------------------

/// The link's web address (what `o` opens and what the cache is keyed by).
fn name(target: &Target) -> String {
    match target {
        Target::GithubPr { repo, number } => format!("{repo}#{number}"),
        Target::Path { text, .. } => text.clone(),
    }
}

fn auth_hint(target: &Target) -> Option<String> {
    match target {
        Target::GithubPr { .. } => Some("peek auth github".into()),
        Target::Path { .. } => None,
    }
}

/// `https://API.github.com/repos` → `api.github.com`.
fn host_of(site: &str) -> String {
    let rest = site.strip_prefix("https://").or_else(|| site.strip_prefix("http://")).unwrap_or(site);
    rest.split(['/', '?', '#']).next().unwrap_or("").to_ascii_lowercase()
}

fn github_client(tokens: &dyn TokenStore, ep: &Endpoints) -> Result<Client, FetchError> {
    let token = tokens
        .get("github")
        .filter(|t| !t.is_empty())
        .ok_or_else(|| FetchError::NoToken { hint: "peek auth github".into() })?;
    Ok(Client::new(
        ep,
        &ep.github_api,
        format!("Bearer {token}"),
        "application/vnd.github.html+json",
        Some(("X-GitHub-Api-Version", "2022-11-28")),
    ))
}

// ---- HTTP -------------------------------------------------------------------------------------

/// One service: requests go to `base` + a path, with its token and media type.
pub(crate) struct Client {
    agent: ureq::Agent,
    base: String,
    auth: String,
    accept: &'static str,
    extra: Option<(&'static str, &'static str)>,
}

impl Client {
    fn new(ep: &Endpoints, base: &str, auth: String, accept: &'static str, extra: Option<(&'static str, &'static str)>) -> Client {
        let agent = ureq::AgentBuilder::new()
            .timeout(ep.timeout)
            .user_agent("peek")
            // A redirect never carries the token on.
            .redirect_auth_headers(ureq::RedirectAuthHeaders::Never)
            .build();
        Client { agent, base: base.trim_end_matches('/').to_string(), auth, accept, extra }
    }

    /// `path` starts with `/` and may hold a query.
    pub(crate) fn json(&self, path: &str) -> Result<serde_json::Value, FetchError> {
        let (body, _) = self.get(path, self.accept)?;
        serde_json::from_slice(&body)
            .map_err(|e| FetchError::Other(format!("Unexpected answer from {}{path}: {e}", self.base)))
    }

    /// The body and its content type.
    pub(crate) fn get(&self, path: &str, accept: &str) -> Result<(Vec<u8>, String), FetchError> {
        let url = format!("{}{path}", self.base);
        if !path.starts_with('/') || path.starts_with("//") {
            return Err(FetchError::Other(format!("Refused to request {url}")));
        }
        let mut req = self.agent.get(&url).set("Authorization", &self.auth).set("Accept", accept);
        if let Some((k, v)) = self.extra {
            req = req.set(k, v);
        }
        let resp = req.call().map_err(|e| match e {
            ureq::Error::Status(status, _) => FetchError::Http { status, url: url.clone() },
            ureq::Error::Transport(t) if timed_out(&t) => FetchError::Timeout,
            ureq::Error::Transport(t) => FetchError::Other(format!("Could not reach {} ({t}).", host_of(&url))),
        })?;
        let content_type = resp.content_type().to_string();
        let mut body = Vec::new();
        resp.into_reader().read_to_end(&mut body).map_err(|e| match e.kind() {
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => FetchError::Timeout,
            _ => FetchError::Other(format!("Could not read the answer from {url}: {e}")),
        })?;
        Ok((body, content_type))
    }
}

fn timed_out(t: &ureq::Transport) -> bool {
    let mut source = std::error::Error::source(t);
    while let Some(e) = source {
        if let Some(io) = e.downcast_ref::<io::Error>() {
            if matches!(io.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock) {
                return true;
            }
        }
        source = e.source();
    }
    false
}

// ---- text -------------------------------------------------------------------------------------

pub(crate) fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

/// `2026-11-01T10:00:00.000+0900` → `2026-11-01 10:00`.
pub(crate) fn short_time(iso: &str) -> String {
    iso.chars().take(16).collect::<String>().replace('T', " ")
}

/// Now in UTC, RFC 3339 to the second.
fn now() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let (era, doe) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

/// One secret line from stdin after a prompt on stderr; without echo when stdin is a terminal.
fn read_secret(prompt: &str) -> io::Result<String> {
    eprint!("{prompt}");
    io::stderr().flush()?;
    let quiet = NoEcho::on_stdin();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    if quiet.is_some() {
        eprintln!();
    }
    Ok(line.trim().to_string())
}

/// Terminal echo off on stdin until dropped.
struct NoEcho(libc::termios);

impl NoEcho {
    fn on_stdin() -> Option<NoEcho> {
        // SAFETY: termios calls on fd 0 with a zeroed struct that tcgetattr fills before use.
        unsafe {
            if libc::isatty(0) != 1 {
                return None;
            }
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(0, &mut t) != 0 {
                return None;
            }
            let saved = t;
            t.c_lflag &= !libc::ECHO;
            libc::tcsetattr(0, libc::TCSANOW, &t);
            Some(NoEcho(saved))
        }
    }
}

impl Drop for NoEcho {
    fn drop(&mut self) {
        // SAFETY: restores the settings read in `on_stdin`.
        unsafe {
            libc::tcsetattr(0, libc::TCSANOW, &self.0);
        }
    }
}
