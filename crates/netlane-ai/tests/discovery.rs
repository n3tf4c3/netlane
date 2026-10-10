use netlane_ai::codex::{discover_in, validate_executable};
use std::{fs, path::Path};

fn fixture_file(path: &Path, data: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, data).unwrap();
}

#[test]
fn npm_shims_are_not_selected_and_native_vendor_layout_is_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture_file(&root.join("codex.ps1"), b"must never execute");
    fixture_file(&root.join("codex.cmd"), b"must never execute");
    assert_eq!(discover_in([root.to_owned()]), None);

    let package = root.join("node_modules/@openai/codex/package.json");
    let platform = if cfg!(target_arch = "aarch64") {
        "codex-win32-arm64"
    } else {
        "codex-win32-x64"
    };
    let triple = if cfg!(target_arch = "aarch64") {
        "aarch64-pc-windows-msvc"
    } else {
        "x86_64-pc-windows-msvc"
    };
    let native = root.join(format!(
        "node_modules/@openai/codex/node_modules/@openai/{platform}/vendor/{triple}/bin/codex.exe"
    ));
    fixture_file(&native, b"fixture, never executed");
    fixture_file(&package, br#"{"name":"another-package"}"#);
    assert_eq!(discover_in([root.to_owned()]), None);
    fixture_file(&package, br#"{"name":"@openai/codex"}"#);
    assert_eq!(discover_in([root.to_owned()]), Some(native.clone()));
    fixture_file(&package, &vec![b' '; 16 * 1024 + 1]);
    assert_eq!(discover_in([root.to_owned()]), None);
    assert!(validate_executable(&native).is_ok());
    assert!(validate_executable(&root.join("codex.ps1")).is_err());
    assert!(validate_executable(Path::new("codex.exe")).is_err());
}

#[test]
fn discovery_stops_after_64_path_entries() {
    let dir = tempfile::tempdir().unwrap();
    let directories: Vec<_> = (0..65).map(|n| dir.path().join(n.to_string())).collect();
    fixture_file(&directories[64].join("codex.exe"), b"fixture");
    assert_eq!(discover_in(directories.clone()), None);
    let native = directories[63].join("codex.exe");
    fixture_file(&native, b"fixture");
    assert_eq!(discover_in(directories), Some(native));
}
