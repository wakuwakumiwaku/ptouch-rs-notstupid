// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Huang Rui <vowstar@gmail.com>

//! Text rendering using cosmic-text.
//!
//! Renders multi-line text into a [`LabelBitmap`] using system fonts.
//! Supports auto-sizing, alignment, and font selection.

use cosmic_text::{Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache};
use serde::{Deserialize, Serialize};

use crate::RenderError;
use crate::Result;
use crate::bitmap::LabelBitmap;

/// Text alignment within the label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    /// Align text to the left edge.
    Left,
    /// Center text horizontally.
    Center,
    /// Align text to the right edge.
    Right,
}

impl TextAlign {
    /// Convert to cosmic-text's alignment type.
    fn to_cosmic(self) -> Align {
        match self {
            TextAlign::Left => Align::Left,
            TextAlign::Center => Align::Center,
            TextAlign::Right => Align::Right,
        }
    }
}

/// Text renderer backed by cosmic-text.
pub struct TextRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRenderer {
    /// Create a new text renderer with system fonts loaded.
    pub fn new() -> Self {
        Self {
            font_system: crate::font::create_font_system(),
            swash_cache: SwashCache::new(),
        }
    }

    /// Render one or more lines of text into a [`LabelBitmap`].
    ///
    /// - `lines`: text lines to render (joined with newlines)
    /// - `print_width`: height of the tape in pixels (this becomes the bitmap
    ///   height; called "print_width" for compatibility with the C API where
    ///   the tape width is the printable width)
    /// - `font_name`: font family name to use (e.g. "DejaVu Sans")
    /// - `font_size`: explicit font size in points, or `None` for auto-detect
    /// - `font_margin`: margin in pixels on each side (top and bottom)
    /// - `align`: horizontal text alignment
    ///
    /// The renderer will:
    /// 1. If font_size is None, auto-detect the largest size that fits
    /// 2. Render the text into a buffer
    /// 3. Convert the rendered glyphs to a 1-bit bitmap
    pub fn render_text(
        &mut self,
        lines: &[&str],
        print_width: u32,
        font_name: &str,
        font_size: Option<f32>,
        font_margin: u32,
        align: TextAlign,
    ) -> Result<LabelBitmap> {
        if print_width == 0 {
            return Err(RenderError::Text("print_width must be > 0".into()));
        }

        let text = lines.join("\n");
        if text.is_empty() {
            return Err(RenderError::Text("no text to render".into()));
        }

        let num_lines = lines.len() as f32;
        let available_height = print_width.saturating_sub(font_margin * 2) as f32;

        if available_height <= 0.0 {
            return Err(RenderError::Text(
                "font_margin too large for tape width".into(),
            ));
        }

        // Determine font size
        let font_size = font_size.unwrap_or_else(|| {
            // Auto-detect: largest size where all lines fit vertically.
            // Line height ~ font_size * 1.2, total ~ line_height * num_lines
            let size = available_height / (num_lines * 1.2);
            size.max(4.0)
        });

        let line_height = (font_size * 1.2).ceil();
        let metrics = Metrics::new(font_size, line_height);

        let resolved = crate::font::find_font(font_name);
        let effective_name = resolved.as_deref().unwrap_or(font_name);
        let family = if effective_name.is_empty() {
            Family::SansSerif
        } else {
            Family::Name(effective_name)
        };
        let attrs = Attrs::new().family(family);
        let cosmic_align = Some(align.to_cosmic());

        // We do not know the final horizontal width yet. Use a large initial
        // width so cosmic-text does not wrap, then we measure the actual
        // extent and create a tight bitmap.
        let layout_width = 16384.0f32;

        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(Some(layout_width), Some(available_height));
        buffer.set_text(&text, &attrs, Shaping::Advanced, cosmic_align);
        buffer.shape_until_scroll(&mut self.font_system, true);

        // Measure tight horizontal extent (min_x..max_x) from layout runs.
        // With Center/Right alignment, cosmic-text offsets glyphs within the
        // large layout_width. We subtract min_x so the bitmap is tight.
        let mut min_x: f32 = f32::MAX;
        let mut max_x: f32 = 0.0;
        for run in buffer.layout_runs() {
            for g in run.glyphs.iter() {
                min_x = min_x.min(g.x);
                max_x = max_x.max(g.x + g.w);
            }
        }
        if min_x == f32::MAX {
            min_x = 0.0;
        }

        let bitmap_width = ((max_x - min_x).ceil() as u32).max(1);
        let bitmap_height = print_width;
        let x_offset = min_x.floor() as i32;

        let mut bitmap = LabelBitmap::new(bitmap_width, bitmap_height);

        // Vertical centering offset
        let total_text_height = (num_lines * line_height) as u32;
        let y_offset = if total_text_height < bitmap_height {
            ((bitmap_height - total_text_height) / 2) as i32
        } else {
            font_margin as i32
        };

        // Draw glyphs onto the bitmap
        let text_color = Color::rgb(0, 0, 0);
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            text_color,
            |x, y, w, h, color| {
                let alpha = color.a();
                if alpha < 128 {
                    return;
                }
                let px = x - x_offset;
                let py = y + y_offset;
                for dy in 0..h as i32 {
                    for dx in 0..w as i32 {
                        let fx = px + dx;
                        let fy = py + dy;
                        if fx >= 0 && fy >= 0 {
                            bitmap.set_pixel(fx as u32, fy as u32, true);
                        }
                    }
                }
            },
        );

