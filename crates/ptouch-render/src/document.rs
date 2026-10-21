// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Huang Rui <vowstar@gmail.com>

//! Label document model and shared element rendering.
//!
//! A [`LabelDocument`] is the serializable description of a label: global tape
//! and font settings plus an ordered list of [`LabelElement`]s. The same model
//! drives both the GUI canvas and command-line rendering, so a design composed
//! in one can be reproduced exactly in the other.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::path::PathBuf;

use log::error;
use serde::{Deserialize, Serialize};

use crate::RenderError;
use crate::Result;
use crate::bitmap::LabelBitmap;
use crate::compose;
use crate::image_loader::{self, ImageLoadOptions};
use crate::text::{TextAlign, TextRenderer};

/// Current on-disk layout format version.
pub const DOCUMENT_VERSION: u32 = 1;

/// Default design resolution for layouts saved before the dpi field existed.
fn default_dpi() -> u16 {
    180
}

/// A complete label design: global settings plus an ordered element list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelDocument {
    /// Layout format version. See [`DOCUMENT_VERSION`].
    pub version: u32,
    /// Tape width in millimeters the design was created for.
    pub tape_width_mm: u8,
    /// Print resolution in dpi the design was created for. Element sizes
    /// and positions are pixels at this resolution. Layouts from before
    /// this field default to 180 dpi.
    #[serde(default = "default_dpi")]
    pub dpi: u16,
    /// Font family name used for text elements.
    pub font_name: String,
    /// Font top/bottom margin in pixels.
    pub font_margin: u32,
    /// Mirror the whole composed label left-right (horizontal). Applied once,
    /// after all elements are composed, independently of per-element flips.
    #[serde(default)]
    pub flip_h: bool,
    /// Mirror the whole composed label top-bottom (vertical). Applied once,
    /// after all elements are composed, independently of per-element flips.
    #[serde(default)]
    pub flip_v: bool,
    /// Optional front/back margin in millimeters.
    /// - If `None` or >= 24.5 mm: Centered full margin using hardware leader (~24.5-27mm), no pre-trim scrap.
    /// - If `< 24.5 mm`: Compact margin (e.g. 3.0mm); automatically triggers pre-trimming the leader scrap.
    #[serde(default)]
    pub margin_mm: Option<f32>,
    /// Optional maximum total finished label length in millimeters (including left & right margins).
    /// When set, if the composed label would exceed this length, text elements are automatically
    /// resized to a smaller font size so the finished label fits within `max_length_mm`.
    /// Shorter labels keep their natural length without extra padding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length_mm: Option<f32>,
    /// When true, text elements are rendered at the largest possible font size whose
    /// actual glyph ink bounding box fits within the confined label area.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fit_largest_font: bool,
    /// Elements in composition order (left to right).
    pub elements: Vec<LabelElement>,
}

impl LabelDocument {
    /// Returns true if this label requires pre-trimming the 24.5mm hardware leader scrap.
    pub fn is_pretrim(&self) -> bool {
        if self.margin_mm.is_none() && self.max_length_mm.is_none() {
            return false;
        }
        self.effective_margin_mm() < 24.5
    }

    /// Effective margin in millimeters.
    /// If `max_length_mm` is set and either `fit_largest_font` is enabled (with no explicit
    /// margin) or the default (~27mm) margin would leave insufficient space (< 10mm) for content,
    /// automatically falls back to a compact 3.0mm pre-trim margin.
    pub fn effective_margin_mm(&self) -> f32 {
        let base =
            if self.margin_mm.is_none() && self.max_length_mm.is_some() && self.fit_largest_font {
                3.0
            } else {
                self.margin_mm.unwrap_or(27.0)
            };
        if let Some(max_len) = self.max_length_mm
            && base * 2.0 + 10.0 > max_len
        {
            return 3.0_f32.min((max_len * 0.2).max(1.5));
        }
        base
    }

    /// Maximum printable content length in millimeters when `max_length_mm` is set.
    /// Subtracts both left and right effective margins from `max_length_mm`.
    pub fn max_content_length_mm(&self) -> Option<f32> {
        self.max_length_mm.map(|max_len| {
            let margin = self.effective_margin_mm();
            (max_len - margin * 2.0).max(2.0)
        })
    }

    /// Maximum printable content width in pixels at the given DPI when `max_length_mm` is set.
    pub fn max_content_width_px(&self, dpi: u16) -> Option<u32> {
        Self::max_content_width_px_with_margin(self.max_length_mm, self.effective_margin_mm(), dpi)
    }

    /// Compute the maximum printable content width in pixels for a given total `max_length_mm`,
    /// single-side `margin_mm`, and `dpi`.
    pub fn max_content_width_px_with_margin(
        max_length_mm: Option<f32>,
        margin_mm: f32,
        dpi: u16,
    ) -> Option<u32> {
        max_length_mm.map(|max_len| {
            let dpi_f = if dpi > 0 { dpi as f32 } else { 180.0 };
            let eff_margin = if margin_mm * 2.0 + 10.0 > max_len {
                3.0_f32.min((max_len * 0.2).max(1.5))
            } else {
                margin_mm
            };
            let content_mm = (max_len - eff_margin * 2.0).max(2.0);
            ((content_mm * dpi_f / 25.4).floor() as u32).max(1)
        })
    }
}

fn default_batch_copies() -> u32 {
    1
}

/// An entry in a label batch with its own document and print count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchEntry {
    /// Optional label title or description.
    #[serde(default)]
    pub title: String,
    /// Number of copies to print for this label (NUMBER TO PRINT).
    #[serde(default = "default_batch_copies")]
    pub copies: u32,
    /// The label document.
    pub document: LabelDocument,
}

/// A batch project containing multiple labels, each with an individual print count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelBatch {
    /// Layout format version.
    pub version: u32,
    /// Default tape width in millimeters for the batch.
    pub tape_width_mm: u8,
    /// Default print resolution in DPI for the batch.
    #[serde(default = "default_dpi")]
    pub dpi: u16,
    /// Ordered list of batch entries.
    pub labels: Vec<BatchEntry>,
}

impl LabelBatch {
    /// Create a new batch with a single label.
    pub fn new(document: LabelDocument, title: impl Into<String>, copies: u32) -> Self {
        let tape_width_mm = document.tape_width_mm;
        let dpi = document.dpi;
        Self {
            version: DOCUMENT_VERSION,
            tape_width_mm,
            dpi,
            labels: vec![BatchEntry {
                title: title.into(),
                copies,
                document,
            }],
        }
    }

    /// Serialize the batch to a TOML string.
    pub fn to_toml_string(&self) -> Result<String> {
        Ok(toml::to_string(self)?)
    }

