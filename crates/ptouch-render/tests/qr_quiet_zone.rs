// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 ptouch-rs-notstupid contributors

//! Check QR geometry through the public renderer and saved-layout APIs.

use ptouch_render::bitmap::LabelBitmap;
use ptouch_render::document::{LabelDocument, render_elements};
use ptouch_render::qr::render_qr_code;
use ptouch_render::text::TextRenderer;
use qrcode::{Color, QrCode};

fn ink_bounds(bitmap: &LabelBitmap) -> (u32, u32, u32, u32) {
    let (mut left, mut top) = (bitmap.width(), bitmap.height());
    let (mut right, mut bottom) = (0, 0);
    for y in 0..bitmap.height() {
        for x in 0..bitmap.width() {
            if bitmap.get_pixel(x, y) {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
            }
        }
    }
    assert!(
        left <= right && top <= bottom,
        "QR must contain dark modules"
    );
    (left, top, right, bottom)
}

fn assert_four_module_border(bitmap: &LabelBitmap, modules: u32) {
    let (left, top, right, bottom) = ink_bounds(bitmap);
    // Finder patterns reach both horizontal and vertical matrix edges, so the
    // ink span measures scale independently of the renderer's border choice.
    let scale = (right - left + 1) / modules;
    assert!(scale >= 1);
    assert_eq!(right - left + 1, modules * scale);
    assert_eq!(bottom - top + 1, modules * scale);
    assert_eq!(
        [
            left,
            top,
            bitmap.width() - right - 1,
            bitmap.height() - bottom - 1
        ],
        [4 * scale; 4],
        "QR quiet zone must be four full modules on every side"
    );
}

#[test]
fn generated_qr_codes_have_four_module_quiet_zones() {
    for content in [
        "ASSET-42",
        "https://inventory.example.test/racks/A-01?asset=1234567890",
    ] {
        let modules = QrCode::new(content.as_bytes()).unwrap().width() as u32;
        for height in [76, 128, 29, 256] {
            let bitmap = render_qr_code(content, height).unwrap();
            assert_four_module_border(&bitmap, modules);
            let scale = (height / (modules + 8)).max(1);
            assert_eq!(bitmap.width(), (modules + 8) * scale);
            assert_eq!(bitmap.height(), bitmap.width());
        }
    }
}

#[test]
fn encoded_modules_remain_verbatim_and_integer_scaled() {
    for content in [
        "ASSET-42",
        " ASSET-42 ",
        "\tASSET-42\r\n",
        "\u{2003}ASSET-42\u{a0}",
    ] {
        let code = QrCode::new(content.as_bytes()).unwrap();
        let modules = code.width() as u32;
        for height in [76, 128, 256] {
            let bitmap = render_qr_code(content, height).unwrap();
            let (left, top, right, bottom) = ink_bounds(&bitmap);
            let scale = (right - left + 1) / modules;
            assert!(scale >= 1);
            assert_eq!(right - left + 1, modules * scale);
            assert_eq!(bottom - top + 1, modules * scale);
            assert_eq!(bitmap.width(), bitmap.height());
            assert!(bitmap.height() <= height);
            for y in 0..bitmap.height() {
                for x in 0..bitmap.width() {
                    let in_matrix = (left..=right).contains(&x) && (top..=bottom).contains(&y);
                    let expected = in_matrix
                        && code[(((x - left) / scale) as usize, ((y - top) / scale) as usize)]
                            == Color::Dark;
                    assert_eq!(bitmap.get_pixel(x, y), expected, "{content:?}: ({x}, {y})");
                }
            }
        }
    }
}

#[test]
fn blank_payload_errors_and_tiny_target_fallback_are_unchanged() {
    for content in ["", " \t\r\n", "\u{2003}"] {
        assert!(render_qr_code(content, 76).is_err());
    }
    let modules = QrCode::new(b"ASSET-42").unwrap().width() as u32;
    for height in [0, 1, 10] {
        let bitmap = render_qr_code("ASSET-42", height).unwrap();
        let (left, top, right, bottom) = ink_bounds(&bitmap);
        assert_eq!(
            right - left + 1,
            modules,
            "fallback stays one pixel per module"
        );
        assert_eq!(bottom - top + 1, modules);
    }
}

#[test]
fn saved_layout_rotations_preserve_quiet_zones_between_cut_marks() {
    let modules = QrCode::new(b"ASSET-42").unwrap().width() as u32;
    for rotation in [0, 90, 180, 270] {
        let doc = LabelDocument::from_toml_str(&format!(
            "version = 1\ntape_width_mm = 12\nfont_name = 'Inter'\nfont_margin = 0\n\
             [[elements]]\ntype = 'cut_mark'\n\
             [[elements]]\ntype = 'qr_code'\ncontent = 'ASSET-42'\nrotation = {rotation}.0\n\
             [[elements]]\ntype = 'cut_mark'\n"
        ))
        .unwrap();
        let restored = LabelDocument::from_toml_str(&doc.to_toml_string().unwrap()).unwrap();
        let bitmap = render_elements(&restored.elements, 76, "Inter", 0, &mut TextRenderer::new())
            .unwrap()
            .unwrap();
        assert_eq!(bitmap.height(), 76);
        let qr_size = bitmap.width() - 18; // Two nine-pixel cut-mark segments.
        let offset_y = (76 - qr_size) / 2;
        let mut qr = LabelBitmap::new(qr_size, qr_size);
        for y in 0..qr_size {
            for x in 0..qr_size {
                qr.set_pixel(x, y, bitmap.get_pixel(x + 9, y + offset_y));
            }
        }
        assert_four_module_border(&qr, modules);
        assert!(bitmap.get_pixel(5, 0));
        assert!(bitmap.get_pixel(bitmap.width() - 4, 0));
    }
}