        Ok(bitmap)
    }

    /// Render text using the largest possible font size whose actual glyph ink
    /// bounding box fits within `(max_width_px, print_width - 2 * font_margin)`.
    ///
    /// When `allow_wrap` is true and a single-line input contains spaces, this
    /// also evaluates balanced 2-line and 3-line word-wrapped splits and picks
    /// whichever layout yields the largest character size (`font_size`).
    #[allow(clippy::too_many_arguments)]
    pub fn render_text_fit_box(
        &mut self,
        lines: &[&str],
        print_width: u32,
        max_width_px: Option<u32>,
        font_name: &str,
        font_margin: u32,
        align: TextAlign,
        allow_wrap: bool,
    ) -> Result<LabelBitmap> {
        if print_width == 0 {
            return Err(RenderError::Text("print_width must be > 0".into()));
        }

        let text = lines.join("\n");
        if text.trim().is_empty() {
            return self.render_text(lines, print_width, font_name, None, font_margin, align);
        }

        let eff_margin = font_margin.max(2).min(print_width / 4);
        let avail_h = print_width.saturating_sub(eff_margin * 2).max(1);
        let avail_w = max_width_px.unwrap_or(16384).max(1);

        let mut candidates = vec![text.clone()];
        if allow_wrap && lines.len() == 1 && max_width_px.is_some() {
            let single = lines[0].trim();
            if single.contains(' ') {
                let two_line = split_balanced_n_lines(single, 2);
                if two_line != text {
                    candidates.push(two_line);
                }
                if single.matches(' ').count() >= 2 && avail_h >= 36 {
                    let three_line = split_balanced_n_lines(single, 3);
                    if !candidates.contains(&three_line) {
                        candidates.push(three_line);
                    }
                }
            }
        }

        let mut best_candidate = text;
        let mut best_font_size = 2.0_f32;

        for (idx, cand) in candidates.into_iter().enumerate() {
            let num_lines = cand.lines().count().max(1);
            let line_factor = if num_lines > 1 { 1.05 } else { 1.15 };

            let mut low = 2.0_f32;
            let mut high = (avail_h as f32) * 1.8;
            let mut cand_best = low;

            for _ in 0..13 {
                let mid = (low + high) * 0.5;
                if let Some((min_x, max_x, min_y, max_y)) =
                    self.measure_ink_bbox(&cand, mid, line_factor, font_name, align)
                {
                    let ink_w = (max_x - min_x).max(1) as u32;
                    let ink_h = (max_y - min_y).max(1) as u32;
                    if ink_w <= avail_w && ink_h <= avail_h {
                        cand_best = mid;
                        low = mid;
                    } else {
                        high = mid;
                    }
                } else {
                    high = mid;
                }
            }

            if idx == 0 {
                best_font_size = cand_best;
                best_candidate = cand;
            } else if cand_best > best_font_size * 1.12 {
                // Only prefer wrapped multi-line layout if it yields noticeably larger letters.
                best_font_size = cand_best;
                best_candidate = cand;
            }
        }

        let num_lines = best_candidate.lines().count().max(1);
        let line_factor = if num_lines > 1 { 1.05 } else { 1.15 };
        self.render_tight_at_size(
            &best_candidate,
            best_font_size,
            line_factor,
            print_width,
            font_name,
            align,
        )
    }

    /// Measure the exact pixel bounding box `(min_x, max_x, min_y, max_y)` of all
    /// glyph pixels with `alpha >= 128` for `text` at `font_size`.
    fn measure_ink_bbox(
        &mut self,
        text: &str,
        font_size: f32,
        line_factor: f32,
        font_name: &str,
        align: TextAlign,
    ) -> Option<(i32, i32, i32, i32)> {
        let line_height = (font_size * line_factor).ceil().max(1.0);
        let metrics = Metrics::new(font_size.max(1.0), line_height);

        let resolved = crate::font::find_font(font_name);
        let effective_name = resolved.as_deref().unwrap_or(font_name);
        let family = if effective_name.is_empty() {
            Family::SansSerif
        } else {
            Family::Name(effective_name)
        };
        let attrs = Attrs::new().family(family);
        let cosmic_align = Some(align.to_cosmic());

        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(Some(16384.0), None);
        buffer.set_text(text, &attrs, Shaping::Advanced, cosmic_align);
        buffer.shape_until_scroll(&mut self.font_system, true);

        let mut min_x = i32::MAX;
        let mut max_x = i32::MIN;
        let mut min_y = i32::MAX;
        let mut max_y = i32::MIN;

        let text_color = Color::rgb(0, 0, 0);
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            text_color,
            |x, y, w, h, color| {
                if color.a() >= 128 && w > 0 && h > 0 {
                    min_x = min_x.min(x);
                    max_x = max_x.max(x + w as i32);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y + h as i32);
                }
            },
        );

        if min_x <= max_x && min_y <= max_y {
            Some((min_x, max_x, min_y, max_y))
        } else {
            None
        }
    }

    /// Render `text` at `font_size` with tight horizontal bounds and vertically centered ink.
    fn render_tight_at_size(
        &mut self,
        text: &str,
        font_size: f32,
        line_factor: f32,
        print_width: u32,
        font_name: &str,
        align: TextAlign,
    ) -> Result<LabelBitmap> {
        let line_height = (font_size * line_factor).ceil().max(1.0);
        let metrics = Metrics::new(font_size.max(1.0), line_height);

        let resolved = crate::font::find_font(font_name);
        let effective_name = resolved.as_deref().unwrap_or(font_name);
        let family = if effective_name.is_empty() {
            Family::SansSerif
        } else {
            Family::Name(effective_name)
        };
        let attrs = Attrs::new().family(family);
        let cosmic_align = Some(align.to_cosmic());

        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(Some(16384.0), None);
        buffer.set_text(text, &attrs, Shaping::Advanced, cosmic_align);
        buffer.shape_until_scroll(&mut self.font_system, true);

        let mut min_x = i32::MAX;
        let mut max_x = i32::MIN;
        let mut min_y = i32::MAX;
        let mut max_y = i32::MIN;

        let text_color = Color::rgb(0, 0, 0);
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            text_color,
            |x, y, w, h, color| {
                if color.a() >= 128 && w > 0 && h > 0 {
                    min_x = min_x.min(x);
                    max_x = max_x.max(x + w as i32);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y + h as i32);
                }
            },
        );

        if min_x > max_x || min_y > max_y {
            return Ok(LabelBitmap::new(1, print_width));
        }

        let ink_w = ((max_x - min_x).max(1)) as u32;
        let ink_h = ((max_y - min_y).max(1)) as u32;
        let x_offset = min_x;
        let y_offset = ((print_width as i32 - ink_h as i32) / 2) - min_y;

        let mut bitmap = LabelBitmap::new(ink_w, print_width);
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            text_color,
            |x, y, w, h, color| {
                if color.a() < 128 {
                    return;
                }
                let px = x - x_offset;
                let py = y + y_offset;
                for dy in 0..h as i32 {
                    for dx in 0..w as i32 {
                        let fx = px + dx;
                        let fy = py + dy;
                        if fx >= 0 && fy >= 0 {
                            bitmap.set_pixel(fx as u32, fy as u32, true);
                        }
                    }
                }
            },
        );

        Ok(bitmap)
    }
}

