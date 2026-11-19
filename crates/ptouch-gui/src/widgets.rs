// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Huang Rui <vowstar@gmail.com>

//! Custom egui widgets and shared file dialog helpers.

use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};

/// If `rfd` returns `None` faster than this, assume no desktop portal was
/// available rather than a user cancelling the dialog.
#[cfg(target_os = "linux")]
const PORTAL_MISSING_THRESHOLD: Duration = Duration::from_millis(200);

/// Run a `zenity` command first; only try `kdialog` if `zenity` is not installed.
/// A non-zero exit code means the user cancelled the dialog, so we stop there.
#[cfg(target_os = "linux")]
fn linux_cli_dialog_fallback(
    zenity_args: &[&str],
    kdialog_args: &[&str],
    default_ext: Option<&str>,
) -> Option<PathBuf> {
    let parse_stdout = |bytes: &[u8]| -> Option<PathBuf> {
        let s = String::from_utf8_lossy(bytes).trim().to_string();
        if s.is_empty() {
            return None;
        }
        let mut p = PathBuf::from(s);
        if let Some(ext) = default_ext
            && p.extension().is_none()
        {
            p = p.with_extension(ext);
        }
        Some(p)
    };

    if let Ok(output) = std::process::Command::new("zenity")
        .args(zenity_args)
        .output()
    {
        return if output.status.success() {
            parse_stdout(&output.stdout)
        } else {
            None
        };
    }

    if let Ok(output) = std::process::Command::new("kdialog")
        .args(kdialog_args)
        .output()
        && output.status.success()
    {
        return parse_stdout(&output.stdout);
    }

    None
}

fn run_dialog_with_linux_fallback<F>(
    native: F,
    zenity_args: &[&str],
    kdialog_args: &[&str],
    default_ext: Option<&str>,
) -> Option<PathBuf>
where
    F: FnOnce() -> Option<PathBuf>,
{
    #[cfg(target_os = "linux")]
    let started = Instant::now();

    if let Some(path) = native() {
        return Some(path);
    }

    #[cfg(target_os = "linux")]
    {
        if started.elapsed() < PORTAL_MISSING_THRESHOLD {
            return linux_cli_dialog_fallback(zenity_args, kdialog_args, default_ext);
        }
    }

    #[cfg(not(target_os = "linux"))]
    let _ = (zenity_args, kdialog_args, default_ext);

    None
}

/// Build an image file dialog with per-format filters.
///
/// The first filter is "All Images" (default), followed by individual
/// format groups so the user can narrow down if needed.
pub fn image_file_dialog() -> rfd::FileDialog {
    rfd::FileDialog::new()
        .add_filter(
            "All Images",
            &[
                "png", "jpg", "jpeg", "gif", "bmp", "tiff", "tif", "webp", "ico", "pnm", "tga",
                "qoi", "svg", "svgz",
            ],
        )
        .add_filter("PNG", &["png"])
        .add_filter("JPEG", &["jpg", "jpeg"])
        .add_filter("SVG", &["svg", "svgz"])
        .add_filter("GIF", &["gif"])
        .add_filter("BMP", &["bmp"])
        .add_filter("TIFF", &["tiff", "tif"])
        .add_filter("WebP", &["webp"])
        .add_filter("ICO", &["ico"])
        .add_filter("QOI", &["qoi"])
        .add_filter("PNM", &["pnm"])
        .add_filter("TGA", &["tga"])
}

/// Pick an image file with Linux fallback to zenity or kdialog when no portal is available.
pub fn pick_image_file() -> Option<PathBuf> {
    run_dialog_with_linux_fallback(
        || image_file_dialog().pick_file(),
        &[
            "--file-selection",
            "--title=Choose Image",
            "--file-filter=Images | *.png *.jpg *.jpeg *.svg *.svgz *.bmp *.gif *.webp",
            "--file-filter=All Files | *",
        ],
        &[
            "--getopenfilename",
            ".",
            "*.png *.jpg *.jpeg *.svg *.svgz *.bmp *.gif *.webp|Images",
        ],
        None,
    )
}

