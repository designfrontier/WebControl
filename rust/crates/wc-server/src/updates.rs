//! `GET /api/system/updates`: the installed build, the latest published GitHub release and the backup
//! `webcontrol-update` keeps. Updating itself stays with that script on the Pi: it needs sudo and restarts us.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::State;
use serde_json::Value;
use tokio::sync::Mutex;
use wc_protocol::system::{InstalledBuild, LatestRelease, UpdateBackup, UpdateState, UpdateStatus, Updates as Reply};

use crate::AppState;

/// `webcontrol-update`'s defaults, and the environment variables it reads them from.
pub const DEFAULT_REPO: &str = "designfrontier/WebControl";
pub const GITHUB_API: &str = "https://api.github.com";
pub const BACKUP_DIR: &str = "/opt/webcontrol-rs/backup";
const BODY_LIMIT: u64 = 1 << 20;

/// Where the latest published release comes from. It blocks, so it runs on a blocking thread.
pub trait ReleaseSource: Send + Sync + 'static {
    fn latest(&self) -> Result<LatestRelease, String>;
}

/// What a server built for tests asks: nothing.
struct Off;

impl ReleaseSource for Off {
    fn latest(&self) -> Result<LatestRelease, String> {
        Err("Update checks are off in this server".into())
    }
}

/// The GitHub REST API, unauthenticated unless given a token.
pub struct GitHub {
    pub api: String,
    pub repo: String,
    pub token: Option<String>,
    pub timeout: Duration,
}

fn valid_tag(tag: &str) -> bool {
    let mut chars = tag.strip_prefix('v').unwrap_or(tag).chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || ".+_-".contains(c))
}

fn is_sha(text: &str) -> bool {
    text.len() >= 7 && text.chars().all(|c| c.is_ascii_hexdigit())
}

impl GitHub {
    fn get(&self, path: &str, accept: &str) -> Result<(u16, String), String> {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(self.timeout))
            .http_status_as_error(false)
            .user_agent(format!("webcontrol-rs/{}", crate::VERSION))
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let url = format!("{}{path}", self.api.trim_end_matches('/'));
        let request = agent.get(&url).header("Accept", accept).header("X-GitHub-Api-Version", "2022-11-28");
        let request = match &self.token {
            Some(token) => request.header("Authorization", format!("Bearer {token}")),
            None => request,
        };
        let mut response = request.call().map_err(|error| format!("Couldn't reach GitHub: {error}"))?;
        let status = response.status().as_u16();
        let body = response.body_mut().with_config().limit(BODY_LIMIT).read_to_string();
        Ok((status, body.map_err(|error| format!("Couldn't read GitHub's answer: {error}"))?))
    }

    /// The full SHA the tag points at, or `None` if GitHub would not say.
    fn tag_commit(&self, tag: &str) -> Option<String> {
        let path = format!("/repos/{}/commits/{tag}", self.repo);
        let (status, body) = self.get(&path, "application/vnd.github.sha").ok()?;
        Some(body.trim().to_lowercase()).filter(|sha| status == 200 && is_sha(sha))
    }
}

impl ReleaseSource for GitHub {
    fn latest(&self) -> Result<LatestRelease, String> {
        let path = format!("/repos/{}/releases/latest", self.repo);
        let (status, body) = self.get(&path, "application/vnd.github+json")?;
        let release = match status {
            200 => parse_release(&body)?,
            404 => return Err(format!("{} has no published release yet", self.repo)),
            403 | 429 => return Err(format!("GitHub refused (HTTP {status}), most likely its hourly rate limit")),
            other => return Err(format!("GitHub answered HTTP {other}")),
        };
        Ok(LatestRelease { commit: self.tag_commit(&release.tag), ..release })
    }
}

