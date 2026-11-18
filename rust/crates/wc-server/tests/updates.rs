//! `GET /api/system/updates`: GitHub's answer parsed, versions compared, the check cached and degraded offline.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::Path;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use wc_config::Paths;
use wc_machine::{Machine, NoHost, NoPort, spawn};
use wc_protocol::system::{LatestRelease, UpdateBackup, UpdateState};
use wc_server::updates::{
    DEFAULT_REPO, GitHub, ReleaseSource, Timing, Updates, Version, compare, parse_manifest, parse_release, repo_from,
};
use wc_server::{AppState, PingConfig, router};
use wc_testkit::contract::{Contract, ResponseBody};

const FIXTURE: &str = include_str!("fixtures/release_latest.json");
const SHA: &str = "b9ea4ee0d14d01a0640427b77e6ee519a2af2f2e";

fn release(tag: &str, commit: Option<&str>) -> LatestRelease {
    LatestRelease {
        tag: tag.into(),
        name: tag.into(),
        published_at: None,
        url: "https://github.com/o/r/releases/tag/x".into(),
        notes: String::new(),
        commit: commit.map(str::to_string),
    }
}

#[test]
fn a_recorded_latest_release_parses_to_its_tag_name_date_page_and_notes() {
    let parsed = parse_release(FIXTURE).unwrap();
    assert_eq!(parsed.tag, "v0.3.0");
    assert_eq!(parsed.name, "webcontrol-rs 0.3.0");
    assert_eq!(parsed.published_at.as_deref(), Some("2026-11-16T20:59:04Z"));
    assert_eq!(parsed.url, "https://github.com/designfrontier/WebControl/releases/tag/v0.3.0");
    assert!(parsed.notes.starts_with("# WebControl 0.3.0\n\n## Display theme"), "{}", parsed.notes);
    assert!(!parsed.notes.contains('\r'));
    assert_eq!(parsed.commit, None);
}

#[test]
fn a_release_without_a_usable_tag_or_page_is_refused_and_a_missing_name_is_the_tag() {
    assert_eq!(parse_release("<html>").unwrap_err(), "GitHub's answer was not JSON");
    let base = json!({"tag_name": "v1.0.0", "html_url": "https://github.com/o/r/releases/tag/v1.0.0"});
    let with = |key: &str, value: Value| {
        let mut body = base.clone();
        body[key] = value;
        parse_release(&body.to_string())
    };
    assert_eq!(with("tag_name", json!("../../evil")).unwrap_err(), "The latest release has no version tag");
    assert_eq!(with("tag_name", Value::Null).unwrap_err(), "The latest release has no version tag");
    assert_eq!(with("tag_name", json!("v")).unwrap_err(), "The latest release has no version tag");
    assert_eq!(with("html_url", json!("")).unwrap_err(), "The latest release has no web address");
    let bare = with("name", Value::Null).unwrap();
    assert_eq!((bare.name.as_str(), bare.notes.as_str(), bare.published_at), ("v1.0.0", "", None));
}

#[test]
fn versions_compare_as_semver() {
    let v = |text: &str| Version::parse(text).unwrap_or_else(|| panic!("{text}"));
    assert!(v("0.3.0") < v("v0.4.0"));
    assert!(v("0.10.0") > v("0.9.9"));
    assert!(v("1.0.0-rc.1") < v("1.0.0"));
    assert!(v("1.0.0-alpha") < v("1.0.0-alpha.1"));
    assert!(v("1.0.0-alpha.2") < v("1.0.0-alpha.10"));
    assert!(v("1.0.0-2") < v("1.0.0-alpha"));
    assert_eq!(v("1.2.3+build.5"), v("v1.2.3"));
    assert_eq!(v("1.2.3").partial_cmp(&v("1.2.3")), Some(std::cmp::Ordering::Equal));
    assert_eq!(v("v1.2"), v("1.2.0"));
    assert_eq!(v("1"), v("1.0.0"));
    ["", "dev", "1.2.3.4", "1.x.3", "+1.2.3", "1.2.3-", "1.2.3-a..b", "1.2.3-a_b"]
        .iter()
        .for_each(|text| assert_eq!(Version::parse(text), None, "{text}"));
}

