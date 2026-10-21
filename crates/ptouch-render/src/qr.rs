// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Huang Rui <vowstar@gmail.com>

//! QR Code generation and 1-bit raster conversion for label printing.

use crate::bitmap::LabelBitmap;
use crate::{RenderError, Result};

/// Generate a 1-bit packed bitmap of a QR code from text or binary content.
///
/// Uses exact integer module scaling to guarantee 100% crisp edges without
/// subpixel blurring or interpolation, ensuring instantaneous scanning with
/// smartphones and optical barcode scanners.
///
/// - `content`: The text, URL, Wi-Fi config, or data to encode.
/// - `target_height`: The maximum available height in pixels (e.g. tape printable height).
///
/// Returns a square `LabelBitmap` with the QR code and a standard 2-module quiet zone.
/// Non-blank payloads are encoded verbatim, including surrounding whitespace.
pub fn render_qr_code(content: &str, target_height: u32) -> Result<LabelBitmap> {
    if content.trim().is_empty() {
        return Err(RenderError::Layout(
            "QR code content cannot be empty".to_string(),
        ));
    }

    let code = qrcode::QrCode::new(content.as_bytes())
        .map_err(|e| RenderError::Layout(format!("QR code generation failed: {e}")))?;

    let module_count = code.width() as u32;
    // Standard quiet zone margin around the QR code matrix (2 modules)
    let quiet_zone = 2u32;
    let total_modules = module_count + quiet_zone * 2;

    // Scale each module by an integer number of pixels to fit target_height
    let module_size = (target_height / total_modules).max(1);
    let size_px = total_modules * module_size;

    let mut bitmap = LabelBitmap::new(size_px, size_px);
    let colors = code.to_colors();

    for (i, color) in colors.into_iter().enumerate() {
        if color == qrcode::Color::Dark {
            let mod_x = (i as u32 % module_count) + quiet_zone;
            let mod_y = (i as u32 / module_count) + quiet_zone;
            for dy in 0..module_size {
                for dx in 0..module_size {
                    bitmap.set_pixel(mod_x * module_size + dx, mod_y * module_size + dy, true);
                }
            }
        }
    }

    Ok(bitmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_content_errors() {
        let err = render_qr_code("", 64);
        assert!(err.is_err());
        let err2 = render_qr_code("   ", 64);
        assert!(err2.is_err());
    }

    #[test]
    fn test_basic_qr_generation() {
        let bmp = render_qr_code("https://github.com", 128).unwrap();
        assert_eq!(bmp.width(), bmp.height());
        assert!(bmp.width() <= 128);
        assert!(bmp.width() > 0);

        // Verify that there are both black and white pixels
        let mut black_pixels = 0;
        let mut white_pixels = 0;
        for y in 0..bmp.height() {
            for x in 0..bmp.width() {
                if bmp.get_pixel(x, y) {
                    black_pixels += 1;
                } else {
                    white_pixels += 1;
                }
            }
        }
        assert!(black_pixels > 0);
        assert!(white_pixels > 0);
        // Quiet zone outer border must be white
        assert!(!bmp.get_pixel(0, 0));
        assert!(!bmp.get_pixel(bmp.width() - 1, bmp.height() - 1));
    }

    #[test]
    fn test_qr_preserves_payload_whitespace() {
        for content in [" ASSET-42 ", "\tASSET-42\r\n", "\u{2003}ASSET-42\u{a0}"] {
            let bitmap = render_qr_code(content, 128).unwrap();
            let code = qrcode::QrCode::new(content.as_bytes()).unwrap();
            let modules = code.width() as u32;
            let scale = 128 / (modules + 4);
            assert_eq!(bitmap.width(), (modules + 4) * scale);
            for y in 0..modules {
                for x in 0..modules {
                    assert_eq!(
                        bitmap.get_pixel((x + 2) * scale, (y + 2) * scale),
                        code[(x as usize, y as usize)] == qrcode::Color::Dark,
                        "payload {content:?}, module ({x}, {y})"
                    );
                }
            }
        }
    }

    #[test]
    fn test_small_target_height_fallback() {
        // Even with tiny target height, module_size is at least 1px
        let bmp = render_qr_code("12345", 10).unwrap();
        assert!(bmp.width() >= 25);
    }
}
