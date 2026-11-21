//! GitHub PR fetching against a local fake server (`tiny_http`) with an in-memory token store.
//! Nothing here talks to the real services or the Keychain.
use peek::fetch::{self, Endpoints, FetchError, Fetched, TokenStore};
use peek::target::Target;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ---- fake server ------------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Hit {
    path: String,
    query: String,
    headers: Vec<(String, String)>,
}

impl Hit {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    fn param(&self, name: &str) -> Option<&str> {
        self.query.split('&').find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='))
    }
}

struct Reply {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    delay: Duration,
}

fn json(status: u16, body: impl Into<Vec<u8>>) -> Reply {
    Reply { status, content_type: "application/json", body: body.into(), delay: Duration::ZERO }
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fetch").join(name)).unwrap()
}

struct Fake {
    base: String,
    hits: Arc<Mutex<Vec<Hit>>>,
}

impl Fake {
    fn start(handler: impl Fn(&Hit) -> Reply + Send + Sync + 'static) -> Fake {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let hits = Arc::new(Mutex::new(Vec::new()));
        let handler = Arc::new(handler);
        let recorded = hits.clone();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let url = req.url().to_string();
                let (path, query) = match url.split_once('?') {
                    Some((p, q)) => (p.to_string(), q.to_string()),
                    None => (url.clone(), String::new()),
                };
                let headers =
                    req.headers().iter().map(|h| (h.field.as_str().as_str().to_string(), h.value.as_str().to_string())).collect();
                let hit = Hit { path, query, headers };
                recorded.lock().unwrap().push(hit.clone());
                let handler = handler.clone();
                std::thread::spawn(move || {
                    let reply = handler(&hit);
                    std::thread::sleep(reply.delay);
                    let ct = tiny_http::Header::from_bytes("Content-Type", reply.content_type).unwrap();
                    let _ = req.respond(tiny_http::Response::from_data(reply.body).with_status_code(reply.status).with_header(ct));
                });
            }
        });
        Fake { base: format!("http://127.0.0.1:{port}"), hits }
    }

    fn hits(&self) -> Vec<Hit> {
        self.hits.lock().unwrap().clone()
    }

    fn hits_to(&self, path: &str) -> Vec<Hit> {
        self.hits().into_iter().filter(|h| h.path == path).collect()
    }

    fn endpoints(&self) -> Endpoints {
        Endpoints { github_api: self.base.clone(), timeout: Duration::from_millis(500) }
    }
}

// ---- tokens and targets -----------------------------------------------------------------------

#[derive(Default)]
struct Mem(Mutex<HashMap<String, String>>);

impl Mem {
    fn with(pairs: &[(&str, &str)]) -> Mem {
        Mem(Mutex::new(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()))
    }
}

impl TokenStore for Mem {
    fn get(&self, key: &str) -> Option<String> {
        self.0.lock().unwrap().get(key).cloned()
    }
    fn set(&self, key: &str, value: &str) -> io::Result<()> {
        self.0.lock().unwrap().insert(key.to_string(), value.to_string());
        Ok(())
    }
}

fn tokens() -> Mem {
    Mem::with(&[("github", "gh-token")])
}

fn pr() -> Target {
    Target::GithubPr { repo: "acme/app".into(), number: 42 }
}