/// A `releases/latest` answer; the commit is looked up separately.
pub fn parse_release(body: &str) -> Result<LatestRelease, String> {
    let json: Value = serde_json::from_str(body).map_err(|_| "GitHub's answer was not JSON".to_string())?;
    let text = |key: &str| json.get(key).and_then(Value::as_str).map(str::to_string).filter(|s| !s.is_empty());
    let tag = text("tag_name").filter(|tag| valid_tag(tag)).ok_or("The latest release has no version tag")?;
    Ok(LatestRelease {
        name: text("name").unwrap_or_else(|| tag.clone()),
        published_at: text("published_at"),
        url: text("html_url").ok_or("The latest release has no web address")?,
        notes: text("body").unwrap_or_default().replace("\r\n", "\n"),
        commit: None,
        tag,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Identifier {
    Number(u64),
    Text(String),
}

/// A pre-release sorts before its release.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Pre {
    Pre(Vec<Identifier>),
    Release,
}

/// A version ordered as `webcontrol-update` orders them: `MAJOR[.MINOR[.PATCH]]`, an optional `v`, build metadata
/// ignored, and a `git describe` build (`-3-gabc1234`, `-dirty`) after the tag it describes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    core: [u64; 3],
    pre: Pre,
    commits_past: Option<u64>,
}

fn number(text: &str) -> Option<u64> {
    text.chars().all(|c| c.is_ascii_digit()).then(|| text.parse().ok()).flatten()
}

fn identifier(text: &str) -> Option<Identifier> {
    match number(text) {
        Some(n) => Some(Identifier::Number(n)),
        None if !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') => {
            Some(Identifier::Text(text.into()))
        }
        None => None,
    }
}

/// Splits `rc.1-3-gabc1234-dirty` into `rc.1` and the commit count, 3.
fn split_describe(suffix: &str) -> (&str, Option<u64>) {
    let (suffix, dirty) = match suffix.strip_suffix("dirty") {
        Some(rest) if rest.is_empty() || rest.ends_with('-') => (rest.strip_suffix('-').unwrap_or(rest), Some(0)),
        _ => (suffix, None),
    };
    let described = suffix.rsplit_once("-g").and_then(|(head, hash)| {
        let hex = !hash.is_empty() && hash.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'));
        let (pre, count) = head.rsplit_once('-').unwrap_or(("", head));
        Some((pre, number(count)?)).filter(|_| hex)
    });
    described.map_or((suffix, dirty), |(pre, count)| (pre, Some(count)))
}

fn pre(text: Option<&str>) -> Option<Pre> {
    let Some(text) = text else { return Some(Pre::Release) };
    text.split('.').map(identifier).collect::<Option<_>>().map(Pre::Pre)
}

impl Version {
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let text = text.split_once('+').map_or(text, |(version, _)| version);
        let (core, suffix) = text.split_once('-').map_or((text, None), |(core, suffix)| (core, Some(suffix)));
        let mut core: Vec<u64> = core.split('.').map(number).collect::<Option<_>>()?;
        if core.len() > 3 {
            return None;
        }
        core.resize(3, 0);
        let (pre_release, commits_past) = suffix.map_or((None, None), |suffix| {
            let (pre_release, commits_past) = split_describe(suffix);
            (Some(pre_release).filter(|p| !p.is_empty() || commits_past.is_none()), commits_past)
        });
        Some(Version { core: core.try_into().ok()?, pre: pre(pre_release)?, commits_past })
    }
}

/// The installed build against the latest release; the same version from another commit is a dev build.
pub fn compare(version: &str, commit: &str, latest: Option<&LatestRelease>) -> UpdateState {
    let Some(latest) = latest else { return UpdateState::Unknown };
    let (Some(installed), Some(released)) = (Version::parse(version), Version::parse(&latest.tag)) else {
        return UpdateState::Unknown;
    };
    let other_commit = latest.commit.as_deref().is_some_and(|sha| is_sha(commit) && !sha.starts_with(commit));
    match installed.cmp(&released) {
        Ordering::Less => UpdateState::UpdateAvailable,
        Ordering::Greater => UpdateState::Ahead,
        Ordering::Equal if other_commit => UpdateState::DevBuild,
        Ordering::Equal => UpdateState::UpToDate,
    }
}

/// `webcontrol-update`'s `MANIFEST`: `version=`, `date=` and `file=` lines.
pub fn parse_manifest(text: &str) -> Option<UpdateBackup> {
    let value = |key: &str| text.lines().find_map(|line| line.strip_prefix(key)).map(|v| v.trim().to_string());
    Some(UpdateBackup {
        version: value("version=").filter(|v| !v.is_empty())?,
        date: value("date=").unwrap_or_default(),
    })
}

/// The repository releases come from: `WEBCONTROL_REPO` if it reads `owner/name`.
pub fn repo_from(value: Option<String>) -> String {
    let part =
        |p: &str| p.chars().any(|c| c != '.') && p.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    value
        .filter(|repo| repo.split_once('/').is_some_and(|(owner, name)| part(owner) && part(name)))
        .unwrap_or_else(|| DEFAULT_REPO.into())
}