    /// Parse a batch from a TOML string. Supports both batch documents and single LabelDocuments.
    pub fn from_toml_str(text: &str) -> Result<Self> {
        // Try parsing directly as LabelBatch
        if let Ok(mut batch) = toml::from_str::<LabelBatch>(text) {
            if batch.version == 0 || batch.version > DOCUMENT_VERSION {
                return Err(RenderError::Layout(format!(
                    "unsupported batch layout version {} (this build supports up to {})",
                    batch.version, DOCUMENT_VERSION
                )));
            }
            for entry in &mut batch.labels {
                entry.document.decode_image_caches()?;
            }
            return Ok(batch);
        }

        // Fall back to parsing as a single LabelDocument
        let doc = LabelDocument::from_toml_str(text)?;
        Ok(LabelBatch {
            version: doc.version,
            tape_width_mm: doc.tape_width_mm,
            dpi: doc.dpi,
            labels: vec![BatchEntry {
                title: "Label 1".to_string(),
                copies: 1,
                document: doc,
            }],
        })
    }
}

impl LabelDocument {
    /// Serialize the document to a TOML string.
    pub fn to_toml_string(&self) -> Result<String> {
        Ok(toml::to_string(self)?)
    }

    /// Parse a document from a TOML string.
    ///
    /// Rejects an unsupported format version and decodes every embedded image
    /// into its render cache, so the returned document is ready to render.
    pub fn from_toml_str(text: &str) -> Result<Self> {
        let mut doc: LabelDocument = toml::from_str(text)?;
        if doc.version == 0 || doc.version > DOCUMENT_VERSION {
            return Err(RenderError::Layout(format!(
                "unsupported layout version {} (this build supports up to {})",
                doc.version, DOCUMENT_VERSION
            )));
        }
        doc.decode_image_caches()?;
        Ok(doc)
    }

    /// Decode every image or QR element into its render cache.
    ///
    /// Returns an error if any embedded image cannot be decoded, so a corrupt
    /// layout fails loudly rather than rendering blank.
    pub fn decode_image_caches(&mut self) -> Result<()> {
        for element in &mut self.elements {
            if let LabelElement::Image {
                image_data, bitmap, ..
            } = element
                && bitmap.is_none()
                && !image_data.is_empty()
            {
                let decoded = image_loader::load_image_from_reader(
                    Cursor::new(image_data.as_slice()),
                    &ImageLoadOptions::default(),
                )?;
                *bitmap = Some(decoded);
            }
            if let LabelElement::QrCode {
                content,
                bitmap,
                target_height,
                ..
            } = element
                && bitmap.is_none()
                && !content.trim().is_empty()
            {
                let h = target_height.unwrap_or(128);
                *bitmap = crate::qr::render_qr_code(content, h).ok();
            }
        }
        Ok(())
    }

    /// Collect the unique `{{name}}` placeholder names used in text and QR elements.
    ///
    /// The result is sorted for stable display and de-duplication.
    pub fn placeholders(&self) -> Vec<String> {
        let mut names = BTreeSet::new();
        for element in &self.elements {
            match element {
                LabelElement::Text { content, .. } | LabelElement::QrCode { content, .. } => {
                    collect_placeholder_names(content, &mut names);
                }
                _ => {}
            }
        }
        names.into_iter().collect()
    }

    /// Replace `{{name}}` placeholders in all text and QR elements using `values`.
    ///
    /// A placeholder with no matching value is replaced with an empty string;
    /// callers that want to reject missing values should check
    /// [`LabelDocument::placeholders`] against the provided keys first.
    pub fn apply_values(&mut self, values: &BTreeMap<String, String>) {
        for element in &mut self.elements {
            match element {
                LabelElement::Text { content, .. } => {
                    *content = substitute_placeholders(content, |name| values.get(name).cloned());
                }
                LabelElement::QrCode {
                    content,
                    bitmap,
                    target_height,
                    ..
                } => {
                    *content = substitute_placeholders(content, |name| values.get(name).cloned());
                    let h = target_height.unwrap_or(128);
                    *bitmap = crate::qr::render_qr_code(content, h).ok();
                }
                _ => {}
            }
        }
    }
}

/// True if `name` is a valid placeholder identifier: non-empty and made of
/// ASCII letters, digits, or underscores.
fn is_valid_placeholder(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Collect placeholder names from `content` into `names`.
fn collect_placeholder_names(content: &str, names: &mut BTreeSet<String>) {
    let mut rest = content;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let name = after[..end].trim();
        if is_valid_placeholder(name) {
            names.insert(name.to_string());
        }
        rest = &after[end + 2..];
    }
}

/// Replace every valid `{{name}}` in `content`. `lookup` returns the value for a
/// name, or `None` to substitute an empty string. Text that is not a valid
/// placeholder is left untouched.
fn substitute_placeholders(content: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut result = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let name = after[..end].trim();
        if is_valid_placeholder(name) {
            result.push_str(&rest[..start]);
            if let Some(value) = lookup(name) {
                result.push_str(&value);
            }
            rest = &after[end + 2..];
        } else {
            // Not a placeholder; keep the literal "{{" and continue past it.
            result.push_str(&rest[..start + 2]);
            rest = after;
        }
    }
    result.push_str(rest);
    result
}

/// A single element in the label composition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LabelElement {
    /// A text block with content, optional font size, alignment, and rotation.
    Text {
        /// Text content; newlines separate lines.
        content: String,
        /// Explicit font size in points. `None` means auto-fit to tape height.
        #[serde(skip_serializing_if = "Option::is_none")]
        font_size: Option<f32>,
        /// Horizontal alignment of the text block.
        align: TextAlign,
        /// Rotation angle in degrees (clockwise). 0.0 = horizontal.
        rotation: f32,
        /// Mirror this element left-right (horizontal). Applied to the element's
        /// own bitmap, before it is composed into the label.
        #[serde(default)]
        flip_h: bool,
        /// Mirror this element top-bottom (vertical). Applied to the element's
        /// own bitmap, before it is composed into the label.
        #[serde(default)]
        flip_v: bool,
    },
    /// An image embedded as its original source bytes.
    Image {
        /// Original file path, kept for display only. Never required to exist.
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<PathBuf>,
        /// Source image bytes (PNG/JPEG/SVG/...), stored as base64 in the file.
        #[serde(with = "crate::base64_bytes")]
        image_data: Vec<u8>,
        /// Decoded render cache; rebuilt from `image_data`, never serialized.
        #[serde(skip)]
        bitmap: Option<LabelBitmap>,
        /// Rotation angle in degrees (clockwise). 0.0 = horizontal.
        rotation: f32,
        /// Target height in pixels. `None` = auto (fit to tape height).
        #[serde(skip_serializing_if = "Option::is_none")]
        target_height: Option<u32>,
        /// Mirror this element left-right (horizontal). Applied to the element's
        /// own bitmap, before it is composed into the label.
        #[serde(default)]
        flip_h: bool,
        /// Mirror this element top-bottom (vertical). Applied to the element's
        /// own bitmap, before it is composed into the label.
        #[serde(default)]
        flip_v: bool,
    },
    /// A cut mark separator.
    CutMark,
    /// Horizontal padding in pixels.
    Padding {
        /// Padding width in pixels.
        pixels: u32,
    },
    /// A QR code generated from URL or text.
    QrCode {
        /// Content encoded in the QR code (URL, text, WiFi, etc.).
        content: String,
        /// Decoded render cache; rebuilt from `content`, never serialized.
        #[serde(skip)]
        bitmap: Option<LabelBitmap>,
        /// Rotation angle in degrees (clockwise). 0.0 = horizontal.
        #[serde(default)]
        rotation: f32,
        /// Target height in pixels. `None` = auto (fit to tape height).
        #[serde(skip_serializing_if = "Option::is_none")]
        target_height: Option<u32>,
        /// Mirror this element left-right (horizontal). Applied to the element's
        /// own bitmap, before it is composed into the label.
        #[serde(default)]
        flip_h: bool,
        /// Mirror this element top-bottom (vertical). Applied to the element's
        /// own bitmap, before it is composed into the label.
        #[serde(default)]
        flip_v: bool,
    },
}