fn github(hit: &Hit) -> Reply {
    let first = hit.param("page").unwrap_or("1") == "1";
    match hit.path.as_str() {
        "/repos/acme/app/pulls/42" => json(200, fixture("github_pr.json")),
        "/repos/acme/app/issues/42/comments" if first => json(200, fixture("github_issue_comments.json")),
        "/repos/acme/app/pulls/42/comments" if first => json(200, fixture("github_review_comments.json")),
        "/repos/acme/app/issues/42/comments" | "/repos/acme/app/pulls/42/comments" => json(200, "[]"),
        _ => json(404, r#"{"message":"Not Found","documentation_url":"https://docs.github.com/rest","status":"404"}"#),
    }
}

fn get(target: &Target, fake: &Fake, cache: &Path) -> Result<Fetched, FetchError> {
    fetch::fetch(target, &tokens(), cache, false, &fake.endpoints())
}

// ---- GitHub -----------------------------------------------------------------------------------

#[test]
fn github_requests_html_bodies() {
    let fake = Fake::start(github);
    let cache = tempfile::tempdir().unwrap();
    let page = get(&pr(), &fake, cache.path()).unwrap();

    for path in ["/repos/acme/app/pulls/42", "/repos/acme/app/issues/42/comments", "/repos/acme/app/pulls/42/comments"] {
        let hits = fake.hits_to(path);
        assert_eq!(hits.len(), 1, "{path}");
        assert_eq!(hits[0].header("Accept"), Some("application/vnd.github.html+json"), "{path}");
        assert_eq!(hits[0].header("Authorization"), Some("Bearer gh-token"), "{path}");
    }
    assert_eq!(page.title, "acme/app#42 Add retry to uploads");
    assert_eq!(page.url, "https://github.com/acme/app/pull/42");
    assert!(page.html.contains("Open"), "{}", page.html);
    assert!(page.html.contains("octo"), "{}", page.html);
    assert!(page.html.contains("by retrying <code>put</code>"), "{}", page.html);
    assert!(page.html.contains("<p>Looks good to me</p>"), "{}", page.html);
    assert!(page.html.contains("src/upload.rs:12"), "{}", page.html);
    assert!(page.html.contains("<p>Use a constant for the retry count</p>"), "{}", page.html);
}

#[test]
fn github_comments_past_the_first_page_are_fetched() {
    let fake = Fake::start(|hit: &Hit| {
        if hit.path != "/repos/acme/app/issues/42/comments" {
            return github(hit);
        }
        let comments: Vec<_> = match hit.param("page").unwrap_or("1") {
            "1" => (0..100).map(|i| serde_json::json!({"id": i, "user": {"login": "bot"}, "created_at": "2026-11-02T09:00:00Z", "body_html": format!("<p>note {i}</p>")})).collect(),
            "2" => vec![serde_json::json!({"id": 100, "user": {"login": "bot"}, "created_at": "2026-11-03T09:00:00Z", "body_html": "<p>the last note</p>"})],
            _ => vec![],
        };
        json(200, serde_json::Value::Array(comments).to_string())
    });
    let cache = tempfile::tempdir().unwrap();
    let page = get(&pr(), &fake, cache.path()).unwrap();
    let pages: Vec<String> =
        fake.hits_to("/repos/acme/app/issues/42/comments").iter().map(|h| h.param("page").unwrap_or("1").to_string()).collect();
    assert_eq!(pages, ["1", "2"]);
    assert!(fake.hits_to("/repos/acme/app/issues/42/comments").iter().all(|h| h.param("per_page") == Some("100")));
    assert!(page.html.contains("<p>note 99</p>"));
    assert!(page.html.contains("<p>the last note</p>"));
}

#[test]
fn github_body_without_html_is_shown_as_text() {
    // An endpoint that ignores the html media type answers with the markdown source in `body` only.
    let fake = Fake::start(|hit: &Hit| {
        let raw = |v: serde_json::Value| json(200, v.to_string());
        match hit.path.as_str() {
            "/repos/acme/app/pulls/42" => {
                let mut pr: serde_json::Value = serde_json::from_str(&fixture("github_pr.json")).unwrap();
                pr.as_object_mut().unwrap().remove("body_html");
                pr["body"] = "PR <b>source</b>".into();
                raw(pr)
            }
            "/repos/acme/app/issues/42/comments" => raw(serde_json::json!([{"id": 1, "user": {"login": "a"}, "created_at": "2026-11-02T09:00:00Z", "body": "issue & comment"}])),
            "/repos/acme/app/pulls/42/comments" => raw(serde_json::json!([{"id": 2, "user": {"login": "b"}, "path": "x.rs", "line": 3, "created_at": "2026-11-02T10:00:00Z", "body": "line one\nline <two>"}])),
            _ => github(hit),
        }
    });
    let cache = tempfile::tempdir().unwrap();
    let page = get(&pr(), &fake, cache.path()).unwrap();
    assert!(page.html.contains("PR &lt;b&gt;source&lt;/b&gt;"), "{}", page.html);
    assert!(page.html.contains("issue &amp; comment"), "{}", page.html);
    assert!(page.html.contains("line one<br>\nline &lt;two&gt;"), "{}", page.html);
}

#[test]
fn closed_draft_pr_shows_closed() {
    let fake = Fake::start(|hit: &Hit| {
        if hit.path != "/repos/acme/app/pulls/42" {
            return github(hit);
        }
        let mut pr: serde_json::Value = serde_json::from_str(&fixture("github_pr.json")).unwrap();
        pr["state"] = "closed".into();
        pr["draft"] = true.into();
        json(200, pr.to_string())
    });
    let cache = tempfile::tempdir().unwrap();
    let page = get(&pr(), &fake, cache.path()).unwrap();
    assert!(page.html.contains("<strong>Closed</strong>"), "{}", page.html);
}

// ---- cache ------------------------------------------------------------------------------------

#[test]
fn cache_is_used_unless_fresh() {
    let fake = Fake::start(github);
    let cache = tempfile::tempdir().unwrap();
    let ep = fake.endpoints();
    let first = fetch::fetch(&pr(), &tokens(), cache.path(), false, &ep).unwrap();
    let again = fetch::fetch(&pr(), &tokens(), cache.path(), false, &ep).unwrap();
    assert_eq!(fake.hits_to("/repos/acme/app/pulls/42").len(), 1);
    assert_eq!(again, first);
    fetch::fetch(&pr(), &tokens(), cache.path(), true, &ep).unwrap();
    assert_eq!(fake.hits_to("/repos/acme/app/pulls/42").len(), 2);
}

#[test]
fn cache_folder_is_private_even_when_an_older_peek_made_it_open() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    // an older peek made the folder with the default mode
    let cache = dir.path().join("links");
    std::fs::create_dir(&cache).unwrap();
    std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o755)).unwrap();
    let page = Fetched { title: "t".into(), html: "<p>h</p>".into(), url: "https://github.com/acme/app/pull/42".into(), fetched_at: "2026-11-21T00:00:00Z".into() };
    fetch::cache::store(&cache, &page.url, &page).unwrap();
    let mode = std::fs::metadata(&cache).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "mode {mode:o}");
}

