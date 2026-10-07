// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Huang Rui <vowstar@gmail.com>

//! Top toolbar panel with element addition and action buttons.

use std::path::PathBuf;

use log::{error, info};

use ptouch_render::document::LabelDocument;
use ptouch_render::raster;
use ptouch_render::text::TextAlign;

use crate::state::{AppState, LabelElement, PrinterCommand, ViewMode};

/// Render the top toolbar.
pub fn show_toolbar(ui: &mut egui::Ui, state: &mut AppState) {
    state.ensure_batch_initialized();

    ui.horizontal(|ui| {
        // -- View mode switcher --
        let batch_count = state.batch_items.len().max(1);
        let view_label = match state.view_mode {
            ViewMode::Designer => format!("📋 View All Labels ({batch_count})"),
            ViewMode::Batch => "✏ Switch to Designer Canvas".to_string(),
        };
        let view_btn = egui::Button::new(egui::RichText::new(view_label).strong().color(
            if state.view_mode == ViewMode::Batch {
                egui::Color32::from_rgb(30, 64, 175)
            } else {
                egui::Color32::from_rgb(20, 110, 40)
            },
        ));
        if ui.add(view_btn).clicked() {
            state.sync_active_to_batch();
            state.view_mode = match state.view_mode {
                ViewMode::Designer => ViewMode::Batch,
                ViewMode::Batch => ViewMode::Designer,
            };
        }

        ui.separator();

        // -- Element addition buttons (enabled in Designer mode) --
        let in_designer = state.view_mode == ViewMode::Designer;

        if ui
            .add_enabled(in_designer, egui::Button::new("Add Text"))
            .clicked()
        {
            state.elements.push(LabelElement::Text {
                content: "Label".to_string(),
                font_size: None,
                align: TextAlign::Left,
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
            });
            state.selected_element = Some(state.elements.len() - 1);
            state.mark_dirty();
            info!("Added text element");
        }

        if ui
            .add_enabled(in_designer, egui::Button::new("Add Image"))
            .clicked()
            && let Some(path) = crate::widgets::pick_image_file()
        {
            // Read the original source bytes so the image is embedded in the
            // label and stays self-contained when saved to a layout file.
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let element = LabelElement::image_from_bytes(Some(path.clone()), bytes);
                    // Reject files that do not decode, so a saved layout can
                    // always be reopened.
                    if matches!(element, LabelElement::Image { bitmap: None, .. }) {
                        error!("Unsupported or corrupt image: {}", path.display());
                        state.status_message = format!("Image load error: {}", path.display());
                    } else {
                        info!("Loaded image: {}", path.display());
                        state.elements.push(element);
                        state.selected_element = Some(state.elements.len() - 1);
                        state.mark_dirty();
                    }
                }
                Err(e) => {
                    error!("Failed to read image: {}", e);
                    state.status_message = format!("Image read error: {}", e);
                }
            }
        }

        if ui
            .add_enabled(in_designer, egui::Button::new("Add QR Code"))
            .clicked()
        {
            let element = LabelElement::qr_from_content("https://example.com");
            state.elements.push(element);
            state.selected_element = Some(state.elements.len() - 1);
            state.mark_dirty();
            info!("Added QR code element");
        }

        if ui
            .add_enabled(in_designer, egui::Button::new("Cut Mark"))
            .clicked()
        {
            state.elements.push(LabelElement::CutMark);
            state.selected_element = Some(state.elements.len() - 1);
            state.mark_dirty();
            info!("Added cut mark");
        }

        if ui
            .add_enabled(in_designer, egui::Button::new("Padding"))
            .clicked()
        {
            state.elements.push(LabelElement::Padding { pixels: 20 });
            state.selected_element = Some(state.elements.len() - 1);
            state.mark_dirty();
            info!("Added padding element");
        }

        ui.separator();

        // -- Action buttons --
        let connected = state.printer_connected;
        let busy = state.is_printer_busy();
        let has_bitmap = state.preview_bitmap.is_some() && !state.needs_rerender;

        // Print active label
        let can_print = connected && !busy && has_bitmap && state.copies > 0;
        let print_btn = egui::Button::new("Print");
        let print_resp = if can_print {
            ui.add(print_btn)
        } else {
            let reason = if !connected {
                "Printer is not connected (check USB cable or power)"
            } else if busy {
                "Printer is busy with an active operation..."
            } else if !has_bitmap {
                "Rendering label preview..."
            } else {
                "NUMBER TO PRINT is set to 0. Increment copies to print."
            };
            ui.add_enabled(false, print_btn)
                .on_disabled_hover_text(reason)
        };

        if print_resp.clicked()
            && let Some(ref bitmap) = state.preview_bitmap
        {
            let raw_lines = raster::bitmap_to_raster_lines(bitmap, state.printer_max_px);
            let dpi = if state.printer_dpi > 0 {
                state.printer_dpi as f32
            } else {
                180.0
            };
            let px_per_mm = dpi / 25.4;
            let copies = state.copies.max(1);

            let blank_line = vec![0u8; (state.printer_max_px as usize).div_ceil(8)];

            let is_pretrim = state.margin_mm.map(|m| m < 24.5).unwrap_or(false);
            let margin_mm = state.margin_mm.unwrap_or(27.0);

            let (raster_lines, chain_print, precut) =
                if state.cut_mode == crate::state::CutMarginMode::ChainPrint {
                    let margin_lines = (2.0 * px_per_mm).round() as usize;
                    let mut lines = Vec::with_capacity(margin_lines * 2 + raw_lines.len());
                    lines.extend(std::iter::repeat_n(blank_line.clone(), margin_lines));
                    lines.extend(raw_lines);
                    lines.extend(std::iter::repeat_n(blank_line, margin_lines));
                    (lines, true, false)
                } else if is_pretrim {
                    // Auto pre-trim: tight margin (e.g. 3mm or 5mm), pre-trims 24.5mm leader scrap snippet
                    let margin_lines = (margin_mm * px_per_mm).round() as usize;
                    let mut lines = Vec::with_capacity(margin_lines * 2 + raw_lines.len());
                    lines.extend(std::iter::repeat_n(blank_line.clone(), margin_lines));
                    lines.extend(raw_lines);
                    lines.extend(std::iter::repeat_n(blank_line, margin_lines));
                    (lines, false, true)
                } else {
                    // Centered / standard: uses natural 24.5mm hardware lead, 0 scrap snippet
                    let lead_mm: f32 = 24.5;
                    let leading_extra_mm = (margin_mm - lead_mm).max(0.0);
                    let leading_lines = (leading_extra_mm * px_per_mm).round() as usize;
                    let trailing_lines = (margin_mm * px_per_mm).round() as usize;

                    let mut lines =
                        Vec::with_capacity(leading_lines + raw_lines.len() + trailing_lines);
                    lines.extend(std::iter::repeat_n(blank_line.clone(), leading_lines));
                    lines.extend(raw_lines);
                    lines.extend(std::iter::repeat_n(blank_line, trailing_lines));
                    (lines, false, false)
                };

            // Track cumulative tape consumption
            let total_job_mm = if state.cut_mode == crate::state::CutMarginMode::ChainPrint {
                (raster_lines.len() as f32 / px_per_mm) * copies as f32
            } else {
                (24.5 + (raster_lines.len() as f32 / px_per_mm)) * copies as f32
            };
            state.tape_printed_meters += total_job_mm / 1000.0;

            if let Some(ref tx) = state.printer_cmd_tx {
                let _ = tx.send(PrinterCommand::Print {
                    raster_lines,
                    chain_print,
                    precut,
                    copies,
                    quality: state.print_quality,
                    target: state.printer_target.clone(),
                });
                state.operation_in_progress = true;
                state.status_message = if copies > 1 {
                    format!("Printing {copies} copies...")
                } else {
                    "Printing...".to_string()
                };
            }
        }

        // Print All (Batch) button
        let total_prints = state.total_batch_prints();
        if total_prints > 1 || state.batch_items.len() > 1 {
            let can_print_batch = connected && !busy && total_prints > 0;
            let batch_btn = egui::Button::new(
                egui::RichText::new(format!("🖨 Print All ({total_prints})"))
                    .strong()
                    .color(if can_print_batch {
                        egui::Color32::from_rgb(22, 101, 52)
                    } else {
                        egui::Color32::GRAY
                    }),
            );
            let batch_resp = if can_print_batch {
                ui.add(batch_btn)
            } else {
                let reason = if !connected {
                    "Printer is not connected"
                } else if busy {
                    "Printing in progress..."
                } else {
                    "All label copies are set to 0"
                };
                ui.add_enabled(false, batch_btn)
                    .on_disabled_hover_text(reason)
            };
            if batch_resp.clicked() {
                do_batch_print(state);
            }
        }

        // Stop / Cancel button when printing
        if state.operation_in_progress
            && ui
                .button(
                    egui::RichText::new("⏹ Cancel")
                        .color(egui::Color32::from_rgb(220, 38, 38))
                        .strong(),
                )
                .on_hover_text("Abort the active print operation immediately")
                .clicked()
        {
            state.request_cancel();
        }

        let can_feed_cut = connected && !busy && !state.printer_target.is_bluetooth();
        let feed_btn = egui::Button::new("Feed & Cut");
        let feed_resp = if can_feed_cut {
            ui.add(feed_btn)
        } else {
            let reason = if !connected {
                "Printer is not connected"
            } else if busy {
                "Printer is busy..."
            } else {
                "Feed & cut not supported on Bluetooth"
            };
            ui.add_enabled(false, feed_btn)
                .on_disabled_hover_text(reason)
        };
        if feed_resp.clicked()
            && let Some(ref tx) = state.printer_cmd_tx
        {
            let _ = tx.send(PrinterCommand::FeedAndCut(state.printer_target.clone()));
            state.operation_in_progress = true;
            state.status_message = "Feeding & cutting...".to_string();
        }

        let tape_btn_label = if let Some(total_m) = state.cartridge_total_length_m {
            let rem = (total_m - state.tape_printed_meters).max(0.0);
            format!("🏷 Tape: {} mm ({:.1}m rem)", state.tape_width_mm, rem)
        } else {
            format!("🏷 Tape: {} mm", state.tape_width_mm)
        };
        if ui.button(tape_btn_label).clicked() {
            state.show_setup_modal = true;
        }

        if ui.button("Export Image").clicked() {
            do_export_image(state);
        }

        ui.separator();

        if ui.button("Save Layout").clicked() {
            do_save_layout(state);
        }

        if ui.button("Open Layout").clicked() {
            do_open_layout(state);
        }

        if ui.button("Save Batch").clicked() {
            crate::panels::batch_panel::do_save_batch(state);
        }

        if ui.button("Open Batch").clicked() {
            crate::panels::batch_panel::do_open_batch(state);
        }
    });
}