impl LabelElement {
    /// Build an image element from decoded source bytes, decoding a render
    /// cache eagerly so previews and rendering are fast.
    pub fn image_from_bytes(path: Option<PathBuf>, image_data: Vec<u8>) -> Self {
        let bitmap = image_loader::load_image_from_reader(
            Cursor::new(&image_data),
            &ImageLoadOptions::default(),
        )
        .ok();
        LabelElement::Image {
            path,
            image_data,
            bitmap,
            rotation: 0.0,
            target_height: None,
            flip_h: false,
            flip_v: false,
        }
    }

    /// Build a QR code element from text/URL, generating a render cache eagerly.
    pub fn qr_from_content(content: impl Into<String>) -> Self {
        let content = content.into();
        let bitmap = crate::qr::render_qr_code(&content, 128).ok();
        LabelElement::QrCode {
            content,
            bitmap,
            rotation: 0.0,
            target_height: None,
            flip_h: false,
            flip_v: false,
        }
    }

    /// Set the decoded render cache for an image element.
    pub fn set_image_bitmap(&mut self, decoded: Option<LabelBitmap>) {
        if let LabelElement::Image { bitmap, .. } = self {
            *bitmap = decoded;
        }
    }

    /// Set the decoded render cache for a QR code element.
    pub fn set_qr_bitmap(&mut self, decoded: Option<LabelBitmap>) {
        if let LabelElement::QrCode { bitmap, .. } = self {
            *bitmap = decoded;
        }
    }

    /// Returns a short display name for the element list.
    pub fn display_name(&self) -> String {
        match self {
            LabelElement::Text { content, .. } => {
                let preview: String = content.chars().take(20).collect();
                if content.chars().count() > 20 {
                    format!("Text: {}...", preview)
                } else {
                    format!("Text: {}", preview)
                }
            }
            LabelElement::Image { path, .. } => {
                let name = path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "embedded".to_string());
                format!("Image: {}", name)
            }
            LabelElement::CutMark => "Cut Mark".to_string(),
            LabelElement::Padding { pixels } => format!("Padding: {} px", pixels),
            LabelElement::QrCode { content, .. } => {
                let preview: String = content.chars().take(20).collect();
                if content.chars().count() > 20 {
                    format!("QR: {}...", preview)
                } else {
                    format!("QR: {}", preview)
                }
            }
        }
    }
}

/// Font settings shared across all text elements in a document.
struct FontSettings<'a> {
    /// Font family name.
    name: &'a str,
    /// Top/bottom margin in pixels.
    margin: u32,
}

/// Render an ordered element list into a single label bitmap.
///
/// Elements are rendered to `tape_width_px` tall segments and concatenated left
/// to right. Per-element rendering failures (empty text, undecodable image) are
/// logged and skipped rather than aborting the whole label. Returns `None` when
/// no element produced any output.
pub fn render_elements(
    elements: &[LabelElement],
    tape_width_px: u32,
    font_name: &str,
    font_margin: u32,
    renderer: &mut TextRenderer,
) -> Result<Option<LabelBitmap>> {
    render_elements_with_max_width(
        elements,
        tape_width_px,
        font_name,
        font_margin,
        None,
        renderer,
    )
}

/// Render an ordered element list into a single label bitmap, optionally confining
/// the total width to `max_width_px` by automatically scaling down the font size
/// of text elements when the initial composition exceeds `max_width_px`.
///
/// If the initial composition already fits within `max_width_px`, the natural
/// (shorter) width is preserved without extra padding.
pub fn render_elements_with_max_width(
    elements: &[LabelElement],
    tape_width_px: u32,
    font_name: &str,
    font_margin: u32,
    max_width_px: Option<u32>,
    renderer: &mut TextRenderer,
) -> Result<Option<LabelBitmap>> {
    render_elements_with_options(
        elements,
        tape_width_px,
        font_name,
        font_margin,
        max_width_px,
        false,
        renderer,
    )
}

