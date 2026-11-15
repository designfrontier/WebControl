//! The gcode and board libraries under `~/.WebControl` (`File/safePaths.py`, `api/files.py`'s listing,
//! `File/retention.py`) and Werkzeug's `secure_filename`, which names every uploaded file.

use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use unicode_normalization::UnicodeNormalization;

/// The library's top-level directory; files in subdirectories carry the subdirectory's name.
pub const LIBRARY_ROOT: &str = ".";

/// A user-supplied path that is absolute or would leave its library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsafePath(pub String);

impl std::fmt::Display for UnsafePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} is outside the library", self.0)
    }
}

impl std::error::Error for UnsafePath {}

fn hex(byte: u8) -> Option<u8> {
    char::from(byte).to_digit(16).map(|d| d as u8)
}

/// Python's `urllib.parse.unquote`.
fn unquote(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped =
            (bytes[at] == b'%').then(|| Some(hex(*bytes.get(at + 1)?)? * 16 + hex(*bytes.get(at + 2)?)?)).flatten();
        match escaped {
            Some(byte) => {
                out.push(byte);
                at += 3;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn fully_decoded(text: &str) -> String {
    let decoded = unquote(text);
    if decoded == text { decoded } else { fully_decoded(&decoded) }
}

fn escapes(relative: &str) -> bool {
    [relative.to_string(), fully_decoded(relative)].iter().map(|variant| variant.replace('\\', "/")).any(|path| {
        path.starts_with('/') || path.chars().nth(1) == Some(':') || path.split('/').any(|part| part == "..")
    })
}

/// `os.path.normpath`, lexically.
fn normalised(path: &Path) -> PathBuf {
    path.components().fold(PathBuf::new(), |mut out, component| {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
        out
    })
}

/// `os.path.realpath`: symlinks resolved as far as the path exists.
fn real(path: &Path) -> PathBuf {
    match path.canonicalize() {
        Ok(resolved) => resolved,
        Err(_) => match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => real(parent).join(name),
            _ => path.to_path_buf(),
        },
    }
}

/// `resolveInside`: `parts` joined under `root`, refused if absolute, if it climbs out, or if a symlink leads out.
pub fn resolve_inside(root: &Path, parts: &[&str]) -> Result<PathBuf, UnsafePath> {
    let texts: Vec<&str> = parts.iter().copied().filter(|part| !part.is_empty()).collect();
    let relative = texts.join("/");
    if relative.contains('\0') || texts.iter().any(|text| escapes(text)) {
        return Err(UnsafePath(relative));
    }
    let candidate = normalised(&root.join(&relative));
    let (real_root, real_candidate) = (real(root), real(&candidate));
    if real_candidate != real_root && !real_candidate.starts_with(&real_root) {
        return Err(UnsafePath(relative));
    }
    Ok(candidate)
}

/// Werkzeug's `secure_filename`: ASCII letters, digits, `_`, `.` and `-`, with no path in it; may be empty.
pub fn secure_filename(name: &str) -> String {
    let ascii: String = name.nfkd().filter(char::is_ascii).collect();
    let spaced = ascii.replace('/', " ");
    // Python's str.split also splits on \x0b and the \x1c-\x1f separators.
    let python_space = |c: char| c.is_ascii_whitespace() || matches!(c, '\x0b' | '\x1c'..='\x1f');
    let joined = spaced.split(python_space).filter(|part| !part.is_empty()).collect::<Vec<_>>().join("_");
    let kept: String = joined.chars().filter(|c| c.is_ascii_alphanumeric() || "_.-".contains(*c)).collect();
    kept.trim_matches(['.', '_']).to_string()
}

/// `directory_setting`: the classic UI stores the root as `./` or `.`, subdirectories maybe with a slash.
pub fn directory_setting(stored: &str) -> String {
    match stored.trim_end_matches('/') {
        "" => LIBRARY_ROOT.into(),
        directory => directory.into(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryFile {
    pub directory: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub directories: Vec<String>,
    pub files: Vec<LibraryFile>,
}

fn visible(names: impl Iterator<Item = String>) -> Vec<String> {
    let mut names: Vec<String> = names.filter(|name| !name.starts_with('.')).collect();
    names.sort_by_key(|name| (name.to_lowercase(), name.clone()));
    names
}

fn entries(path: &Path, wanted: fn(&Path) -> bool) -> Vec<String> {
    let names = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| wanted(&entry.path()))
        .filter_map(|entry| entry.file_name().into_string().ok());
    visible(names)
}

/// `list_library`: files directly in `root` (directory `.`) and in each of its immediate subdirectories.
pub fn list_library(root: &Path) -> Listing {
    let directories: Vec<String> =
        std::iter::once(LIBRARY_ROOT.to_string()).chain(entries(root, Path::is_dir)).collect();
    let files = directories
        .iter()
        .flat_map(|directory| {
            let path = if directory == LIBRARY_ROOT { root.to_path_buf() } else { root.join(directory) };
            entries(&path, Path::is_file)
                .into_iter()
                .map(|name| LibraryFile { directory: directory.clone(), name })
                .collect::<Vec<_>>()
        })
        .collect();
    Listing { directories, files }
}

/// `keepNewest`: deletes all but the `keep` most recently modified files in `dir` named `<prefix>*<suffix>`.
pub fn keep_newest(dir: &Path, prefix: &str, suffix: &str, keep: usize) {
    let modified = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut matching: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_str().is_some_and(|n| n.starts_with(prefix) && n.ends_with(suffix)))
        .map(|entry| entry.path())
        .collect();
    matching.sort_by_key(|path| std::cmp::Reverse(modified(path)));
    matching.iter().skip(keep).for_each(|path| {
        let _ = std::fs::remove_file(path);
    });
}
