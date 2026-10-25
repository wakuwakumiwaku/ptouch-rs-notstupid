// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 ptouch-rs-notstupid contributors

//! Exercise alpha handling through the public PNG and saved-layout APIs.

use std::io::Cursor;

use image::{DynamicImage, GrayAlphaImage, ImageFormat, LumaA, Rgba, RgbaImage};
use ptouch_render::bitmap::LabelBitmap;
use ptouch_render::document::{LabelDocument, LabelElement, render_elements};
use ptouch_render::image_loader::{BinarizeMode, ImageLoadOptions, load_image_from_reader};
use ptouch_render::text::TextRenderer;

fn png(image: impl Into<DynamicImage>) -> Vec<u8> {
    let mut encoded = Cursor::new(Vec::new());
    image
        .into()
        .write_to(&mut encoded, ImageFormat::Png)
        .unwrap();
    encoded.into_inner()
}

fn artwork(background: Rgba<u8>) -> RgbaImage {
    let mut image = RgbaImage::from_pixel(12, 10, background);
    for y in 3..6 {
        for x in 2..4 {
            image.put_pixel(x, y, Rgba([0, 0, 0, 255]));
        }
    }
    image
}

fn load(bytes: &[u8], options: &ImageLoadOptions) -> LabelBitmap {
    load_image_from_reader(Cursor::new(bytes), options).unwrap()
}

fn assert_same(actual: &LabelBitmap, expected: &LabelBitmap) {
    assert_eq!(actual.width(), expected.width());
    assert_eq!(actual.height(), expected.height());
    assert_eq!(actual.data(), expected.data());
}

fn assert_artwork(bitmap: &LabelBitmap) {
    assert_eq!((bitmap.width(), bitmap.height()), (12, 10));
    for y in 0..10 {
        for x in 0..12 {
            assert_eq!(
                bitmap.get_pixel(x, y),
                (2..4).contains(&x) && (3..6).contains(&y),
                "artwork pixel ({x}, {y})"
            );
        }
    }
}

#[test]
fn transparent_black_artwork_is_not_erased_by_auto_inversion() {
    let bytes = png(artwork(Rgba([0, 0, 0, 0])));
    assert_artwork(&load(&bytes, &ImageLoadOptions::default()));
}

#[test]
fn hidden_rgb_does_not_change_visible_artwork_in_any_binarization_mode() {
    let opaque = png(artwork(Rgba([255, 255, 255, 255])));
    for hidden_rgb in [[0, 0, 0, 0], [255, 255, 255, 0], [200, 17, 99, 0]] {
        let transparent = png(artwork(Rgba(hidden_rgb)));
        for binarize in [
            BinarizeMode::Auto,
            BinarizeMode::Threshold,
            BinarizeMode::Dither,
        ] {
            for auto_invert in [false, true] {
                for target_height in [None, Some(20)] {
                    let options = ImageLoadOptions {
                        binarize,
                        target_height,
                        auto_invert,
                    };
                    assert_same(&load(&transparent, &options), &load(&opaque, &options));
                }
            }
        }
    }
}

#[test]
fn partial_alpha_matches_artwork_precomposited_on_white() {
    let alpha = [0, 64, 128, 192, 255];
    // Black with these opacities over white has these exact gray values.
    let gray = [255, 191, 127, 63, 0];
    let mut transparent = RgbaImage::from_pixel(20, 10, Rgba([0, 0, 0, 0]));
    let mut opaque = RgbaImage::from_pixel(20, 10, Rgba([255, 255, 255, 255]));
    for (x, (&a, &g)) in alpha.iter().zip(&gray).enumerate() {
        for y in 0..10 {
            transparent.put_pixel(x as u32, y, Rgba([0, 0, 0, a]));
            opaque.put_pixel(x as u32, y, Rgba([g, g, g, 255]));
        }
    }
    let transparent = png(transparent);
    let opaque = png(opaque);
    for binarize in [
        BinarizeMode::Auto,
        BinarizeMode::Threshold,
        BinarizeMode::Dither,
    ] {
        let options = ImageLoadOptions {
            binarize,
            ..ImageLoadOptions::default()
        };
        assert_same(&load(&transparent, &options), &load(&opaque, &options));
    }
}

#[test]
fn grayscale_alpha_png_preserves_black_artwork() {
    let mut image = GrayAlphaImage::from_pixel(12, 10, LumaA([0, 0]));
    for y in 3..6 {
        for x in 2..4 {
            image.put_pixel(x, y, LumaA([0, 255]));
        }
    }
    assert_artwork(&load(&png(image), &ImageLoadOptions::default()));
}

#[test]
fn sixteen_bit_rgba_png_preserves_black_artwork() {
    let mut image = image::ImageBuffer::from_pixel(12, 10, Rgba([0u16, 0, 0, 0]));
    for y in 3..6 {
        for x in 2..4 {
            image.put_pixel(x, y, Rgba([0, 0, 0, u16::MAX]));
        }
    }
    let bytes = png(DynamicImage::ImageRgba16(image));
    assert_artwork(&load(&bytes, &ImageLoadOptions::default()));
}

#[test]
fn embedded_png_survives_layout_round_trip_and_composition() {
    let bytes = png(artwork(Rgba([0, 0, 0, 0])));
    let mut doc = LabelDocument::from_toml_str(
        "version = 1\ntape_width_mm = 12\nfont_name = 'Inter'\nfont_margin = 0\nelements = []\n",
    )
    .unwrap();
    doc.elements = vec![LabelElement::image_from_bytes(None, bytes.clone())];
    let restored = LabelDocument::from_toml_str(&doc.to_toml_string().unwrap()).unwrap();
    let LabelElement::Image {
        image_data, bitmap, ..
    } = &restored.elements[0]
    else {
        panic!("expected image element");
    };
    assert_eq!(
        image_data, &bytes,
        "original PNG bytes must remain embedded"
    );
    assert_artwork(bitmap.as_ref().unwrap());
    let composed = render_elements(&restored.elements, 10, "", 0, &mut TextRenderer::new())
        .unwrap()
        .unwrap();
    assert_artwork(&composed);
}

#[test]
fn opaque_rgb_and_rgba_inputs_keep_existing_inversion_behavior() {
    let bright = artwork(Rgba([255, 255, 255, 255]));
    let mut dark = bright.clone();
    for pixel in dark.pixels_mut() {
        for channel in &mut pixel.0[..3] {
            *channel = 255 - *channel;
        }
    }
    for image in [bright, dark] {
        let rgb = DynamicImage::ImageRgba8(image.clone()).to_rgb8();
        assert_artwork(&load(&png(rgb), &ImageLoadOptions::default()));
        assert_artwork(&load(&png(image), &ImageLoadOptions::default()));
    }
}

#[test]
fn fully_transparent_pixels_remain_blank_without_auto_inversion() {
    let options = ImageLoadOptions {
        auto_invert: false,
        ..ImageLoadOptions::default()
    };
    let bytes = png(RgbaImage::from_pixel(12, 10, Rgba([0, 0, 0, 0])));
    let bitmap = load(&bytes, &options);
    assert!(bitmap.data().iter().all(|&byte| byte == 0));
}
