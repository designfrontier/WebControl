//! Ported from test_safe_paths (the resolver), test_api_files (the listing) and test_log_retention (keepNewest).

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use wc_config::library::{
    LibraryFile, Listing, UnsafePath, directory_setting, keep_newest, list_library, resolve_inside, secure_filename,
};

const ESCAPES: [&str; 9] = [
    "../outside.nc",
    "jobs/../../outside.nc",
    "/etc/passwd",
    "..\\outside.nc",
    "%2e%2e/outside.nc",
    "..%2foutside.nc",
    "%252e%252e%252foutside.nc",
    "C:\\Windows\\win.ini",
    "jobs/\0.nc",
];

#[test]
fn paths_inside_the_root_resolve_under_it() {
    let root = tempfile::tempdir().unwrap();
    for relative in ["part.nc", "./part.nc", "jobs/part.nc", ".", "", "a..b.nc", "%zz.nc", "100%.nc"] {
        let resolved = resolve_inside(root.path(), &[relative]).unwrap();
        assert!(resolved.starts_with(root.path()), "{relative}: {}", resolved.display());
    }
    assert_eq!(resolve_inside(root.path(), &["./", "part.nc"]).unwrap(), root.path().join("part.nc"));
    assert_eq!(resolve_inside(root.path(), &["jobs/./x/", "", "part.nc"]).unwrap(), root.path().join("jobs/x/part.nc"));
}

#[test]
fn paths_that_leave_the_root_are_refused() {
    let root = tempfile::tempdir().unwrap();
    ESCAPES.iter().for_each(|relative| {
        assert!(resolve_inside(root.path(), &[relative]).is_err(), "{relative:?}");
    });
    assert_eq!(resolve_inside(root.path(), &["..", "part.nc"]), Err(UnsafePath("../part.nc".into())));
    assert_eq!(UnsafePath("../x".into()).to_string(), "\"../x\" is outside the library");
}

#[cfg(unix)]
#[test]
fn symlinks_that_lead_out_of_the_root_are_refused() {
    let base = tempfile::tempdir().unwrap();
    let (root, elsewhere) = (base.path().join("root"), base.path().join("elsewhere"));
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(elsewhere.join("secret.nc"), "G0 X1\n").unwrap();
    std::os::unix::fs::symlink(&elsewhere, root.join("link")).unwrap();
    assert!(resolve_inside(&root, &["link/secret.nc"]).is_err());
    assert!(resolve_inside(&root, &["link"]).is_err());
    assert!(resolve_inside(&root, &["link/new.nc"]).is_err());
    std::os::unix::fs::symlink(root.join("jobs"), root.join("inner")).unwrap();
    assert!(resolve_inside(&root, &["inner/new.nc"]).is_ok());
}

/// Outputs of werkzeug.utils.secure_filename.
#[test]
fn secure_filenames_match_werkzeug() {
    let cases = [
        (" My Parts/../x ", "My_Parts_.._x"),
        ("../", ""),
        ("  ", ""),
        ("é café.nc", "e_cafe.nc"),
        ("a\\b.nc", "ab.nc"),
        ("..hidden", "hidden"),
        ("x__y", "x__y"),
        ("ﬁle.nc", "file.nc"),
        ("a/b/c.nc", "a_b_c.nc"),
        ("file name (1).nc", "file_name_1.nc"),
        ("\t tab\nnew.nc", "tab_new.nc"),
        ("../../../etc/passwd", "etc_passwd"),
    ];
    cases.iter().for_each(|(name, expected)| assert_eq!(secure_filename(name), *expected, "{name:?}"));
}

#[test]
fn the_stored_directory_is_normalised() {
    [("", "."), ("./", "."), (".", "."), ("parts/", "parts"), ("parts", "parts")]
        .iter()
        .for_each(|(stored, expected)| assert_eq!(directory_setting(stored), *expected, "{stored:?}"));
}

fn write(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "").unwrap();
}

#[test]
fn the_listing_has_root_and_subdirectory_files_sorted_without_hidden_entries() {
    let root = tempfile::tempdir().unwrap();
    ["b.nc", "A.nc", ".DS_Store", "parts/leg.nc", ".git/config", "parts/deeper/no.nc"]
        .iter()
        .for_each(|name| write(&root.path().join(name)));
    let file = |directory: &str, name: &str| LibraryFile { directory: directory.into(), name: name.into() };
    assert_eq!(
        list_library(root.path()),
        Listing {
            directories: vec![".".into(), "parts".into()],
            files: vec![file(".", "A.nc"), file(".", "b.nc"), file("parts", "leg.nc")],
        }
    );
}

#[test]
fn a_missing_library_lists_only_the_root() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(list_library(&root.path().join("missing")), Listing { directories: vec![".".into()], files: vec![] });
}

#[test]
fn keep_newest_removes_all_but_the_newest_matching_files() {
    let dir = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    (0..7).for_each(|index| {
        let path = dir.path().join(format!("wc_backup_{index}.zip"));
        fs::write(&path, "").unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(now - Duration::from_secs(100 - index))
            .unwrap();
    });
    fs::write(dir.path().join("other.zip"), "").unwrap();
    keep_newest(dir.path(), "wc_backup_", ".zip", 5);
    let mut left: Vec<String> =
        fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    left.sort();
    assert_eq!(
        left,
        ["other.zip", "wc_backup_2.zip", "wc_backup_3.zip", "wc_backup_4.zip", "wc_backup_5.zip", "wc_backup_6.zip"]
    );
    keep_newest(&dir.path().join("missing"), "wc_backup_", ".zip", 5);
}
