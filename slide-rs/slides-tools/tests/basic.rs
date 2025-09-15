use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::process::Command;

#[test]
fn slides_write_create_markdown() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("slides")).unwrap();
    let status = Command::cargo_bin("slides_write")
        .expect("bin")
        .current_dir(root)
        .args(["--path", "slides/test.md", "--mode", "create", "--content", "# Hello\n"])
        .status()
        .unwrap();
    assert!(status.success());
    let s = fs::read_to_string(root.join("slides/test.md")).unwrap();
    assert!(s.contains("# Hello"));
}

#[test]
fn slides_apply_patch_reject_outside_slides() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("slides")).unwrap();
    let payload = "*** Begin Patch\n*** Add File: notes.md\n+OOPS\n*** End Patch\n";
    let output = Command::cargo_bin("slides_apply_patch")
        .expect("bin")
        .current_dir(root)
        .arg(payload)
        .output()
        .unwrap();
    assert!(!output.status.success());
}