/// Print all labels in the batch project with their individual NUMBER TO PRINT counts.
pub fn do_batch_print(state: &mut AppState) {
    state.sync_active_to_batch();

    let total_prints = state.total_batch_prints();
    if total_prints == 0 {
        state.status_message = "No labels to print (all copies set to 0)".to_string();
        return;
    }

    let dpi = if state.printer_dpi > 0 {
        state.printer_dpi as f32
    } else {
        180.0
    };
    let px_per_mm = dpi / 25.4;
    let blank_line = vec![0u8; (state.printer_max_px as usize).div_ceil(8)];

    let mut all_labels_raster: Vec<(Vec<Vec<u8>>, bool)> = Vec::new();
    let mut total_mm = 0.0f32;

    let mut renderer = ptouch_render::text::TextRenderer::new();

    for item in &state.batch_items {
        if item.copies == 0 {
            continue;
        }

        let result = ptouch_render::document::render_elements(
            &item.document.elements,
            state.tape_width_px,
            &item.document.font_name,
            item.document.font_margin,
            &mut renderer,
        );

        let bitmap = match result {
            Ok(Some(bmp)) => bmp.mirrored(item.document.flip_h, item.document.flip_v),
            _ => continue,
        };

        let raw_lines = raster::bitmap_to_raster_lines(&bitmap, state.printer_max_px);

        let is_pretrim = item.document.is_pretrim();
        let margin_mm = item.document.effective_margin_mm();

        let (lines, precut) = if state.cut_mode == crate::state::CutMarginMode::ChainPrint {
            let margin_lines = (2.0 * px_per_mm).round() as usize;
            let mut l = Vec::with_capacity(margin_lines * 2 + raw_lines.len());
            l.extend(std::iter::repeat_n(blank_line.clone(), margin_lines));
            l.extend(raw_lines);
            l.extend(std::iter::repeat_n(blank_line.clone(), margin_lines));
            (l, false)
        } else if is_pretrim {
            // Auto pre-trim: feed and snip 24.5 mm scrap, print with custom compact margin
            let margin_lines = (margin_mm * px_per_mm).round() as usize;
            let mut l = Vec::with_capacity(margin_lines * 2 + raw_lines.len());
            l.extend(std::iter::repeat_n(blank_line.clone(), margin_lines));
            l.extend(raw_lines);
            l.extend(std::iter::repeat_n(blank_line.clone(), margin_lines));
            (l, true)
        } else {
            // Centered: uses printer's natural 24.5 mm hardware lead, 0 scrap snippet
            let lead_mm: f32 = 24.5;
            let leading_extra_mm = (margin_mm - lead_mm).max(0.0);
            let leading_lines = (leading_extra_mm * px_per_mm).round() as usize;
            let trailing_lines = (margin_mm * px_per_mm).round() as usize;

            let mut l = Vec::with_capacity(leading_lines + raw_lines.len() + trailing_lines);
            l.extend(std::iter::repeat_n(blank_line.clone(), leading_lines));
            l.extend(raw_lines);
            l.extend(std::iter::repeat_n(blank_line.clone(), trailing_lines));
            (l, false)
        };

        let label_mm = if state.cut_mode == crate::state::CutMarginMode::ChainPrint {
            lines.len() as f32 / px_per_mm
        } else {
            24.5 + (lines.len() as f32 / px_per_mm)
        };
        for _ in 0..item.copies {
            all_labels_raster.push((lines.clone(), precut));
            total_mm += label_mm;
        }
    }

    if all_labels_raster.is_empty() {
        state.status_message = "No renderable labels to print".to_string();
        return;
    }

    state.tape_printed_meters += total_mm / 1000.0;

    let cut_each = state.auto_cut && state.cut_mode != crate::state::CutMarginMode::ChainPrint;

    if let Some(ref tx) = state.printer_cmd_tx {
        let _ = tx.send(PrinterCommand::PrintBatch {
            labels: all_labels_raster,
            cut_each,
            quality: state.print_quality,
            target: state.printer_target.clone(),
        });
        state.operation_in_progress = true;
        state.status_message = format!("Printing batch of {total_prints} labels...");
    }
}

