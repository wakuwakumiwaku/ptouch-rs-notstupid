// SPDX-License-Identifier: MIT

//! Exercise CSV placeholder binding through the real CLI, using offline exports only.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "ptouch-csv-headers-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("cannot create CSV test directory: {err}"),
            }
        }
    }

    fn layout(&self, content: &str) {
        // All test strings are simple TOML strings; no external images or fonts.
        fs::write(
            self.0.join("label.ptl"),
            format!(
                "version = 1\ntape_width_mm = 12\nfont_name = \"Inter\"\nfont_margin = 0\n\
                 [[elements]]\ntype = \"text\"\ncontent = \"{content}\"\n\
                 font_size = 18.0\nalign = \"left\"\nrotation = 0.0\n"
            ),
        )
        .unwrap();
    }

    fn run(&self, args: &[&str], stdin: Option<&str>) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ptouch"))
            .current_dir(&self.0)
            .args(["print", "--layout", "label.ptl", "--tape-width", "76"])
            .args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(input) = stdin {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
        }
        child.wait_with_output().unwrap()
    }

    fn assert_rejected(&self, output: Output, header: &str) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(1),
            "ambiguous CSV must fail, stdout: {}, stderr: {stderr}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            stderr.contains(&format!("duplicate CSV column name: {header:?}")),
            "error must identify the repeated header: {stderr}"
        );
        assert!(output.stdout.is_empty(), "must not report any saved rows");
        assert_eq!(
            fs::read(self.0.join("row-1.png")).unwrap(),
            b"existing destination must survive invalid headers"
        );
        assert!(!self.0.join("row-2.png").exists());
    }

    fn existing_output(&self) {
        fs::write(
            self.0.join("row-1.png"),
            b"existing destination must survive invalid headers",
        )
        .unwrap();
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn duplicate_file_columns_are_rejected_before_writing_any_row() {
    let dir = TestDir::new();
    dir.layout("Name={{name}}");
    dir.existing_output();
    fs::write(
        dir.0.join("input.csv"),
        "name,name\nAlice,Mallory\nBob,Eve\n",
    )
    .unwrap();
    let output = dir.run(&["--csv", "input.csv", "-o", "row-{n}.png"], None);
    dir.assert_rejected(output, "name");
}

#[test]
fn nonadjacent_stdin_duplicates_are_rejected_even_with_a_constant() {
    let dir = TestDir::new();
    dir.layout("{{id}} / {{name}}");
    dir.existing_output();
    let output = dir.run(
        &["--csv", "-", "--set", "id=CONST", "-o", "row-{n}.png"],
        Some("id,name,id\nA001,Alice,B999\n"),
    );
    dir.assert_rejected(output, "id");
}

#[test]
fn quoted_unused_duplicates_are_rejected_even_with_allow_missing() {
    let dir = TestDir::new();
    dir.layout("ID={{id}} / {{missing}}");
    dir.existing_output();
    let output = dir.run(
        &[
            "--csv",
            "-",
            "--allow-missing",
            "--set",
            "id=CONST",
            "-o",
            "row-{n}.png",
        ],
        Some("\"note\",id,\"note\"\nfirst,A001,second\n"),
    );
    dir.assert_rejected(output, "note");
}

#[test]
fn duplicate_headers_are_rejected_without_data_rows() {
    let dir = TestDir::new();
    dir.layout("Name={{name}}");
    dir.existing_output();
    let output = dir.run(&["--csv", "-", "-o", "row-{n}.png"], Some("name,name\n"));
    dir.assert_rejected(output, "name");
}

#[test]
fn unique_csv_rows_match_single_exports_and_preserve_precedence() {
    let dir = TestDir::new();
    dir.layout("Name={{name}} | {{id}} | {{dept}}");
    fs::write(
        dir.0.join("input.csv"),
        "name,id\r\n\"Alice, Q\",A001\r\n,B002\r\n",
    )
    .unwrap();
    let output = assert_success(dir.run(
        &[
            "--csv",
            "input.csv",
            "--set",
            "name=Default",
            "--set",
            "dept=Eng",
            "-o",
            "row-{n}.png",
        ],
        None,
    ));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Processed 2 row(s)"));

    for (row, name, id) in [(1, "Alice, Q", "A001"), (2, "", "B002")] {
        let expected = format!("expected-{row}.png");
        assert_success(dir.run(
            &[
                "--set",
                &format!("name={name}"),
                "--set",
                &format!("id={id}"),
                "--set",
                "dept=Eng",
                "-o",
                &expected,
            ],
            None,
        ));
        let actual = fs::read(dir.0.join(format!("row-{row}.png"))).unwrap();
        assert!(actual.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(actual, fs::read(dir.0.join(expected)).unwrap(), "row {row}");
    }
    assert!(!dir.0.join("row-3.png").exists());
}

#[test]
fn unique_headers_remain_case_sensitive_and_untrimmed() {
    let dir = TestDir::new();
    dir.layout("{{name}} / {{Name}}");
    assert_success(dir.run(
        &["--csv", "-", "-o", "row-{n}.png"],
        Some("name,Name, name \nlower,UPPER,unused\n"),
    ));
    assert_success(dir.run(
        &[
            "--set",
            "name=lower",
            "--set",
            "Name=UPPER",
            "-o",
            "expected.png",
        ],
        None,
    ));
    assert_eq!(
        fs::read(dir.0.join("row-1.png")).unwrap(),
        fs::read(dir.0.join("expected.png")).unwrap()
    );
}

#[test]
fn unique_header_only_csv_still_succeeds_without_output() {
    let dir = TestDir::new();
    dir.layout("Name={{name}}");
    let output = assert_success(dir.run(&["--csv", "-", "-o", "row-{n}.png"], Some("name\n")));
    assert!(String::from_utf8_lossy(&output.stderr).contains("CSV had no data rows"));
    assert!(output.stdout.is_empty());
    assert!(!dir.0.join("row-1.png").exists());
}
