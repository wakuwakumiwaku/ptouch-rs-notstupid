// SPDX-License-Identifier: MIT

//! Report physical lengths using the saved layout resolution; never open a device.

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
                "ptouch-export-dpi-{}-{}",
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

    fn export(&self, dpi: Option<u16>, forced_width: bool) -> String {
        let dpi_field = dpi.map_or(String::new(), |dpi| format!("dpi = {dpi}\n"));
        fs::write(
            self.0.join("label.ptl"),
            format!(
                "version = 1\ntape_width_mm = 12\n{dpi_field}font_name = \"Inter\"\n\
                 font_margin = 0\n[[elements]]\ntype = \"padding\"\npixels = 300\n"
            ),
        )
        .unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ptouch"));
        cmd.current_dir(&self.0)
            .args(["print", "--layout", "label.ptl", "--output", "label.png"])
            .stdin(Stdio::null());
        if forced_width {
            cmd.args(["--tape-width", "76"]);
        }
        let output = cmd.output().unwrap();
        assert!(
            output.status.success(),
            "export failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let png = fs::read(self.0.join("label.png")).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 300);
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn saved_resolution_controls_reported_export_length() {
    for forced_width in [false, true] {
        let dir = TestDir::new();
        let report = dir.export(Some(300), forced_width);
        assert!(report.contains("25.4 mm of tape"), "{report}");
    }
}

#[test]
fn legacy_and_explicit_180_dpi_layouts_keep_their_length() {
    for dpi in [None, Some(180)] {
        let dir = TestDir::new();
        let report = dir.export(dpi, false);
        assert!(report.contains("42.3 mm of tape"), "{report}");
    }
}