/// Save the current design to a `.ptl` layout file (TOML with embedded images).
fn do_save_layout(state: &mut AppState) {
    if state.elements.is_empty() {
        state.status_message = "Nothing to save".to_string();
        return;
    }

    let document = LabelDocument {
        version: ptouch_render::document::DOCUMENT_VERSION,
        tape_width_mm: state.tape_width_mm,
        dpi: state.printer_dpi,
        font_name: state.font_name.clone(),
        font_margin: state.font_margin,
        flip_h: state.overall_flip_h,
        flip_v: state.overall_flip_v,
        margin_mm: state.margin_mm,
        elements: state.elements.clone(),
    };

    let text = match document.to_toml_string() {
        Ok(text) => text,
        Err(e) => {
            state.status_message = format!("Save error: {}", e);
            error!("Layout serialize error: {}", e);
            return;
        }
    };

    if let Some(path) = crate::widgets::save_layout_file() {
        let save_path: PathBuf = if path.extension().is_none() {
            path.with_extension("ptl")
        } else {
            path
        };
        match std::fs::write(&save_path, text) {
            Ok(()) => {
                state.status_message = format!("Saved to {}", save_path.display());
                info!("Saved layout: {}", save_path.display());
            }
            Err(e) => {
                state.status_message = format!("Save error: {}", e);
                error!("Layout write error: {}", e);
            }
        }
    }
}