#[test]
fn a_git_describe_build_comes_after_its_release_as_webcontrol_update_orders_it() {
    let v = |text: &str| Version::parse(text).unwrap_or_else(|| panic!("{text}"));
    assert!(v("0.4.0-3-gabc1234") > v("0.4.0"));
    assert!(v("0.4.0-3-gabc1234") < v("0.4.0-12-g0123456"));
    assert!(v("0.4.0-3-gabc1234-dirty") > v("0.4.0"));
    assert!(v("0.4.0-dirty") > v("0.4.0"));
    assert!(v("0.4.0-3-gabc1234") < v("0.4.1-rc.1"));
    assert!(v("0.4.0-rc.1") < v("0.4.0"));
    assert!(v("0.4.0-3-gXYZ") < v("0.4.0"), "not a commit hash, so a pre-release");
}

#[test]
fn the_installed_build_against_the_latest_release() {
    let latest = |tag: &str, commit: Option<&str>| Some(release(tag, commit));
    assert_eq!(compare("0.3.0", "b9ea4ee", latest("v0.4.0", Some(SHA)).as_ref()), UpdateState::UpdateAvailable);
    assert_eq!(compare("0.3.0", "b9ea4ee", latest("v0.3.0", Some(SHA)).as_ref()), UpdateState::UpToDate);
    assert_eq!(compare("0.3.0", "dc9bdca", latest("v0.3.0", Some(SHA)).as_ref()), UpdateState::DevBuild);
    assert_eq!(compare("0.3.0", "dc9bdca", latest("v0.3.0", None).as_ref()), UpdateState::UpToDate);
    assert_eq!(compare("0.3.0", "unknown", latest("v0.3.0", Some(SHA)).as_ref()), UpdateState::UpToDate);
    assert_eq!(compare("0.4.0", "dc9bdca", latest("v0.3.0", Some(SHA)).as_ref()), UpdateState::Ahead);
    assert_eq!(compare("0.3.0-rc.1", "dc9bdca", latest("v0.3.0", None).as_ref()), UpdateState::UpdateAvailable);
    assert_eq!(compare("0.3.0-2-gdc9bdca", "dc9bdca", latest("v0.3.0", Some(SHA)).as_ref()), UpdateState::Ahead);
    assert_eq!(compare("dev", "dc9bdca", latest("v0.3.0", None).as_ref()), UpdateState::Unknown);
    assert_eq!(compare("0.3.0", "dc9bdca", latest("nightly", None).as_ref()), UpdateState::Unknown);
    assert_eq!(compare("0.3.0", "dc9bdca", None), UpdateState::Unknown);
}

#[test]
fn the_backup_manifest_gives_the_version_and_date() {
    let manifest = "version=0.2.0\ndate=2026-11-16T18:00:00Z\nfile=/opt/webcontrol-rs/webcontrol-rs\n";
    let backup = UpdateBackup { version: "0.2.0".into(), date: "2026-11-16T18:00:00Z".into() };
    assert_eq!(parse_manifest(manifest), Some(backup));
    assert_eq!(parse_manifest("version=0.2.0\n").unwrap().date, "");
    assert_eq!(parse_manifest("date=2026-11-16\n"), None);
    assert_eq!(parse_manifest("version=\n"), None);
}

#[test]
fn the_repository_is_webcontrol_repo_if_it_reads_owner_slash_name() {
    assert_eq!(repo_from(None), DEFAULT_REPO);
    assert_eq!(repo_from(Some("me/Web-Control.rs".into())), "me/Web-Control.rs");
    ["me", "me/", "/repo", "me/repo/x", "me/re po", "../x", "me/."]
        .iter()
        .for_each(|bad| assert_eq!(repo_from(Some(bad.to_string())), DEFAULT_REPO, "{bad}"));
}

/// A stand-in for api.github.com; `latest` answers `/repos/o/r/releases/latest`, `sha` the commit lookup.
struct FakeGitHub {
    url: String,
    seen: Arc<Mutex<Vec<HeaderMap>>>,
}