#[test]
fn unwritable_cache_does_not_fail_the_fetch() {
    let fake = Fake::start(github);
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("a-file");
    std::fs::write(&cache, "not a folder").unwrap();
    let page = get(&pr(), &fake, &cache).unwrap();
    assert_eq!(page.title, "acme/app#42 Add retry to uploads");
}

// ---- errors -----------------------------------------------------------------------------------

#[test]
fn no_token_points_to_peek_auth() {
    let fake = Fake::start(github);
    let cache = tempfile::tempdir().unwrap();
    let err = fetch::fetch(&pr(), &Mem::default(), cache.path(), false, &fake.endpoints()).unwrap_err();
    assert_eq!(err, FetchError::NoToken { hint: "peek auth github".into() });
    assert!(fetch::error_page(&err, &pr()).html.contains("peek auth github"));
    assert!(fake.hits().is_empty());
}

#[test]
fn unauthorized_points_to_peek_auth() {
    for code in [401, 403] {
        let fake = Fake::start(move |_: &Hit| json(code, "{}"));
        let cache = tempfile::tempdir().unwrap();
        let err = get(&pr(), &fake, cache.path()).unwrap_err();
        assert!(matches!(err, FetchError::Http { status, .. } if status == code), "{err:?}");
        let page = fetch::error_page(&err, &pr());
        assert!(page.html.contains("peek auth github"), "{}", page.html);
        assert!(page.html.contains(&code.to_string()), "{}", page.html);
    }
}

