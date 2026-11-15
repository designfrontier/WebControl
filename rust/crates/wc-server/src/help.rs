//! `GET /api/system/help`: the embedded `docs/` pages as markdown and their images, as `api/system.py` serves them.

use std::path::Path;
use std::sync::LazyLock;

use axum::Json;
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use regex::Regex;
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::json;
use wc_config::library::resolve_inside;

use crate::assets::not_found_html;

#[derive(RustEmbed)]
#[folder = "../../../docs"]
struct Docs;

/// A root that never exists, so resolving a page under it is purely lexical.
const NOWHERE: &str = "/nonexistent-webcontrol-docs";

static BOUNDARY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^-{3,}\s*$").expect("boundary regex"));
static LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\s*site\.baseurl\s*\}\}\{%\s*link\s+(\S+?)\s*%\}").expect("link regex"));
static SPAN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"</?span[^>]*>").expect("span regex"));
static IMG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<img\s+src="([^"]+)"\s+alt="([^"]*)"[^>]*>"#).expect("img regex"));

/// python-frontmatter's YAML split: the `title`, if any, and the content after the front matter.
pub fn front_matter(text: &str) -> (Option<String>, String) {
    let text = text.trim();
    let pieces: Vec<&str> = BOUNDARY.splitn(text, 3).collect();
    let [before, yaml, content] = pieces[..] else {
        return (None, text.into());
    };
    if !before.is_empty() {
        return (None, text.into());
    }
    let title = yaml
        .lines()
        .find_map(|line| line.strip_prefix("title:"))
        .map(|value| value.trim().trim_matches(|c| c == '\'' || c == '"').to_string())
        .filter(|title| !title.is_empty());
    (title, content.trim().into())
}

/// `clean_markdown`: Jekyll links, labels, `</br>`, spans and `<img>` tags made plain markdown.
pub fn clean_markdown(text: &str) -> String {
    let linked = LINK.replace_all(text, "$1");
    let labelled = linked.replace("{: .label .label-blue }", "").replace('Â', "").replace("</br>", "  ");
    IMG.replace_all(&SPAN.replace_all(&labelled, ""), "![$2]($1)").into_owned()
}

/// The embedded file a request names, refused as `resolveInside` refuses it.
fn embedded(path: &str) -> Option<(String, rust_embed::EmbeddedFile)> {
    let resolved = resolve_inside(Path::new(NOWHERE), &[path]).ok()?;
    let key = resolved.strip_prefix(NOWHERE).ok()?.to_str()?.to_string();
    Docs::get(&key).map(|file| (key, file))
}

#[derive(Deserialize)]
pub(crate) struct HelpQuery {
    path: Option<String>,
}

pub(crate) async fn page(Query(query): Query<HelpQuery>) -> Response {
    let path = query.path.filter(|path| !path.is_empty()).unwrap_or_else(|| "index.md".into());
    let Some((key, file)) = embedded(&path).filter(|(key, _)| key.ends_with(".md")) else {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "no such help page"}))).into_response();
    };
    let (title, content) = front_matter(&String::from_utf8_lossy(&file.data));
    let stem = || Path::new(&key).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    Json(json!({"path": path, "title": title.unwrap_or_else(stem), "markdown": clean_markdown(&content)}))
        .into_response()
}

pub(crate) fn asset(path: &str) -> Response {
    match embedded(&format!("assets/{path}")) {
        Some((_, file)) => ([(header::CONTENT_TYPE, file.metadata.mimetype().to_string())], file.data).into_response(),
        None => not_found_html(),
    }
}