/// Open a `.ptl` layout file, replacing the current design.
fn do_open_layout(state: &mut AppState) {
    let Some(path) = crate::widgets::pick_layout_file() else {
        return;
    };

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            state.status_message = format!("Open error: {}", e);
            error!("Layout read error: {}", e);
            return;
        }
    };

    match LabelDocument::from_toml_str(&text) {
        Ok(document) => {
            apply_layout(state, document);
            state.status_message = format!("Opened {}", path.display());
            info!("Opened layout: {}", path.display());
        }
        Err(e) => {
            state.status_message = format!("Open error: {}", e);
            error!("Layout parse error: {}", e);
        }
    }
}

pub(crate) fn apply_layout(state: &mut AppState, document: LabelDocument) {
    let doc_clone = document.clone();
    if !state.printer_target.is_bluetooth() {
        state.tape_width_mm = document.tape_width_mm;
        state.update_tape_pixels();
    }
    state.font_name = document.font_name;
    state.font_margin = document.font_margin;
    state.overall_flip_h = document.flip_h;
    state.overall_flip_v = document.flip_v;
    state.margin_mm = document.margin_mm;
    state.elements = document.elements;
    state.selected_element = if state.elements.is_empty() {
        None
    } else {
        Some(0)
    };
    if state.batch_items.is_empty() {
        let title = state
            .elements
            .first()
            .map(|e| e.display_name())
            .unwrap_or_else(|| "Label 1".into());
        state.batch_items.push(crate::state::BatchItem {
            id: state.next_batch_item_id,
            title,
            copies: 1,
            document: doc_clone,
            preview_bitmap: None,
            preview_texture: None,
            dirty: true,
        });
        state.next_batch_item_id += 1;
        state.active_batch_index = 0;
    } else if state.active_batch_index < state.batch_items.len() {
        state.batch_items[state.active_batch_index].document = doc_clone;
        state.batch_items[state.active_batch_index].title = state
            .elements
            .first()
            .map(|e| e.display_name())
            .unwrap_or_else(|| "Label".into());
        state.batch_items[state.active_batch_index].dirty = true;
    }
    state.mark_dirty();
}