#[test]
fn not_found_page() {
    let fake = Fake::start(|_: &Hit| json(404, r#"{"message":"Not Found","status":"404"}"#));
    let cache = tempfile::tempdir().unwrap();
    let err = get(&pr(), &fake, cache.path()).unwrap_err();
    assert!(matches!(err, FetchError::Http { status: 404, .. }), "{err:?}");
    let page = fetch::error_page(&err, &pr());
    assert!(page.title.contains("acme/app#42"), "{}", page.title);
    assert!(page.html.to_lowercase().contains("not found"), "{}", page.html);
    assert_eq!(page.url, "https://github.com/acme/app/pull/42");
}

#[test]
fn timeout_gives_error_page() {
    let fake = Fake::start(|hit: &Hit| Reply { delay: Duration::from_secs(3), ..github(hit) });
    let cache = tempfile::tempdir().unwrap();
    let start = Instant::now();
    let err = get(&pr(), &fake, cache.path()).unwrap_err();
    assert!(start.elapsed() < Duration::from_secs(2), "{:?}", start.elapsed());
    assert_eq!(err, FetchError::Timeout);
    let page = fetch::error_page(&err, &pr());
    assert!(page.html.contains("in time"), "{}", page.html);
}

#[test]
fn unreachable_gives_error_page() {
    // Port 1 (tcpmux) has no listener; a freed ephemeral port could be taken by another test's fake server.
    let base = "http://127.0.0.1:1".to_string();
    let ep = Endpoints { github_api: base, timeout: Duration::from_millis(500) };
    let cache = tempfile::tempdir().unwrap();
    let err = fetch::fetch(&pr(), &tokens(), cache.path(), false, &ep).unwrap_err();
    assert!(matches!(err, FetchError::Other(_)), "{err:?}");
    assert!(fetch::error_page(&err, &pr()).html.contains("Could not reach"));
}

// ---- peek auth --------------------------------------------------------------------------------

/// Runs this test binary again as a child that calls `run_auth` with real stdin (a pipe) and writes what it stored.
fn auth_child(args: &str, stdin: &str) -> (String, String) {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("stored");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["auth_reads_secret_from_stdin", "--exact", "--nocapture", "--test-threads=1"])
        .env("PEEK_TEST_AUTH_ARGS", args)
        .env("PEEK_TEST_AUTH_OUT", &out)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let res = child.wait_with_output().unwrap();
    assert!(res.status.success(), "{}", String::from_utf8_lossy(&res.stderr));
    (std::fs::read_to_string(&out).unwrap(), String::from_utf8_lossy(&res.stderr).into_owned())
}

#[test]
fn auth_reads_secret_from_stdin() {
    if let (Ok(args), Ok(out)) = (std::env::var("PEEK_TEST_AUTH_ARGS"), std::env::var("PEEK_TEST_AUTH_OUT")) {
        let store = Mem::default();
        let args: Vec<String> = args.split_whitespace().map(String::from).collect();
        let report = match fetch::run_auth(&args, &store) {
            Ok(()) => {
                let mut stored: Vec<String> = store.0.lock().unwrap().iter().map(|(k, v)| format!("{k}={v}")).collect();
                stored.sort();
                format!("ok {}", stored.join(" "))
            }
            Err(e) => format!("err {} stored={}", e.kind() == io::ErrorKind::InvalidInput, store.0.lock().unwrap().len()),
        };
        std::fs::write(out, report).unwrap();
        return;
    }

    let (stored, stderr) = auth_child("github", "ghp-secret-1\n");
    assert_eq!(stored, "ok github=ghp-secret-1");
    assert!(stderr.contains("token"), "{stderr}");
    assert!(!stderr.contains("ghp-secret-1"), "{stderr}");

    // Jira and Confluence were removed (2026-11-18): an Atlassian site is no longer taken.
    let (stored, _) = auth_child("atlassian https://example.atlassian.net/", "me@example.com\nat-secret-2\n");
    assert_eq!(stored, "err true stored=0");

    let (stored, _) = auth_child("github", "\n");
    assert_eq!(stored, "err true stored=0");
}