/// Render an ordered element list into a single label bitmap, optionally confining
/// the total width to `max_width_px` and optionally maximizing the font size
/// (`fit_largest_font`) so text fills the largest possible font in the confined area.
pub fn render_elements_with_options(
    elements: &[LabelElement],
    tape_width_px: u32,
    font_name: &str,
    font_margin: u32,
    max_width_px: Option<u32>,
    fit_largest_font: bool,
    renderer: &mut TextRenderer,
) -> Result<Option<LabelBitmap>> {
    let font_settings = FontSettings {
        name: font_name,
        margin: font_margin,
    };

    // First pass: render all non-text segments and count text elements.
    let mut non_text_segments: Vec<Option<LabelBitmap>> = Vec::with_capacity(elements.len());
    let mut fixed_width_px: u32 = 0;
    let mut non_empty_text_count: usize = 0;
    let mut any_rotated_text = false;

    for element in elements {
        match element {
            LabelElement::Text {
                content, rotation, ..
            } => {
                non_text_segments.push(None);
                if !content.trim().is_empty() {
                    non_empty_text_count += 1;
                    if is_rotated(*rotation) {
                        any_rotated_text = true;
                    }
                }
            }
            LabelElement::Image {
                image_data,
                bitmap,
                rotation,
                target_height,
                flip_h,
                flip_v,
                ..
            } => {
                let seg = render_image_segment(
                    bitmap.as_ref(),
                    image_data,
                    *rotation,
                    *target_height,
                    tape_width_px,
                )
                .map(|s| s.mirrored(*flip_h, *flip_v));
                if let Some(ref s) = seg {
                    fixed_width_px += s.width();
                }
                non_text_segments.push(seg);
            }
            LabelElement::CutMark => {
                let seg = compose::cutmark(tape_width_px);
                fixed_width_px += seg.width();
                non_text_segments.push(Some(seg));
            }
            LabelElement::Padding { pixels } => {
                let seg = compose::padding(tape_width_px, *pixels);
                fixed_width_px += seg.width();
                non_text_segments.push(Some(seg));
            }
            LabelElement::QrCode {
                content,
                bitmap,
                rotation,
                target_height,
                flip_h,
                flip_v,
            } => {
                let seg = render_qr_segment(
                    bitmap.as_ref(),
                    content,
                    *rotation,
                    *target_height,
                    tape_width_px,
                )
                .map(|s| s.mirrored(*flip_h, *flip_v));
                if let Some(ref s) = seg {
                    fixed_width_px += s.width();
                }
                non_text_segments.push(seg);
            }
        }
    }

    let mut text_segments: Vec<Option<LabelBitmap>> = Vec::with_capacity(elements.len());

    if fit_largest_font && non_empty_text_count > 0 && !any_rotated_text {
        // Fast path for a single unrotated text element in a confined area.
        if non_empty_text_count == 1 && max_width_px.is_some() {
            let target_text_w =
                max_width_px.map(|max_w| max_w.saturating_sub(fixed_width_px).max(1));
            for element in elements {
                if let LabelElement::Text {
                    content,
                    align,
                    flip_h,
                    flip_v,
                    ..
                } = element
                {
                    if content.trim().is_empty() {
                        text_segments.push(None);
                    } else {
                        let lines: Vec<&str> = content.lines().collect();
                        let seg = match renderer.render_text_fit_box(
                            &lines,
                            tape_width_px,
                            target_text_w,
                            font_name,
                            font_margin,
                            *align,
                            true,
                        ) {
                            Ok(bmp) => Some(bmp.mirrored(*flip_h, *flip_v)),
                            Err(e) => {
                                error!("Text fit-box render failed: {}", e);
                                None
                            }
                        };
                        text_segments.push(seg);
                    }
                } else {
                    text_segments.push(None);
                }
            }
        } else {
            // Multiple text elements or unconstrained width with fit_largest_font.
            let mut initial_text_width_px: u32 = 0;
            let mut initial_widths: Vec<u32> = Vec::with_capacity(elements.len());

            for element in elements {
                if let LabelElement::Text {
                    content,
                    font_size,
                    align,
                    rotation,
                    flip_h,
                    flip_v,
                } = element
                {
                    if content.trim().is_empty() {
                        initial_widths.push(0);
                        text_segments.push(None);
                    } else if font_size.is_some() && max_width_px.is_none() {
                        let seg = render_text_segment_scaled(
                            renderer,
                            content,
                            *font_size,
                            *align,
                            *rotation,
                            tape_width_px,
                            &font_settings,
                            1.0,
                        )
                        .map(|s| s.mirrored(*flip_h, *flip_v));
                        let w = seg.as_ref().map_or(0, |s| s.width());
                        initial_text_width_px += w;
                        initial_widths.push(w);
                        text_segments.push(seg);
                    } else {
                        let lines: Vec<&str> = content.lines().collect();
                        let seg = renderer
                            .render_text_fit_box(
                                &lines,
                                tape_width_px,
                                None,
                                font_name,
                                font_margin,
                                *align,
                                false,
                            )
                            .ok()
                            .map(|s| s.mirrored(*flip_h, *flip_v));
                        let w = seg.as_ref().map_or(0, |s| s.width());
                        initial_text_width_px += w;
                        initial_widths.push(w);
                        text_segments.push(seg);
                    }
                } else {
                    initial_widths.push(0);
                    text_segments.push(None);
                }
            }

            if let Some(max_w) = max_width_px
                && fixed_width_px + initial_text_width_px > max_w
                && initial_text_width_px > 0
            {
                let target_text_w = max_w.saturating_sub(fixed_width_px).max(1);
                let mut fitted_segments = Vec::with_capacity(elements.len());
                for (element, init_w) in elements.iter().zip(initial_widths) {
                    if let LabelElement::Text {
                        content,
                        align,
                        flip_h,
                        flip_v,
                        ..
                    } = element
                    {
                        if content.trim().is_empty() || init_w == 0 {
                            fitted_segments.push(None);
                        } else {
                            let share_w = ((target_text_w as u64 * init_w as u64)
                                / (initial_text_width_px as u64))
                                .max(1) as u32;
                            let lines: Vec<&str> = content.lines().collect();
                            let seg = renderer
                                .render_text_fit_box(
                                    &lines,
                                    tape_width_px,
                                    Some(share_w),
                                    font_name,
                                    font_margin,
                                    *align,
                                    true,
                                )
                                .ok()
                                .map(|s| s.mirrored(*flip_h, *flip_v));
                            fitted_segments.push(seg);
                        }
                    } else {
                        fitted_segments.push(None);
                    }
                }
                text_segments = fitted_segments;
            }
        }
    } else {
        // Standard scale-down path (when fit_largest_font is false or text is rotated).
        let mut initial_text_width_px: u32 = 0;
        let mut has_text = false;

        for element in elements {
            if let LabelElement::Text {
                content,
                font_size,
                align,
                rotation,
                flip_h,
                flip_v,
            } = element
            {
                let seg = render_text_segment_scaled(
                    renderer,
                    content,
                    *font_size,
                    *align,
                    *rotation,
                    tape_width_px,
                    &font_settings,
                    1.0,
                )
                .map(|s| s.mirrored(*flip_h, *flip_v));
                if let Some(ref s) = seg {
                    initial_text_width_px += s.width();
                    has_text = true;
                }
                text_segments.push(seg);
            } else {
                text_segments.push(None);
            }
        }

        let total_initial_width = fixed_width_px + initial_text_width_px;

        // If max_width_px is set and the rendered label exceeds it, binary-search
        // the largest font scale factor in (0.0, 1.0] that fits within the budget.
        if let Some(max_w) = max_width_px
            && total_initial_width > max_w
            && has_text
        {
            let target_text_w = max_w.saturating_sub(fixed_width_px).max(1);
            let mut low = 0.01_f32;
            let mut high = 1.0_f32;
            let mut best_text_segments: Option<Vec<Option<LabelBitmap>>> = None;

            for _ in 0..12 {
                let mid = (low + high) * 0.5;
                let mut candidate_segments = Vec::with_capacity(elements.len());
                let mut candidate_w: u32 = 0;

                for element in elements {
                    if let LabelElement::Text {
                        content,
                        font_size,
                        align,
                        rotation,
                        flip_h,
                        flip_v,
                    } = element
                    {
                        let seg = render_text_segment_scaled(
                            renderer,
                            content,
                            *font_size,
                            *align,
                            *rotation,
                            tape_width_px,
                            &font_settings,
                            mid,
                        )
                        .map(|s| s.mirrored(*flip_h, *flip_v));
                        if let Some(ref s) = seg {
                            candidate_w += s.width();
                        }
                        candidate_segments.push(seg);
                    } else {
                        candidate_segments.push(None);
                    }
                }

                if candidate_w <= target_text_w {
                    best_text_segments = Some(candidate_segments);
                    low = mid;
                } else {
                    high = mid;
                }
            }

            text_segments = match best_text_segments {
                Some(best) => best,
                None => {
                    let mut min_segments = Vec::with_capacity(elements.len());
                    for element in elements {
                        if let LabelElement::Text {
                            content,
                            font_size,
                            align,
                            rotation,
                            flip_h,
                            flip_v,
                        } = element
                        {
                            let seg = render_text_segment_scaled(
                                renderer,
                                content,
                                *font_size,
                                *align,
                                *rotation,
                                tape_width_px,
                                &font_settings,
                                low,
                            )
                            .map(|s| s.mirrored(*flip_h, *flip_v));
                            min_segments.push(seg);
                        } else {
                            min_segments.push(None);
                        }
                    }
                    min_segments
                }
            };
        }
    }

    // Concatenate segments in composition order.
    let mut result: Option<LabelBitmap> = None;
    for (non_text, text) in non_text_segments.into_iter().zip(text_segments) {
        let Some(segment) = text.or(non_text) else {
            continue;
        };
        result = Some(match result {
            Some(prev) => prev.append(&segment),
            None => segment,
        });
    }

    Ok(result)
}