/// Export the current label preview as an image file.
fn do_export_image(state: &mut AppState) {
    let bitmap = match state.preview_bitmap {
        Some(ref bmp) => bmp,
        None => {
            state.status_message = "Nothing to export".to_string();
            return;
        }
    };

    if let Some(path) = crate::widgets::save_image_file() {
        let save_path: PathBuf = if path.extension().is_none() {
            path.with_extension("png")
        } else {
            path
        };
        match bitmap.save(&save_path) {
            Ok(()) => {
                state.status_message = format!("Saved to {}", save_path.display());
                info!("Exported image: {}", save_path.display());
            }
            Err(e) => {
                state.status_message = format!("Save error: {}", e);
                error!("Image save error: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PrinterTarget;
    use ptouch_render::text::TextRenderer;

    fn layout(tape_width_mm: u8) -> LabelDocument {
        let document = LabelDocument {
            version: ptouch_render::document::DOCUMENT_VERSION,
            tape_width_mm,
            dpi: 180,
            font_name: "Inter".into(),
            font_margin: 0,
            flip_h: false,
            flip_v: false,
            margin_mm: None,
            elements: vec![LabelElement::CutMark],
        };
        LabelDocument::from_toml_str(&document.to_toml_string().unwrap()).unwrap()
    }

    #[test]
    fn opening_layout_keeps_bluetooth_tape_geometry() {
        for width in [12, 24] {
            let mut state = AppState {
                printer_target: PrinterTarget::Bluetooth {
                    name: "PT-P300BT".into(),
                    address: "AA:BB:CC:DD:EE:FF".into(),
                },
                printer_connected: true,
                printer_max_px: 128,
                tape_width_mm: 12,
                tape_width_px: 64,
                ..AppState::default()
            };
            apply_layout(&mut state, layout(width));
            assert_eq!((state.tape_width_mm, state.tape_width_px), (12, 64));
            let bitmap = ptouch_render::document::render_elements(
                &state.elements,
                state.tape_width_px,
                &state.font_name,
                state.font_margin,
                &mut TextRenderer::new(),
            )
            .unwrap()
            .unwrap();
            let lines = raster::bitmap_to_raster_lines(&bitmap, state.printer_max_px);
            assert!(
                lines
                    .iter()
                    .all(|line| line[..4].iter().chain(&line[12..]).all(|&byte| byte == 0))
            );
        }
    }

    #[test]
    fn opening_usb_layout_uses_saved_tape_width() {
        let mut state = AppState {
            tape_width_mm: 24,
            tape_width_px: 128,
            ..AppState::default()
        };
        apply_layout(&mut state, layout(12));
        assert_eq!((state.tape_width_mm, state.tape_width_px), (12, 76));
    }
}