/// Save an image file with Linux fallback to zenity or kdialog when no portal is available.
pub fn save_image_file() -> Option<PathBuf> {
    run_dialog_with_linux_fallback(
        || {
            rfd::FileDialog::new()
                .add_filter("PNG", &["png"])
                .add_filter("JPEG", &["jpg", "jpeg"])
                .add_filter("BMP", &["bmp"])
                .add_filter("GIF", &["gif"])
                .add_filter("TIFF", &["tiff", "tif"])
                .add_filter("WebP", &["webp"])
                .set_file_name("label.png")
                .save_file()
        },
        &[
            "--file-selection",
            "--save",
            "--confirm-overwrite",
            "--title=Export Image",
            "--filename=label.png",
            "--file-filter=PNG (*.png) | *.png",
            "--file-filter=JPEG (*.jpg) | *.jpg",
        ],
        &["--getsavefilename", "label.png", "*.png|PNG Image (*.png)"],
        Some("png"),
    )
}

/// Build a file dialog filtered to layout files (`.ptl`).
pub fn layout_file_dialog() -> rfd::FileDialog {
    rfd::FileDialog::new().add_filter("Layout", &["ptl"])
}

/// Pick a layout file (`.ptl`).
pub fn pick_layout_file() -> Option<PathBuf> {
    run_dialog_with_linux_fallback(
        || layout_file_dialog().pick_file(),
        &[
            "--file-selection",
            "--title=Open Layout",
            "--file-filter=Layout (*.ptl) | *.ptl",
            "--file-filter=All Files | *",
        ],
        &["--getopenfilename", ".", "*.ptl|Layout files (*.ptl)"],
        None,
    )
}

/// Save a layout file (`.ptl`).
pub fn save_layout_file() -> Option<PathBuf> {
    run_dialog_with_linux_fallback(
        || layout_file_dialog().set_file_name("label.ptl").save_file(),
        &[
            "--file-selection",
            "--save",
            "--confirm-overwrite",
            "--title=Save Layout",
            "--filename=label.ptl",
            "--file-filter=Layout (*.ptl) | *.ptl",
        ],
        &[
            "--getsavefilename",
            "label.ptl",
            "*.ptl|Layout files (*.ptl)",
        ],
        Some("ptl"),
    )
}

/// Pick a batch (`.ptb`) or single layout (`.ptl`) file.
pub fn pick_batch_file() -> Option<PathBuf> {
    run_dialog_with_linux_fallback(
        || {
            rfd::FileDialog::new()
                .add_filter("Batch or Layout", &["ptb", "ptl"])
                .add_filter("Batch", &["ptb"])
                .add_filter("Layout", &["ptl"])
                .pick_file()
        },
        &[
            "--file-selection",
            "--title=Open Batch",
            "--file-filter=Batch & Layout (*.ptb *.ptl) | *.ptb *.ptl",
            "--file-filter=All Files | *",
        ],
        &[
            "--getopenfilename",
            ".",
            "*.ptb *.ptl|Batch and Layout files (*.ptb *.ptl)",
        ],
        None,
    )
}

/// Save a batch file (`.ptb`).
pub fn save_batch_file() -> Option<PathBuf> {
    run_dialog_with_linux_fallback(
        || {
            rfd::FileDialog::new()
                .add_filter("Batch", &["ptb"])
                .set_file_name("batch.ptb")
                .save_file()
        },
        &[
            "--file-selection",
            "--save",
            "--confirm-overwrite",
            "--title=Save Batch",
            "--filename=batch.ptb",
            "--file-filter=Batch (*.ptb) | *.ptb",
        ],
        &[
            "--getsavefilename",
            "batch.ptb",
            "*.ptb|Batch files (*.ptb)",
        ],
        Some("ptb"),
    )
}

/// Theme-aware accent colors so text remains crisp on both dark and light UI surfaces.
pub fn accent_blue(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(125, 196, 255)
    } else {
        egui::Color32::from_rgb(30, 64, 175)
    }
}

pub fn accent_green(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(110, 231, 150)
    } else {
        egui::Color32::from_rgb(22, 101, 52)
    }
}

pub fn accent_amber(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(251, 168, 92)
    } else {
        egui::Color32::from_rgb(194, 65, 12)
    }
}

pub fn accent_red(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(248, 113, 113)
    } else {
        egui::Color32::from_rgb(200, 38, 38)
    }
}
