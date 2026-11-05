// SPDX-License-Identifier: MIT

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "ptouch-zero-dpi-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("cannot create export fixture: {e}"),
            }
        }
    }

    fn export(&self, forced_width: bool, csv: bool) -> std::process::Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ptouch"));
        cmd.current_dir(&self.0)
            .args(["print", "--layout", "label.ptl", "--output"])
            .arg(if csv { "label-{n}.png" } else { "label.png" })
            .stdin(Stdio::null());
        if forced_width {
            cmd.args(["--tape-width", "76"]);
        }
        if csv {
            fs::write(self.0.join("rows.csv"), "unused\none\n").unwrap();
            cmd.args(["--csv", "rows.csv"]);
        }
        cmd.output().unwrap()
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn layout(dpi: Option<u16>) -> String {
    let dpi = dpi.map_or(String::new(), |dpi| format!("dpi = {dpi}\n"));
    format!(
        "version = 1\ntape_width_mm = 12\n{dpi}font_name = \"Inter\"\n\
         font_margin = 0\n[[elements]]\ntype = \"padding\"\npixels = 300\n"
    )
}

#[test]
fn zero_dpi_export_fails_before_overwriting_any_output() {
    for forced_width in [false, true] {
        for csv in [false, true] {
            let dir = TestDir::new();
            fs::write(dir.0.join("label.ptl"), layout(Some(0))).unwrap();
            let destination = dir.0.join(if csv { "label-1.png" } else { "label.png" });
            fs::write(&destination, b"existing-output-canary").unwrap();
            let output = dir.export(forced_width, csv);
            assert!(
                !output.status.success(),
                "zero DPI must not succeed: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("dpi must be greater than zero"), "{stderr}");
            assert_eq!(fs::read(destination).unwrap(), b"existing-output-canary");
        }
    }
}

#[test]
fn positive_and_legacy_dpi_exports_keep_dimensions_and_finite_lengths() {
    for (dpi, mm) in [(None, "42.3"), (Some(180), "42.3"), (Some(300), "25.4")] {
        for forced_width in [false, true] {
            let dir = TestDir::new();
            fs::write(dir.0.join("label.ptl"), layout(dpi)).unwrap();
            let output = dir.export(forced_width, false);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            assert!(stdout.contains(&format!("{mm} mm of tape")), "{stdout}");
            let png = fs::read(dir.0.join("label.png")).unwrap();
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
            assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 300);
            let expected_height = 76;
            assert_eq!(
                u32::from_be_bytes(png[20..24].try_into().unwrap()),
                expected_height
            );
        }
    }
}