/// Render a single text element to a tape-height bitmap with an optional font scale factor,
/// or `None` to skip it.
#[allow(clippy::too_many_arguments)]
fn render_text_segment_scaled(
    renderer: &mut TextRenderer,
    content: &str,
    font_size: Option<f32>,
    align: TextAlign,
    rotation: f32,
    tape_width_px: u32,
    font: &FontSettings,
    scale: f32,
) -> Option<LabelBitmap> {
    if content.is_empty() {
        return None;
    }
    let lines: Vec<&str> = content.lines().collect();
    let is_rotated = is_rotated(rotation);

    // For rotated text with auto size, pick a size whose rotated bounding box
    // fits within the tape height.
    let base_font_size = rotation_aware_font_size(font_size, rotation, &lines, tape_width_px);

    let effective_font_size = if (scale - 1.0).abs() < 1e-5 {
        base_font_size
    } else {
        let unscaled = base_font_size.unwrap_or_else(|| {
            let num_lines = lines.len().max(1) as f32;
            let available_height = tape_width_px.saturating_sub(font.margin * 2) as f32;
            (available_height / (num_lines * 1.2)).max(4.0)
        });
        Some((unscaled * scale).max(1.0))
    };

    // Rotated text needs a taller render area so all lines stay visible; the
    // height becomes tape length after rotation.
    let render_height = if is_rotated {
        if let Some(fs) = effective_font_size {
            let line_h = (fs * 1.2).ceil();
            let text_h = (lines.len() as f32 * line_h).ceil() as u32 + font.margin * 2;
            text_h.max(tape_width_px)
        } else {
            tape_width_px
        }
    } else {
        tape_width_px
    };

    let bmp = match renderer.render_text(
        &lines,
        render_height,
        font.name,
        effective_font_size,
        font.margin,
        align,
    ) {
        Ok(bmp) => bmp,
        Err(e) => {
            error!("Text render failed: {}", e);
            return None;
        }
    };

    if is_rotated {
        // Trim whitespace so the rotated bounding box reflects actual content,
        // not full tape-height padding.
        Some(
            bmp.trim_vertical()
                .rotate(rotation)
                .fit_height(tape_width_px),
        )
    } else {
        Some(bmp)
    }
}

/// Render a single image element to a tape-height bitmap, or `None` to skip it.
fn render_image_segment(
    cached: Option<&LabelBitmap>,
    image_data: &[u8],
    rotation: f32,
    target_height: Option<u32>,
    tape_width_px: u32,
) -> Option<LabelBitmap> {
    let bmp = if let Some(bmp) = cached {
        bmp.clone()
    } else if !image_data.is_empty() {
        match image_loader::load_image_from_reader(
            Cursor::new(image_data),
            &ImageLoadOptions::default(),
        ) {
            Ok(bmp) => bmp,
            Err(e) => {
                error!("Image decode failed: {}", e);
                return None;
            }
        }
    } else {
        error!("Image element has no data");
        return None;
    };

    // Scale to target height (auto = tape height, manual = specified).
    let bmp = bmp.scale_to_height(target_height.unwrap_or(tape_width_px));

    if is_rotated(rotation) {
        Some(bmp.rotate(rotation).fit_height(tape_width_px))
    } else {
        Some(bmp.fit_height(tape_width_px))
    }
}

/// Render a QR code element to a tape-height bitmap, or `None` to skip it.
fn render_qr_segment(
    _cached: Option<&LabelBitmap>,
    content: &str,
    rotation: f32,
    target_height: Option<u32>,
    tape_width_px: u32,
) -> Option<LabelBitmap> {
    if content.trim().is_empty() {
        return None;
    }
    let desired_h = target_height.unwrap_or(tape_width_px).min(tape_width_px);
    let mut qr_bmp = crate::qr::render_qr_code(content, desired_h).ok()?;

    if is_rotated(rotation) {
        qr_bmp = qr_bmp.rotate(rotation);
    }

    Some(qr_bmp.fit_height(tape_width_px))
}

/// Returns true if the angle is not effectively a multiple of 360 degrees.
fn is_rotated(rotation_deg: f32) -> bool {
    let norm = ((rotation_deg % 360.0) + 360.0) % 360.0;
    !(norm.abs() < 0.5 || (norm - 360.0).abs() < 0.5)
}