async fn fake_github(latest: (StatusCode, String), sha: (StatusCode, String), delay: Duration) -> FakeGitHub {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (seen_latest, seen_sha) = (seen.clone(), seen.clone());
    let app = Router::new()
        .route(
            "/repos/o/r/releases/latest",
            get(move |headers: HeaderMap| async move {
                seen_latest.lock().unwrap().push(headers);
                tokio::time::sleep(delay).await;
                latest
            }),
        )
        .route(
            "/repos/o/r/commits/{tag}",
            get(move |Path(tag): Path<String>, headers: HeaderMap| async move {
                seen_sha.lock().unwrap().push(headers);
                assert_eq!(tag, "v0.3.0");
                sha
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    FakeGitHub { url, seen }
}

fn github(api: &str, token: Option<&str>, timeout: Duration) -> GitHub {
    GitHub { api: format!("{api}/"), repo: "o/r".into(), token: token.map(str::to_string), timeout }
}

async fn latest_from(source: GitHub) -> Result<LatestRelease, String> {
    tokio::task::spawn_blocking(move || source.latest()).await.unwrap()
}

const SECOND: Duration = Duration::from_secs(1);

#[tokio::test]
async fn github_is_asked_for_the_latest_release_and_its_tags_commit() {
    let fake =
        fake_github((StatusCode::OK, FIXTURE.into()), (StatusCode::OK, format!("{SHA}\n")), Duration::ZERO).await;
    let latest = latest_from(github(&fake.url, Some("t0ken"), 5 * SECOND)).await.unwrap();
    assert_eq!((latest.tag.as_str(), latest.commit.as_deref()), ("v0.3.0", Some(SHA)));
    let seen = fake.seen.lock().unwrap();
    let header = |n: usize, name: &str| seen[n].get(name).map(|v| v.to_str().unwrap().to_string());
    assert_eq!(header(0, "accept").as_deref(), Some("application/vnd.github+json"));
    assert_eq!(header(1, "accept").as_deref(), Some("application/vnd.github.sha"));
    assert_eq!(header(0, "authorization").as_deref(), Some("Bearer t0ken"));
    assert!(header(0, "user-agent").unwrap().starts_with("webcontrol-rs/"));
}

#[tokio::test]
async fn a_tag_github_will_not_resolve_leaves_the_commit_unknown() {
    let cases = [(StatusCode::NOT_FOUND, "{}".to_string()), (StatusCode::OK, "not a sha".to_string())];
    for sha in cases {
        let fake = fake_github((StatusCode::OK, FIXTURE.into()), sha, Duration::ZERO).await;
        let latest = latest_from(github(&fake.url, None, 5 * SECOND)).await.unwrap();
        assert_eq!(latest.commit, None);
        assert_eq!(fake.seen.lock().unwrap()[0].get("authorization"), None);
    }
}

#[tokio::test]
async fn github_refusals_read_as_reasons() {
    let cases = [
        (StatusCode::NOT_FOUND, "o/r has no published release yet"),
        (StatusCode::FORBIDDEN, "GitHub refused (HTTP 403), most likely its hourly rate limit"),
        (StatusCode::TOO_MANY_REQUESTS, "GitHub refused (HTTP 429), most likely its hourly rate limit"),
        (StatusCode::BAD_GATEWAY, "GitHub answered HTTP 502"),
    ];
    for (status, reason) in cases {
        let fake = fake_github((status, "{}".into()), (StatusCode::OK, SHA.into()), Duration::ZERO).await;
        assert_eq!(latest_from(github(&fake.url, None, 5 * SECOND)).await.unwrap_err(), reason);
    }
    let fake = fake_github((StatusCode::OK, "x".repeat(2 << 20)), (StatusCode::OK, SHA.into()), Duration::ZERO).await;
    let error = latest_from(github(&fake.url, None, 5 * SECOND)).await.unwrap_err();
    assert!(error.starts_with("Couldn't read GitHub's answer"), "{error}");
}

#[tokio::test]
async fn an_unreachable_or_slow_github_is_an_error_not_a_wait() {
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    let error = latest_from(github(&url, None, 5 * SECOND)).await.unwrap_err();
    assert!(error.starts_with("Couldn't reach GitHub: "), "{error}");

    let slow = fake_github((StatusCode::OK, FIXTURE.into()), (StatusCode::OK, SHA.into()), 10 * SECOND).await;
    let started = std::time::Instant::now();
    let error = latest_from(github(&slow.url, None, Duration::from_millis(200))).await.unwrap_err();
    assert!(error.starts_with("Couldn't reach GitHub: "), "{error}");
    assert!(started.elapsed() < 5 * SECOND);
}

/// Answers from a list in turn, counting the calls; past the end it repeats the last.
#[derive(Clone)]
struct Scripted {
    answers: Arc<Vec<Result<LatestRelease, String>>>,
    calls: Arc<AtomicUsize>,
    delay: Duration,
}

impl Scripted {
    fn new(answers: Vec<Result<LatestRelease, String>>) -> Scripted {
        Scripted { answers: Arc::new(answers), calls: Arc::new(AtomicUsize::new(0)), delay: Duration::ZERO }
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ReleaseSource for Scripted {
    fn latest(&self) -> Result<LatestRelease, String> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(self.delay);
        self.answers[n.min(self.answers.len() - 1)].clone()
    }
}

struct Panics;

impl ReleaseSource for Panics {
    fn latest(&self) -> Result<LatestRelease, String> {
        panic!("a bug in the check")
    }
}

const LONG: Timing = Timing { fresh: Duration::from_secs(600), retry: Duration::from_secs(30), timeout: SECOND };
const NONE: Timing = Timing { fresh: Duration::ZERO, retry: Duration::ZERO, timeout: SECOND };

fn updates(source: impl ReleaseSource, backup: &std::path::Path, timing: Timing) -> Updates {
    Updates::new(source, "o/r", backup, timing)
}

#[tokio::test]
async fn a_check_is_kept_for_ten_minutes_and_shared() {
    let source = Scripted::new(vec![Ok(release("v999.0.0", None))]);
    let checker = updates(source.clone(), std::path::Path::new("/nonexistent"), LONG);
    let (first, second) = tokio::join!(checker.report(), checker.report());
    assert_eq!(source.calls(), 1);
    assert_eq!(first, second);
    assert_eq!(first.status, UpdateState::UpdateAvailable);
    assert_eq!(first.repo, "o/r");
    assert_eq!(first.installed.version, wc_server::VERSION);
    assert_eq!(first.installed.commit, wc_server::COMMIT);
    assert!(first.checked_at.unwrap() > 1_700_000_000_000);
    assert_eq!(first.backup, None);

    let source = Scripted::new(vec![Ok(release("v0.0.1", None))]);
    let checker = updates(source.clone(), std::path::Path::new("/nonexistent"), NONE);
    checker.report().await;
    assert_eq!(checker.report().await.status, UpdateState::Ahead);
    assert_eq!(source.calls(), 2);
}

#[tokio::test]
async fn offline_reads_as_an_error_retried_sooner_and_keeps_the_release_found_before() {
    let source = Scripted::new(vec![Err("Couldn't reach GitHub: dns".into())]);
    let checker = updates(source.clone(), std::path::Path::new("/nonexistent"), LONG);
    let offline = checker.report().await;
    assert_eq!((offline.latest, offline.status), (None, UpdateState::Unknown));
    assert_eq!(offline.error.as_deref(), Some("Couldn't reach GitHub: dns"));
    checker.report().await;
    assert_eq!(source.calls(), 1, "an error is kept for the retry time");

    let timing = Timing { fresh: Duration::ZERO, retry: Duration::ZERO, timeout: SECOND };
    let source = Scripted::new(vec![Ok(release("v999.0.0", None)), Err("GitHub answered HTTP 502".into())]);
    let checker = updates(source.clone(), std::path::Path::new("/nonexistent"), timing);
    checker.report().await;
    let later = checker.report().await;
    assert_eq!(later.latest.unwrap().tag, "v999.0.0");
    assert_eq!(later.error.as_deref(), Some("GitHub answered HTTP 502"));
    assert_eq!(later.status, UpdateState::UpdateAvailable);
}

#[tokio::test]
async fn a_check_that_outlasts_its_timeout_or_panics_is_an_error() {
    let mut source = Scripted::new(vec![Ok(release("v999.0.0", None))]);
    source.delay = Duration::from_millis(500);
    let timing = Timing { timeout: Duration::from_millis(50), ..LONG };
    let report = updates(source, std::path::Path::new("/nonexistent"), timing).report().await;
    assert_eq!(report.error.as_deref(), Some("Couldn't reach GitHub within 0.1 s"));

    let report = updates(Panics, std::path::Path::new("/nonexistent"), LONG).report().await;
    assert_eq!(report.error.as_deref(), Some("The update check failed"));
}

#[tokio::test]
async fn a_check_gets_the_timeout_for_each_of_its_two_requests() {
    let mut source = Scripted::new(vec![Ok(release("v999.0.0", None))]);
    source.delay = Duration::from_millis(300);
    let timing = Timing { timeout: Duration::from_millis(200), ..LONG };
    let report = updates(source, std::path::Path::new("/nonexistent"), timing).report().await;
    assert_eq!((report.error, report.status), (None, UpdateState::UpdateAvailable));
}

#[tokio::test]
async fn the_backup_slot_is_read_from_its_manifest() {
    let backup = tempfile::tempdir().unwrap();
    let checker = |source| updates(source, backup.path(), NONE);
    assert_eq!(checker(Scripted::new(vec![Err("x".into())])).report().await.backup, None);
    std::fs::write(backup.path().join("MANIFEST"), "version=0.2.0\ndate=2026-11-16T18:00:00Z\nfile=/x\n").unwrap();
    let report = checker(Scripted::new(vec![Err("x".into())])).report().await;
    assert_eq!(report.backup, Some(UpdateBackup { version: "0.2.0".into(), date: "2026-11-16T18:00:00Z".into() }));
}

async fn get_json(app: Router, path: &str) -> Value {
    let request = Request::builder().uri(path).body(Body::empty()).unwrap();
    let response = tower::ServiceExt::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    Contract::shared().validate_response("GET", path, 200, &ResponseBody::Json(body.clone())).unwrap();
    body
}

fn app(updates: Updates) -> Router {
    let (machine, _task) =
        spawn(Machine::new("h", wc_config::Settings::defaults()), Box::new(NoPort), Arc::new(NoHost));
    let paths = Paths::new(std::path::Path::new("/nonexistent-home"));
    router(AppState::new(machine, PingConfig::default(), paths).with_updates(updates))
}

#[tokio::test]
async fn the_route_answers_the_contract_and_system_info_badges_a_newer_release() {
    let backup = tempfile::tempdir().unwrap();
    std::fs::write(backup.path().join("MANIFEST"), "version=0.2.0\ndate=2026-11-16\n").unwrap();
    let source = Scripted::new(vec![Ok(LatestRelease { notes: "## Notes".into(), ..release("v999.0.0", Some(SHA)) })]);
    let checker = updates(source.clone(), backup.path(), LONG);

    let info = get_json(app(checker.clone()), "/api/system/info").await;
    assert_eq!(info["update"], json!({"available": false, "version": null}), "nothing is known yet");
    let deadline = std::time::Instant::now() + 5 * SECOND;
    while source.calls() == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(source.calls(), 1, "system info starts a check in the background");

    let body = get_json(app(checker.clone()), "/api/system/updates").await;
    assert_eq!(body["status"], "updateAvailable");
    assert_eq!(body["latest"]["notes"], "## Notes");
    assert_eq!(body["latest"]["commit"], SHA);
    assert_eq!(body["backup"], json!({"version": "0.2.0", "date": "2026-11-16"}));
    assert_eq!(body["error"], Value::Null);

    let info = get_json(app(checker), "/api/system/info").await;
    assert_eq!(info["update"], json!({"available": true, "version": "v999.0.0"}));
    assert_eq!(source.calls(), 1);
}

#[tokio::test]
async fn a_server_built_without_update_checks_says_so_and_from_env_builds_one() {
    let (machine, _task) =
        spawn(Machine::new("h", wc_config::Settings::defaults()), Box::new(NoPort), Arc::new(NoHost));
    let paths = Paths::new(std::path::Path::new("/nonexistent-home"));
    let body = get_json(router(AppState::new(machine, PingConfig::default(), paths)), "/api/system/updates").await;
    assert_eq!(body["error"], "Update checks are off in this server");
    assert_eq!((body["status"].as_str(), body["repo"].as_str()), (Some("unknown"), Some(DEFAULT_REPO)));
    let _ = Updates::from_env();
}