/// How long answers are kept and how long each request to GitHub gets.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub fresh: Duration,
    pub retry: Duration,
    pub timeout: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing { fresh: Duration::from_secs(600), retry: Duration::from_secs(30), timeout: Duration::from_secs(5) }
    }
}

#[derive(Clone)]
struct Checked {
    at: Instant,
    checked_at: u64,
    latest: Option<LatestRelease>,
    error: Option<String>,
}

struct Checker {
    source: Arc<dyn ReleaseSource>,
    repo: String,
    backup: PathBuf,
    timing: Timing,
    last: Mutex<Option<Checked>>,
}

/// The cached update check, shared by every request.
#[derive(Clone)]
pub struct Updates(Arc<Checker>);

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis() as u64)
}

impl Updates {
    pub fn new(source: impl ReleaseSource, repo: &str, backup: &Path, timing: Timing) -> Updates {
        let source = Arc::new(source);
        let last = Mutex::new(None);
        Updates(Arc::new(Checker { source, repo: repo.into(), backup: backup.into(), timing, last }))
    }

    pub fn off() -> Updates {
        Updates::new(Off, DEFAULT_REPO, Path::new(BACKUP_DIR), Timing::default())
    }

    /// As `webcontrol-update` is configured: `WEBCONTROL_REPO`, `WC_GITHUB_API` and `GITHUB_TOKEN`.
    pub fn from_env() -> Updates {
        let env = |key: &str| std::env::var(key).ok().filter(|v| !v.is_empty());
        let (repo, timing) = (repo_from(env("WEBCONTROL_REPO")), Timing::default());
        let api = env("WC_GITHUB_API").unwrap_or_else(|| GITHUB_API.into());
        let github = GitHub { api, repo: repo.clone(), token: env("GITHUB_TOKEN"), timeout: timing.timeout };
        Updates::new(github, &repo, Path::new(BACKUP_DIR), timing)
    }

    fn stale(&self, checked: &Checked) -> bool {
        let keep = if checked.error.is_none() { self.0.timing.fresh } else { self.0.timing.retry };
        checked.at.elapsed() >= keep
    }

    /// The last check if it is fresh, else a new one; a failed check keeps the release found before it.
    async fn check(&self) -> Checked {
        let mut last = self.0.last.lock().await;
        if let Some(checked) = last.as_ref().filter(|checked| !self.stale(checked)) {
            return checked.clone();
        }
        let source = self.0.source.clone();
        let fetch = tokio::task::spawn_blocking(move || source.latest());
        // Two requests, the release then its tag's commit, each get the timeout.
        let wait = self.0.timing.timeout * 2;
        let result = match tokio::time::timeout(wait, fetch).await {
            Ok(joined) => joined.unwrap_or_else(|_| Err("The update check failed".into())),
            Err(_) => Err(format!("Couldn't reach GitHub within {} s", wait.as_secs_f32())),
        };
        let before = last.take().and_then(|checked| checked.latest);
        let (latest, error) = match result {
            Ok(release) => (Some(release), None),
            Err(error) => (before, Some(error)),
        };
        last.insert(Checked { at: Instant::now(), checked_at: now_ms(), latest, error }).clone()
    }

    pub async fn report(&self) -> Reply {
        let checked = self.check().await;
        let manifest = tokio::fs::read_to_string(self.0.backup.join("MANIFEST")).await.ok();
        Reply {
            installed: InstalledBuild { version: crate::VERSION.into(), commit: crate::COMMIT.into() },
            repo: self.0.repo.clone(),
            status: compare(crate::VERSION, crate::COMMIT, checked.latest.as_ref()),
            latest: checked.latest,
            error: checked.error,
            checked_at: Some(checked.checked_at),
            backup: manifest.as_deref().and_then(parse_manifest),
        }
    }

    /// From the last check only, never waiting; a missing or stale one is refreshed in the background.
    pub fn status(&self) -> UpdateStatus {
        let last = self.0.last.try_lock().ok().and_then(|last| last.clone());
        if last.as_ref().is_none_or(|checked| self.stale(checked)) {
            let updates = self.clone();
            tokio::spawn(async move { updates.check().await });
        }
        let newer = last
            .and_then(|checked| checked.latest)
            .filter(|latest| compare(crate::VERSION, crate::COMMIT, Some(latest)) == UpdateState::UpdateAvailable);
        UpdateStatus { available: newer.is_some(), version: newer.map(|latest| latest.tag) }
    }
}

pub(crate) async fn updates(State(state): State<AppState>) -> Json<Reply> {
    Json(state.updates.report().await)
}