/// Calculate a font size that fits within `tape_height` after rotation.
///
/// For 0 degrees, returns `font_size` unchanged (`None` lets the renderer
/// auto-size). For other angles, estimates the largest font size whose rotated
/// bounding box fits within the tape height.
fn rotation_aware_font_size(
    font_size: Option<f32>,
    rotation_deg: f32,
    lines: &[&str],
    tape_height: u32,
) -> Option<f32> {
    // User-specified font size: use it directly, no auto-adjustment.
    if font_size.is_some() {
        return font_size;
    }
    if !is_rotated(rotation_deg) {
        return None;
    }

    let norm = ((rotation_deg % 360.0) + 360.0) % 360.0;
    let angle_rad = norm.to_radians();
    let sin_a = angle_rad.sin().abs();
    let cos_a = angle_rad.cos().abs();
    let num_lines = lines.len().max(1) as f32;
    let max_chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1) as f32;
    let available = tape_height as f32;

    // Rotated bounding-box height:
    //   bbox_h = text_width * |sin| + text_height * |cos|
    // with text_width  ~ max_chars * font_size * 0.6
    //      text_height ~ num_lines * font_size * 1.2
    // Solve for font_size.
    let denom = max_chars * 0.6 * sin_a + num_lines * 1.2 * cos_a;
    if denom > 0.01 {
        Some((available / denom).max(4.0))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a solid black image of the given size as PNG bytes.
    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 255]));
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(Cursor::new(&mut buf));
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            w,
            h,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        buf
    }

    /// Encode a PNG whose left half is black and right half is white.
    fn png_left_black(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(w, h, |x, _| {
            if x < w / 2 {
                image::Rgba([0, 0, 0, 255])
            } else {
                image::Rgba([255, 255, 255, 255])
            }
        });
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(Cursor::new(&mut buf));
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            w,
            h,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        buf
    }

    /// Build an image element with an explicit horizontal flip.
    fn flipped_image_element(image_data: Vec<u8>, flip_h: bool) -> LabelElement {
        match LabelElement::image_from_bytes(None, image_data) {
            LabelElement::Image {
                path,
                image_data,
                bitmap,
                rotation,
                target_height,
                flip_v,
                ..
            } => LabelElement::Image {
                path,
                image_data,
                bitmap,
                rotation,
                target_height,
                flip_h,
                flip_v,
            },
            other => other,
        }
    }

    #[test]
    fn test_per_element_flip_h_mirrors_image() {
        let mut renderer = TextRenderer::new();
        let data = png_left_black(40, 40);

        let plain = render_elements(
            &[flipped_image_element(data.clone(), false)],
            64,
            "",
            0,
            &mut renderer,
        )
        .unwrap()
        .unwrap();
        let flipped = render_elements(
            &[flipped_image_element(data, true)],
            64,
            "",
            0,
            &mut renderer,
        )
        .unwrap()
        .unwrap();

        assert_eq!(plain.width(), flipped.width());
        assert_eq!(plain.height(), flipped.height());
        let w = plain.width();

        // The flipped element is the plain one mirrored left-right: pixel (x,y)
        // in the flip equals (w-1-x, y) in the plain. This holds whichever side
        // binarization decided was black.
        let mut differs = false;
        for y in 0..plain.height() {
            for x in 0..w {
                assert_eq!(
                    flipped.get_pixel(x, y),
                    plain.get_pixel(w - 1 - x, y),
                    "mirror mismatch at ({}, {})",
                    x,
                    y
                );
                if plain.get_pixel(x, y) != plain.get_pixel(w - 1 - x, y) {
                    differs = true;
                }
            }
        }
        // The source is asymmetric, so the flip must actually change something.
        assert!(
            differs,
            "expected an asymmetric image so the flip is visible"
        );
    }

    #[test]
    fn test_flip_fields_round_trip() {
        let mut doc = sample_document();
        doc.flip_h = true;
        if let LabelElement::Text { flip_v, .. } = &mut doc.elements[0] {
            *flip_v = true;
        }
        let parsed = LabelDocument::from_toml_str(&doc.to_toml_string().unwrap()).unwrap();
        assert!(parsed.flip_h);
        assert!(!parsed.flip_v);
        match &parsed.elements[0] {
            LabelElement::Text { flip_h, flip_v, .. } => {
                assert!(!flip_h);
                assert!(flip_v);
            }
            _ => panic!("expected text element"),
        }
    }

    #[test]
    fn test_old_layout_without_flip_defaults_false() {
        // A v1 layout written before the mirror feature has no flip fields.
        let text = "version = 1\ntape_width_mm = 12\nfont_name = \"x\"\n\
                    font_margin = 0\n\n[[elements]]\ntype = \"text\"\n\
                    content = \"hi\"\nalign = \"left\"\nrotation = 0.0\n";
        let doc = LabelDocument::from_toml_str(text).unwrap();
        assert!(!doc.flip_h);
        assert!(!doc.flip_v);
        match &doc.elements[0] {
            LabelElement::Text { flip_h, flip_v, .. } => {
                assert!(!flip_h);
                assert!(!flip_v);
            }
            _ => panic!("expected text element"),
        }
    }

    #[test]
    fn test_empty_elements_returns_none() {
        let mut renderer = TextRenderer::new();
        let result = render_elements(&[], 64, "", 0, &mut renderer).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_padding_and_cutmark_dimensions() {
        let mut renderer = TextRenderer::new();
        let elements = vec![LabelElement::Padding { pixels: 10 }, LabelElement::CutMark];
        let bmp = render_elements(&elements, 64, "", 0, &mut renderer)
            .unwrap()
            .unwrap();
        assert_eq!(bmp.height(), 64);
        // Padding (10) followed by the 9-pixel cut mark.
        assert_eq!(bmp.width(), 19);
    }

    #[test]
    fn test_empty_text_is_skipped() {
        let mut renderer = TextRenderer::new();
        let elements = vec![LabelElement::Text {
            content: String::new(),
            font_size: Some(24.0),
            align: TextAlign::Left,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        }];
        let result = render_elements(&elements, 64, "", 0, &mut renderer).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_image_element_fits_tape_height() {
        let mut renderer = TextRenderer::new();
        let element = LabelElement::image_from_bytes(None, png_bytes(40, 40));
        let bmp = render_elements(&[element], 64, "", 0, &mut renderer)
            .unwrap()
            .unwrap();
        assert_eq!(bmp.height(), 64);
        assert!(bmp.width() > 0);
    }

    #[test]
    fn test_rotated_image_fits_tape_height() {
        let mut renderer = TextRenderer::new();
        let element = match LabelElement::image_from_bytes(None, png_bytes(40, 20)) {
            LabelElement::Image {
                path,
                image_data,
                bitmap,
                target_height,
                ..
            } => LabelElement::Image {
                path,
                image_data,
                bitmap,
                rotation: 90.0,
                target_height,
                flip_h: false,
                flip_v: false,
            },
            other => other,
        };
        let bmp = render_elements(&[element], 64, "", 0, &mut renderer)
            .unwrap()
            .unwrap();
        assert_eq!(bmp.height(), 64);
    }

    /// Build a small document containing one of each element kind.
    fn sample_document() -> LabelDocument {
        LabelDocument {
            version: DOCUMENT_VERSION,
            tape_width_mm: 12,
            dpi: 180,
            font_name: "DejaVuSans".into(),
            font_margin: 2,
            flip_h: false,
            flip_v: false,
            margin_mm: None,
            max_length_mm: None,
            fit_largest_font: false,
            elements: vec![
                LabelElement::Text {
                    content: "Hi".into(),
                    font_size: Some(24.0),
                    align: TextAlign::Center,
                    rotation: 0.0,
                    flip_h: false,
                    flip_v: false,
                },
                LabelElement::image_from_bytes(Some("logo.png".into()), png_bytes(8, 8)),
                LabelElement::CutMark,
                LabelElement::Padding { pixels: 20 },
            ],
        }
    }

    #[test]
    fn test_document_round_trip_all_kinds() {
        let doc = sample_document();
        let text = doc.to_toml_string().unwrap();
        let parsed = LabelDocument::from_toml_str(&text).unwrap();

        assert_eq!(parsed.elements.len(), 4);
        assert_eq!(parsed.tape_width_mm, 12);
        assert_eq!(parsed.font_name, "DejaVuSans");

        // Embedded image bytes survive the round-trip exactly.
        match (&doc.elements[1], &parsed.elements[1]) {
            (
                LabelElement::Image { image_data: a, .. },
                LabelElement::Image {
                    image_data: b,
                    bitmap,
                    ..
                },
            ) => {
                assert_eq!(a, b);
                // The render cache is rebuilt on load.
                assert!(bitmap.is_some());
            }
            _ => panic!("expected image elements"),
        }
    }

    #[test]
    fn test_cutmark_serializes_as_tagged_table() {
        let doc = sample_document();
        let text = doc.to_toml_string().unwrap();
        assert!(text.contains("cut_mark"));
    }

    #[test]
    fn test_newer_version_is_rejected() {
        let doc = LabelDocument {
            version: DOCUMENT_VERSION + 1,
            tape_width_mm: 12,
            dpi: 180,
            font_name: "x".into(),
            font_margin: 0,
            flip_h: false,
            flip_v: false,
            margin_mm: None,
            max_length_mm: None,
            fit_largest_font: false,
            elements: vec![LabelElement::CutMark],
        };
        let text = doc.to_toml_string().unwrap();
        assert!(LabelDocument::from_toml_str(&text).is_err());
    }

    #[test]
    fn test_corrupt_image_data_is_rejected() {
        let doc = LabelDocument {
            version: DOCUMENT_VERSION,
            tape_width_mm: 12,
            dpi: 180,
            font_name: "x".into(),
            font_margin: 0,
            flip_h: false,
            flip_v: false,
            margin_mm: None,
            max_length_mm: None,
            fit_largest_font: false,
            elements: vec![LabelElement::Image {
                path: None,
                image_data: b"not a real image".to_vec(),
                bitmap: None,
                rotation: 0.0,
                target_height: None,
                flip_h: false,
                flip_v: false,
            }],
        };
        let text = doc.to_toml_string().unwrap();
        assert!(LabelDocument::from_toml_str(&text).is_err());
    }

    #[test]
    fn test_invalid_base64_is_rejected() {
        let text = "version = 1\ntape_width_mm = 12\nfont_name = \"x\"\n\
                    font_margin = 0\n\n[[elements]]\ntype = \"image\"\n\
                    image_data = \"not valid base64 !!!\"\nrotation = 0.0\n";
        assert!(LabelDocument::from_toml_str(text).is_err());
    }

    fn text_doc(content: &str) -> LabelDocument {
        LabelDocument {
            version: DOCUMENT_VERSION,
            tape_width_mm: 12,
            dpi: 180,
            font_name: "x".into(),
            font_margin: 0,
            flip_h: false,
            flip_v: false,
            margin_mm: None,
            max_length_mm: None,
            fit_largest_font: false,
            elements: vec![LabelElement::Text {
                content: content.into(),
                font_size: Some(24.0),
                align: TextAlign::Left,
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
            }],
        }
    }

    fn text_of(doc: &LabelDocument) -> &str {
        match &doc.elements[0] {
            LabelElement::Text { content, .. } => content,
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn test_placeholders_unique_sorted() {
        let doc = text_doc("{{name}} {{id}}\n{{name}}");
        assert_eq!(
            doc.placeholders(),
            vec!["id".to_string(), "name".to_string()]
        );
    }

    #[test]
    fn test_placeholders_ignore_invalid() {
        // Spaces make it not a placeholder; it stays literal and is not listed.
        let doc = text_doc("{{ not a var }} {{ok}}");
        assert_eq!(doc.placeholders(), vec!["ok".to_string()]);
    }

    #[test]
    fn test_apply_values_substitutes() {
        let mut doc = text_doc("Hi {{name}} ({{id}})");
        let mut values = BTreeMap::new();
        values.insert("name".to_string(), "Alice".to_string());
        values.insert("id".to_string(), "A001".to_string());
        doc.apply_values(&values);
        assert_eq!(text_of(&doc), "Hi Alice (A001)");
    }

    #[test]
    fn test_apply_values_blanks_unresolved() {
        let mut doc = text_doc("Hi {{name}}!");
        doc.apply_values(&BTreeMap::new());
        assert_eq!(text_of(&doc), "Hi !");
    }

    #[test]
    fn test_apply_values_trims_and_keeps_literals() {
        let mut doc = text_doc("{{ name }} {{bad var}}");
        let mut values = BTreeMap::new();
        values.insert("name".to_string(), "Bob".to_string());
        doc.apply_values(&values);
        // Whitespace inside braces is tolerated; the invalid one stays literal.
        assert_eq!(text_of(&doc), "Bob {{bad var}}");
    }

    #[test]
    fn test_unterminated_placeholder_left_intact() {
        let mut doc = text_doc("price {{name");
        doc.apply_values(&BTreeMap::new());
        assert_eq!(text_of(&doc), "price {{name");
    }

    #[test]
    fn test_image_from_bytes_decodes_cache() {
        let element = LabelElement::image_from_bytes(Some("logo.png".into()), png_bytes(8, 8));
        match element {
            LabelElement::Image { bitmap, path, .. } => {
                assert!(bitmap.is_some());
                assert_eq!(path.unwrap().file_name().unwrap(), "logo.png");
            }
            _ => panic!("expected image element"),
        }
    }

    #[test]
    fn test_label_batch_round_trip() {
        let doc1 = text_doc("Label 1");
        let doc2 = text_doc("Label 2");
        let batch = LabelBatch {
            version: DOCUMENT_VERSION,
            tape_width_mm: 12,
            dpi: 180,
            labels: vec![
                BatchEntry {
                    title: "First".to_string(),
                    copies: 3,
                    document: doc1,
                },
                BatchEntry {
                    title: "Second".to_string(),
                    copies: 1,
                    document: doc2,
                },
            ],
        };
        let toml_str = batch.to_toml_string().expect("serialize batch");
        let parsed = LabelBatch::from_toml_str(&toml_str).expect("deserialize batch");
        assert_eq!(parsed.labels.len(), 2);
        assert_eq!(parsed.labels[0].title, "First");
        assert_eq!(parsed.labels[0].copies, 3);
        assert_eq!(parsed.labels[1].title, "Second");
        assert_eq!(parsed.labels[1].copies, 1);
    }

    #[test]
    fn test_label_batch_fallback_from_single_doc() {
        let doc = text_doc("Single Label");
        let toml_str = doc.to_toml_string().expect("serialize doc");
        let parsed = LabelBatch::from_toml_str(&toml_str).expect("deserialize single as batch");
        assert_eq!(parsed.labels.len(), 1);
        assert_eq!(parsed.labels[0].copies, 1);
        assert_eq!(text_of(&parsed.labels[0].document), "Single Label");
    }

    #[test]
    fn test_margin_mm_and_pretrim_detection() {
        let mut doc = text_doc("Compact");
        assert!(!doc.is_pretrim());
        assert_eq!(doc.effective_margin_mm(), 27.0);

        doc.margin_mm = Some(3.0);
        assert!(doc.is_pretrim());
        assert_eq!(doc.effective_margin_mm(), 3.0);

        doc.margin_mm = Some(25.0);
        assert!(!doc.is_pretrim());
        assert_eq!(doc.effective_margin_mm(), 25.0);

        let toml_str = doc.to_toml_string().unwrap();
        let parsed = LabelDocument::from_toml_str(&toml_str).unwrap();
        assert_eq!(parsed.margin_mm, Some(25.0));
    }

    #[test]
    fn test_max_length_mm_auto_margin_and_confinement() {
        let mut doc =
            text_doc("Very Long Label Text That Needs Auto-Shrinking To Fit Inside 100mm");
        doc.font_name = "Inter".into();
        if let LabelElement::Text { font_size, .. } = &mut doc.elements[0] {
            *font_size = None;
        }

        // Default ~27mm margin with 100mm max length leaves 46mm for content
        doc.max_length_mm = Some(100.0);
        assert!(!doc.is_pretrim());
        assert_eq!(doc.effective_margin_mm(), 27.0);
        assert_eq!(doc.max_content_length_mm(), Some(46.0));

        // When max_length_mm is 50mm (where 2 * 27mm = 54mm won't fit), auto-switches to 3mm pre-trim
        doc.max_length_mm = Some(50.0);
        assert!(doc.is_pretrim());
        assert_eq!(doc.effective_margin_mm(), 3.0);
        assert_eq!(doc.max_content_length_mm(), Some(44.0));

        // Round-trip through TOML preserves max_length_mm
        let toml_str = doc.to_toml_string().unwrap();
        let parsed = LabelDocument::from_toml_str(&toml_str).unwrap();
        assert_eq!(parsed.max_length_mm, Some(50.0));

        // Render unconstrained vs constrained
        let mut renderer = TextRenderer::new();
        let unconstrained = render_elements(&doc.elements, 76, &doc.font_name, 0, &mut renderer)
            .unwrap()
            .unwrap();
        let max_px = doc.max_content_width_px(180).unwrap();
        let constrained = render_elements_with_max_width(
            &doc.elements,
            76,
            &doc.font_name,
            0,
            Some(max_px),
            &mut renderer,
        )
        .unwrap()
        .unwrap();

        assert!(unconstrained.width() > max_px);
        assert!(
            constrained.width() <= max_px,
            "constrained width {} should be <= max_px {}",
            constrained.width(),
            max_px
        );

        // Short text that already fits within max_px keeps its exact natural width
        let short_doc = LabelDocument {
            max_length_mm: Some(100.0),
            margin_mm: Some(3.0),
            ..text_doc("OK")
        };
        let short_unconstrained =
            render_elements(&short_doc.elements, 76, "Inter", 0, &mut renderer)
                .unwrap()
                .unwrap();
        let short_max_px = short_doc.max_content_width_px(180).unwrap();
        let short_constrained = render_elements_with_max_width(
            &short_doc.elements,
            76,
            "Inter",
            0,
            Some(short_max_px),
            &mut renderer,
        )
        .unwrap()
        .unwrap();
        assert_eq!(short_unconstrained.width(), short_constrained.width());
    }

    #[test]
    fn test_fit_largest_font_in_confined_area() {
        let mut doc = text_doc("SERVER RACK A-01\n10.0.4.15");
        doc.font_name = "Inter".into();
        doc.max_length_mm = Some(100.0);
        doc.fit_largest_font = true;

        // With fit_largest_font enabled and no explicit margin, 100mm label uses 3mm compact margin (94mm content)
        assert!(doc.is_pretrim());
        assert_eq!(doc.effective_margin_mm(), 3.0);
        assert_eq!(doc.max_content_length_mm(), Some(94.0));

        let mut renderer = TextRenderer::new();
        let max_px = doc.max_content_width_px(180).unwrap();
        let fitted = render_elements_with_options(
            &doc.elements,
            128,
            &doc.font_name,
            0,
            Some(max_px),
            true,
            &mut renderer,
        )
        .unwrap()
        .unwrap();

        assert!(
            fitted.width() <= max_px,
            "fitted width {} should be <= max_px {}",
            fitted.width(),
            max_px
        );
        // Should be significantly wider than the manual 24pt render because it scales UP to fill the 128px tape height!
        let small = render_elements_with_options(
            &doc.elements,
            128,
            &doc.font_name,
            0,
            Some(max_px),
            false,
            &mut renderer,
        )
        .unwrap()
        .unwrap();
        assert!(
            fitted.width() > small.width() * 2,
            "fit_largest_font width {} should be > 2x small width {}",
            fitted.width(),
            small.width()
        );
    }

    #[test]
    fn test_qrcode_element_serialization_and_rendering() {
        let doc = LabelDocument {
            version: DOCUMENT_VERSION,
            tape_width_mm: 24,
            dpi: 180,
            font_name: "Inter".into(),
            font_margin: 0,
            flip_h: false,
            flip_v: false,
            margin_mm: None,
            max_length_mm: None,
            fit_largest_font: false,
            elements: vec![
                LabelElement::qr_from_content("https://example.com"),
                LabelElement::Text {
                    content: "Scan Me".into(),
                    font_size: None,
                    align: TextAlign::Center,
                    rotation: 0.0,
                    flip_h: false,
                    flip_v: false,
                },
            ],
        };

        // Test serialization round trip
        let toml_str = doc.to_toml_string().expect("serialize doc with qr");
        assert!(toml_str.contains("type = \"qr_code\""));
        assert!(toml_str.contains("content = \"https://example.com\""));

        let parsed = LabelDocument::from_toml_str(&toml_str).expect("deserialize doc with qr");
        assert_eq!(parsed.elements.len(), 2);
        assert_eq!(parsed.elements[0].display_name(), "QR: https://example.com");

        // Test rendering
        let mut renderer = TextRenderer::new();
        let rendered = render_elements(&parsed.elements, 128, &parsed.font_name, 0, &mut renderer)
            .expect("render")
            .expect("not empty");
        assert_eq!(rendered.height(), 128);
        assert!(rendered.width() > 100);
    }

    #[test]
    fn test_qr_document_preserves_payload_whitespace() {
        let content = "\tASSET-42\r\n";
        let mut doc = text_doc("");
        doc.elements = vec![LabelElement::qr_from_content("{{asset}}")];
        doc.apply_values(&BTreeMap::from([("asset".into(), content.into())]));
        let loaded = LabelDocument::from_toml_str(&doc.to_toml_string().unwrap()).unwrap();
        let mut renderer = TextRenderer::new();
        let bitmap = render_elements(&loaded.elements, 128, "", 0, &mut renderer)
            .unwrap()
            .unwrap();

        // Compare the composed modules to the exact template value, not a
        // second call to our QR renderer (which could repeat the same bug).
        let code = qrcode::QrCode::new(content.as_bytes()).unwrap();
        let modules = code.width() as u32;
        let scale = 128 / (modules + 4);
        let size = (modules + 4) * scale;
        assert_eq!(bitmap.width(), size);
        assert_eq!(bitmap.height(), 128);
        let y_offset = (128 - size) / 2;
        for y in 0..modules {
            for x in 0..modules {
                assert_eq!(
                    bitmap.get_pixel((x + 2) * scale, y_offset + (y + 2) * scale),
                    code[(x as usize, y as usize)] == qrcode::Color::Dark,
                    "module ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn test_qrcode_placeholders_and_values() {
        let mut doc = LabelDocument {
            version: DOCUMENT_VERSION,
            tape_width_mm: 24,
            dpi: 180,
            font_name: "Inter".into(),
            font_margin: 0,
            flip_h: false,
            flip_v: false,
            margin_mm: None,
            max_length_mm: None,
            fit_largest_font: false,
            elements: vec![LabelElement::qr_from_content(
                "https://example.com/item/{{item_id}}",
            )],
        };

        let placeholders = doc.placeholders();
        assert_eq!(placeholders, vec!["item_id"]);

        let mut values = BTreeMap::new();
        values.insert("item_id".to_string(), "ABC-123".to_string());
        doc.apply_values(&values);

        if let LabelElement::QrCode { content, .. } = &doc.elements[0] {
            assert_eq!(content, "https://example.com/item/ABC-123");
        } else {
            panic!("Expected QrCode element");
        }
    }
}