/// Split a single-line string into `n` balanced lines at word boundaries.
fn split_balanced_n_lines(s: &str, n: usize) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.len() < n || n <= 1 {
        return s.to_string();
    }
    let total_len: usize = words.iter().map(|w| w.len()).sum::<usize>() + words.len() - 1;
    let mut lines = Vec::with_capacity(n);
    let mut start_idx = 0;

    for line_idx in 0..(n - 1) {
        let remaining_lines = n - line_idx;
        let remaining_words = &words[start_idx..];
        let rem_len: usize = remaining_words.iter().map(|w| w.len()).sum::<usize>()
            + remaining_words.len().saturating_sub(1);
        let target_len = rem_len / remaining_lines;

        let mut best_end = start_idx + 1;
        let mut best_diff = usize::MAX;
        let mut acc_len = 0;
        let max_end = words.len() - (remaining_lines - 1);

        for end in (start_idx + 1)..=max_end {
            let w_len = words[end - 1].len();
            acc_len += if end == start_idx + 1 {
                w_len
            } else {
                1 + w_len
            };
            let diff = acc_len.abs_diff(target_len);
            if diff <= best_diff {
                best_diff = diff;
                best_end = end;
            } else {
                break;
            }
        }
        lines.push(words[start_idx..best_end].join(" "));
        start_idx = best_end;
    }
    let _ = total_len;
    lines.push(words[start_idx..].join(" "));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_align_conversion() {
        assert_eq!(TextAlign::Left.to_cosmic(), Align::Left);
        assert_eq!(TextAlign::Center.to_cosmic(), Align::Center);
        assert_eq!(TextAlign::Right.to_cosmic(), Align::Right);
    }

    #[test]
    fn test_empty_text_error() {
        let mut renderer = TextRenderer::new();
        let result = renderer.render_text(&[], 64, "sans-serif", None, 2, TextAlign::Center);
        assert!(result.is_err());
    }

    #[test]
    fn test_zero_width_error() {
        let mut renderer = TextRenderer::new();
        let result = renderer.render_text(&["hello"], 0, "sans-serif", None, 0, TextAlign::Center);
        assert!(result.is_err());
    }

    #[test]
    fn test_render_basic_text() {
        let mut renderer = TextRenderer::new();
        // Use a generic family so it works even without specific fonts
        let result = renderer.render_text(&["Test"], 64, "", Some(24.0), 2, TextAlign::Left);
        // This may succeed or fail depending on available system fonts.
        // We just verify it does not panic.
        if let Ok(bmp) = result {
            assert!(bmp.width() > 0);
            assert_eq!(bmp.height(), 64);
        }
    }
}
