// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Huang Rui <vowstar@gmail.com>

//! Batch & Multi-Label overview panel.
//!
//! Displays all labels in the project with live previews and an explicit
//! "NUMBER TO PRINT" copy counter right next to each label.

use eframe::egui;

use crate::state::{AppState, ViewMode};

/// Render the All Labels Batch Overview in the central panel.
pub fn show_batch_panel(
    ui: &mut egui::Ui,
    state: &mut AppState,
    renderer: &mut ptouch_render::text::TextRenderer,
) {
    state.ensure_batch_initialized();
    state.sync_active_to_batch();

    // Update any dirty previews for batch items
    update_batch_item_previews(ui.ctx(), state, renderer);

    let total_labels = state.batch_items.len();
    let total_prints = state.total_batch_prints();
    let px_per_mm = state.printer_dpi as f32 / 25.4;

    // Estimate total tape length for the entire batch
    let mut total_batch_mm = 0.0f32;
    let mut total_scrap_mm = 0.0f32;
    let mut pretrim_count = 0;
    let mut centered_count = 0;

    for item in &state.batch_items {
        if item.copies > 0 {
            let is_pre = item.document.is_pretrim();
            let margin = item.document.effective_margin_mm();
            let label_len_mm = if let Some(ref bmp) = item.preview_bitmap {
                let raw_mm = bmp.width() as f32 / px_per_mm;
                if state.cut_mode == crate::state::CutMarginMode::ChainPrint {
                    raw_mm + 4.0
                } else if is_pre {
                    raw_mm + margin * 2.0 + 24.5
                } else {
                    raw_mm + margin * 2.0
                }
            } else {
                30.0
            };
            if state.cut_mode != crate::state::CutMarginMode::ChainPrint {
                if is_pre {
                    pretrim_count += item.copies;
                    total_scrap_mm += 24.5 * item.copies as f32;
                } else {
                    centered_count += item.copies;
                }
            }
            total_batch_mm += label_len_mm * item.copies as f32;
        }
    }

    // Top control banner
    egui::Frame::canvas(ui.style())
        .fill(egui::Color32::from_rgb(248, 250, 252))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                // Return to Designer
                if ui
                    .button(egui::RichText::new("✏ Switch to Designer Canvas").strong())
                    .clicked()
                {
                    state.view_mode = ViewMode::Designer;
                }

                ui.separator();

                // Add blank label
                if ui
                    .button(
                        egui::RichText::new("➕ Add New Label")
                            .color(egui::Color32::from_rgb(20, 110, 40)),
                    )
                    .clicked()
                {
                    state.add_blank_batch_item();
                }

                // Quick Generate
                if ui
                    .button(
                        egui::RichText::new("⚡ Quick Generate (Series / List)...")
                            .color(egui::Color32::from_rgb(30, 90, 180)),
                    )
                    .clicked()
                {
                    state.show_generator_modal = true;
                }

                ui.separator();

                // Save / Open Batch
                if ui.button("💾 Save Batch").clicked() {
                    do_save_batch(state);
                }
                if ui.button("📂 Open Batch").clicked() {
                    do_open_batch(state);
                }

                ui.separator();

                ui.checkbox(&mut state.auto_cut, "✂ Cut between labels")
                    .on_hover_text(
                        "When checked, the printer automatically cuts between every label.\n\
                         When unchecked, prints as a continuous strip (cutting only after the last label).",
                    );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let connected = state.printer_connected;
                    let busy = state.is_printer_busy();
                    let can_print_all = connected && !busy && total_prints > 0;

                    let print_btn_text = if busy {
                        format!("⏳ Printing... ({total_prints} Prints)")
                    } else if total_prints > 0 {
                        format!("🖨 Print All Labels ({total_prints} Prints)")
                    } else {
                        "🖨 Print All Labels (0 Prints)".to_string()
                    };

                    let btn = egui::Button::new(
                        egui::RichText::new(print_btn_text)
                            .strong()
                            .size(13.0)
                            .color(egui::Color32::WHITE),
                    )
                    .fill(if can_print_all {
                        egui::Color32::from_rgb(22, 101, 52) // Dark green
                    } else {
                        egui::Color32::from_rgb(120, 120, 120)
                    });

                    if can_print_all {
                        if ui.add(btn).clicked() {
                            crate::panels::toolbar::do_batch_print(state);
                        }
                    } else {
                        let reason = if !connected {
                            "Printer is not connected (check USB cable / power)"
                        } else if busy {
                            "Printing in progress..."
                        } else {
                            "All label copies are set to 0. Set at least one label to 1 or more copies."
                        };
                        ui.add_enabled(false, btn).on_disabled_hover_text(reason);
                    }

                    // Cancel batch button when busy
                    if state.operation_in_progress
                        && ui
                            .button(
                                egui::RichText::new("⏹ Cancel Batch")
                                    .color(egui::Color32::from_rgb(220, 38, 38))
                                    .strong(),
                            )
                            .on_hover_text("Abort remaining labels in the batch immediately")
                            .clicked()
                    {
                        state.request_cancel();
                    }

                    // Summary badge
                    let cut_tag = if state.auto_cut && state.cut_mode != crate::state::CutMarginMode::ChainPrint {
                        "✂ Cut each"
                    } else {
                        "⛓ Continuous strip"
                    };

                    let trim_summary = if state.cut_mode == crate::state::CutMarginMode::ChainPrint {
                        String::new()
                    } else if pretrim_count > 0 && centered_count > 0 {
                        format!(" • Mixed: {centered_count} centered, {pretrim_count} pre-trimmed ({total_scrap_mm:.1}mm scrap)")
                    } else if pretrim_count > 0 {
                        format!(" • All {pretrim_count} pre-trimmed ({total_scrap_mm:.1}mm scrap)")
                    } else {
                        " • All centered (0mm scrap)".to_string()
                    };

                    ui.label(
                        egui::RichText::new(format!(
                            "{total_labels} label(s) • {total_prints} print(s) • ~{:.1} cm • {cut_tag}{trim_summary}",
                            total_batch_mm / 10.0
                        ))
                        .strong()
                        .color(egui::Color32::from_rgb(50, 70, 90)),
                    );
                });
            });
        });

    // Cartridge length warning banner
    if let Some(total_m) = state.cartridge_total_length_m {
        let remaining_m = (total_m - state.tape_printed_meters).max(0.0);
        let batch_m = total_batch_mm / 1000.0;
        if batch_m > remaining_m {
            ui.add_space(4.0);
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(254, 242, 242))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(239, 68, 68)))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "⚠️ Cartridge Warning: This batch needs ~{:.2}m tape, but only ~{:.2}m remains in the cartridge! Check tape before printing to avoid running out mid-batch.",
                                batch_m, remaining_m
                            ))
                            .color(egui::Color32::from_rgb(185, 28, 28))
                            .strong(),
                        );
                    });
                });
        }
    }

    ui.add_space(8.0);

    // Scrollable list showing ALL labels with live previews and "NUMBER TO PRINT" right next to each
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(4.0);

            let mut to_duplicate = None;
            let mut to_remove = None;
            let mut to_edit = None;
            let mut active_text_changed = false;
            let mut active_margin_changed = false;
            let active_idx = state.active_batch_index;
            let default_tape_px = state.tape_width_px;

            for (idx, item) in state.batch_items.iter_mut().enumerate() {
                let is_active = idx == active_idx;
                let bg_color = if is_active {
                    egui::Color32::from_rgb(240, 246, 255) // light blue highlight
                } else {
                    egui::Color32::from_rgb(255, 255, 255)
                };

                let border_stroke = if is_active {
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(59, 130, 246))
                } else {
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(210, 215, 225))
                };

                egui::Frame::new()
                    .fill(bg_color)
                    .stroke(border_stroke)
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(14, 12))
                    .show(ui, |ui| {
                        // Header row of the card
                        ui.horizontal(|ui| {
                            let badge_text = format!("#{}", idx + 1);
                            ui.label(
                                egui::RichText::new(badge_text)
                                    .strong()
                                    .size(14.0)
                                    .color(egui::Color32::from_rgb(30, 64, 175)),
                            );

                            if let Some(ptouch_render::document::LabelElement::Text {
                                content,
                                ..
                            }) = item.document.elements.iter_mut().find(|el| matches!(el, ptouch_render::document::LabelElement::Text { .. }))
                            {
                                let resp = ui.add(
                                    egui::TextEdit::singleline(content)
                                        .desired_width(240.0)
                                        .font(egui::TextStyle::Heading)
                                        .hint_text("Label text..."),
                                );
                                if resp.changed() {
                                    item.title = content
                                        .lines()
                                        .next()
                                        .unwrap_or("Label")
                                        .trim()
                                        .to_string();
                                    item.dirty = true;
                                    if is_active {
                                        active_text_changed = true;
                                    }
                                }
                                let has_qr = item.document.elements.iter().any(|el| matches!(el, ptouch_render::document::LabelElement::QrCode { .. }));
                                if has_qr {
                                    ui.label(
                                        egui::RichText::new("📱+📝")
                                            .small()
                                            .color(egui::Color32::from_rgb(100, 116, 139)),
                                    ).on_hover_text("This label contains both text and QR code");
                                }
                            } else if let Some(ptouch_render::document::LabelElement::QrCode {
                                content,
                                bitmap,
                                target_height,
                                ..
                            }) = item.document.elements.iter_mut().find(|el| matches!(el, ptouch_render::document::LabelElement::QrCode { .. }))
                            {
                                ui.label(egui::RichText::new("📱 QR:").strong().size(13.0));
                                let resp = ui.add(
                                    egui::TextEdit::singleline(content)
                                        .desired_width(240.0)
                                        .font(egui::TextStyle::Monospace)
                                        .hint_text("https://... or text"),
                                );
                                if resp.changed() {
                                    let h = target_height.unwrap_or(item.document.tape_width_mm as u32 * 8);
                                    *bitmap = ptouch_render::qr::render_qr_code(content, h).ok();
                                    item.title = format!("QR: {}", content.trim());
                                    item.dirty = true;
                                    if is_active {
                                        active_text_changed = true;
                                    }
                                }
                            } else {
                                let resp = ui.add(
                                    egui::TextEdit::singleline(&mut item.title)
                                        .desired_width(240.0)
                                        .font(egui::TextStyle::Heading)
                                        .hint_text("Label title..."),
                                );
                                if resp.changed() {
                                    item.dirty = true;
                                }
                                if ui.small_button("➕ Add Text").clicked() {
                                    item.document.elements.push(
                                        ptouch_render::document::LabelElement::Text {
                                            content: item.title.clone(),
                                            font_size: None,
                                            align: ptouch_render::text::TextAlign::Center,
                                            rotation: 0.0,
                                            flip_h: false,
                                            flip_v: false,
                                        },
                                    );
                                    item.dirty = true;
                                    if is_active {
                                        active_text_changed = true;
                                    }
                                }
                                if ui.small_button("➕ Add QR").clicked() {
                                    let initial = if item.title.is_empty() {
                                        "https://example.com"
                                    } else {
                                        &item.title
                                    };
                                    item.document.elements.push(
                                        ptouch_render::document::LabelElement::qr_from_content(initial),
                                    );
                                    item.dirty = true;
                                    if is_active {
                                        active_text_changed = true;
                                    }
                                }
                            }

                            if is_active {
                                ui.label(
                                    egui::RichText::new("(Currently in Designer)")
                                        .small()
                                        .color(egui::Color32::from_rgb(59, 130, 246)),
                                );
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if total_labels > 1 && ui.button("🗑 Delete").clicked() {
                                    to_remove = Some(idx);
                                }

                                if ui.button("📋 Duplicate").clicked() {
                                    to_duplicate = Some(idx);
                                }

                                if ui
                                    .button(egui::RichText::new("✏ Edit in Designer").strong())
                                    .clicked()
                                {
                                    to_edit = Some(idx);
                                }
                            });
                        });

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        // Content row: Label preview on Left, "NUMBER TO PRINT" right NEXT to it on Right!
                        ui.horizontal(|ui| {
                            // Left: Label Visual Preview
                            let label_px_w = item
                                .preview_bitmap
                                .as_ref()
                                .map(|b| b.width())
                                .unwrap_or(120);
                            let _label_px_h = item
                                .preview_bitmap
                                .as_ref()
                                .map(|b| b.height())
                                .unwrap_or(default_tape_px);

                            let mm_w = label_px_w as f32 / px_per_mm;
                            let mm_h = item.document.tape_width_mm as f32;

                            ui.vertical(|ui| {
                                ui.set_min_width(320.0);
                                ui.label(
                                    egui::RichText::new("Label Preview (Exact Tape Print):")
                                        .small()
                                        .strong()
                                        .color(egui::Color32::from_rgb(70, 80, 95)),
                                );

                                ui.add_space(4.0);

                                // White tape preview container
                                let preview_frame_resp = egui::Frame::new()
                                    .fill(egui::Color32::WHITE)
                                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(180, 185, 195)))
                                    .corner_radius(4.0)
                                    .inner_margin(egui::Margin::symmetric(10, 8))
                                    .show(ui, |ui| {
                                        if let Some(ref tex) = item.preview_texture {
                                            // Scale texture to display cleanly
                                            let max_display_w = 340.0;
                                            let max_display_h = 60.0;
                                            let scale = (max_display_w / tex.size()[0] as f32)
                                                .min(max_display_h / tex.size()[1] as f32)
                                                .min(1.0);
                                            let size = egui::vec2(
                                                tex.size()[0] as f32 * scale,
                                                tex.size()[1] as f32 * scale,
                                            );
                                            ui.image((tex.id(), size));
                                        } else {
                                            ui.label(
                                                egui::RichText::new("[Rendering preview...]")
                                                    .italics()
                                                    .color(egui::Color32::GRAY),
                                            );
                                        }
                                    });

                                let preview_interact = ui.interact(
                                    preview_frame_resp.response.rect,
                                    ui.id().with(("batch_preview_card", idx)),
                                    egui::Sense::click(),
                                );
                                if preview_interact.double_clicked() {
                                    to_edit = Some(idx);
                                }
                                preview_interact
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .on_hover_text("Double-click to open and edit in Designer");

                                ui.add_space(2.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Dimensions: {:.1} mm × {:.1} mm  •  Tape: {} mm  •  Font: {}",
                                        mm_w, mm_h, item.document.tape_width_mm, item.document.font_name
                                    ))
                                    .size(10.5)
                                    .color(egui::Color32::from_rgb(110, 115, 125)),
                                );

                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    let is_pretrim = item.document.is_pretrim();
                                    let cur_margin = item.document.effective_margin_mm();

                                    if is_pretrim {
                                        ui.label(
                                            egui::RichText::new(format!("✂ Auto Pre-trim ({:.1} mm • 24.5 mm scrap)", cur_margin))
                                                .small()
                                                .strong()
                                                .color(egui::Color32::from_rgb(194, 65, 12)),
                                        ).on_hover_text("Margin < 24.5 mm. Printer will pre-trim the 24.5 mm scrap snippet.");
                                    } else {
                                        ui.label(
                                            egui::RichText::new(format!("📏 Centered Lead ({:.1} mm • 0 mm scrap)", cur_margin))
                                                .small()
                                                .strong()
                                                .color(egui::Color32::from_rgb(22, 101, 52)),
                                        ).on_hover_text("Margin ≥ 24.5 mm. Uses printer's natural hardware lead. Zero scrap snippet!");
                                    }

                                    ui.separator();

                                    let is_cent = item.document.margin_mm.is_none() || item.document.margin_mm == Some(27.0);
                                    if ui.selectable_label(is_cent, "Centered").clicked() {
                                        item.document.margin_mm = None;
                                        item.dirty = true;
                                        if is_active {
                                            active_margin_changed = true;
                                        }
                                    }

                                    let is_3 = item.document.margin_mm == Some(3.0);
                                    if ui.selectable_label(is_3, "✂ 3mm").clicked() {
                                        item.document.margin_mm = Some(3.0);
                                        item.dirty = true;
                                        if is_active {
                                            active_margin_changed = true;
                                        }
                                    }

                                    let is_5 = item.document.margin_mm == Some(5.0);
                                    if ui.selectable_label(is_5, "✂ 5mm").clicked() {
                                        item.document.margin_mm = Some(5.0);
                                        item.dirty = true;
                                        if is_active {
                                            active_margin_changed = true;
                                        }
                                    }
                                });
                            });

                            ui.add_space(20.0);
                            ui.separator();
                            ui.add_space(20.0);

                            // Right: NUMBER TO PRINT (Prominent counter right next to each label)
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new("NUMBER TO PRINT")
                                        .strong()
                                        .size(13.0)
                                        .color(egui::Color32::from_rgb(20, 30, 45)),
                                );

                                ui.add_space(6.0);

                                ui.horizontal(|ui| {
                                    // Decrement button
                                    if ui
                                        .button(egui::RichText::new("  ➖  ").size(13.0))
                                        .clicked()
                                    {
                                        item.copies = item.copies.saturating_sub(1);
                                    }

                                    // Numeric input
                                    ui.add(
                                        egui::DragValue::new(&mut item.copies)
                                            .range(0..=999)
                                            .speed(0.2),
                                    );

                                    // Increment button
                                    if ui
                                        .button(egui::RichText::new("  ➕  ").size(13.0))
                                        .clicked()
                                    {
                                        item.copies = item.copies.saturating_add(1);
                                    }

                                    ui.label(
                                        egui::RichText::new("copies")
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(60, 70, 85)),
                                    );
                                });

                                ui.add_space(6.0);

                                // Quick copy presets
                                ui.horizontal(|ui| {
                                    ui.small("Presets:");
                                    if ui.small_button("0").clicked() {
                                        item.copies = 0;
                                    }
                                    if ui.small_button("1").clicked() {
                                        item.copies = 1;
                                    }
                                    if ui.small_button("2").clicked() {
                                        item.copies = 2;
                                    }
                                    if ui.small_button("5").clicked() {
                                        item.copies = 5;
                                    }
                                    if ui.small_button("10").clicked() {
                                        item.copies = 10;
                                    }
                                });

                                ui.add_space(4.0);

                                // Print status feedback
                                if item.copies == 0 {
                                    ui.label(
                                        egui::RichText::new("⚠️ Skipped (0 copies)")
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(180, 100, 30)),
                                    );
                                } else {
                                    let subtotal_mm = mm_w * item.copies as f32;
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "✓ Will print {} copy(ies) (~{:.1} mm tape)",
                                            item.copies, subtotal_mm
                                        ))
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(22, 101, 52)),
                                    );
                                }
                            });
                        });
                    });

                ui.add_space(8.0);
            }

            // Apply deferred actions
            if active_text_changed && state.active_batch_index < state.batch_items.len() {
                state.elements =
                    state.batch_items[state.active_batch_index].document.elements.clone();
                state.validate_selection();
                state.mark_dirty();
            }
            if active_margin_changed && state.active_batch_index < state.batch_items.len() {
                state.margin_mm =
                    state.batch_items[state.active_batch_index].document.margin_mm;
                state.mark_dirty();
            }
            if let Some(idx) = to_duplicate {
                state.duplicate_batch_item(idx);
            }
            if let Some(idx) = to_remove {
                state.remove_batch_item(idx);
            }
            if let Some(idx) = to_edit {
                state.switch_active_batch(idx);
                state.view_mode = ViewMode::Designer;
                state.request_text_focus = true;
            }

            ui.add_space(16.0);
        });
}

/// Update preview textures for all batch items that are dirty or missing textures.
pub fn update_batch_item_previews(
    ctx: &egui::Context,
    state: &mut AppState,
    renderer: &mut ptouch_render::text::TextRenderer,
) {
    for item in &mut state.batch_items {
        if item.preview_texture.is_none() || item.dirty {
            if let Ok(Some(bitmap)) = ptouch_render::document::render_elements(
                &item.document.elements,
                state.tape_width_px,
                &item.document.font_name,
                item.document.font_margin,
                renderer,
            ) {
                let mirrored = bitmap.mirrored(item.document.flip_h, item.document.flip_v);
                let rgba = mirrored.to_rgba_image();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                let tex_name = format!("batch_preview_{}", item.id);
                let texture =
                    ctx.load_texture(tex_name, color_image, egui::TextureOptions::NEAREST);
                item.preview_texture = Some(texture);
                item.preview_bitmap = Some(mirrored);
            }
            item.dirty = false;
        }
    }
}

/// Save the current batch of labels to a `.ptb` / `.ptl` layout batch file.
pub fn do_save_batch(state: &mut AppState) {
    state.sync_active_to_batch();

    let mut entries = Vec::new();
    for item in &state.batch_items {
        entries.push(ptouch_render::document::BatchEntry {
            title: item.title.clone(),
            copies: item.copies,
            document: item.document.clone(),
        });
    }

    let batch = ptouch_render::document::LabelBatch {
        version: ptouch_render::document::DOCUMENT_VERSION,
        tape_width_mm: state.tape_width_mm,
        dpi: state.printer_dpi,
        labels: entries,
    };

    let text = match batch.to_toml_string() {
        Ok(t) => t,
        Err(e) => {
            state.status_message = format!("Serialize batch error: {e}");
            return;
        }
    };

    if let Some(path) = crate::widgets::save_layout_file() {
        let save_path = if path.extension().is_none() {
            path.with_extension("ptb")
        } else {
            path
        };
        match std::fs::write(&save_path, text) {
            Ok(()) => {
                state.status_message = format!("Saved batch to {}", save_path.display());
            }
            Err(e) => {
                state.status_message = format!("Save error: {e}");
            }
        }
    }
}

/// Open a batch layout file or single layout file into the batch queue.
pub fn do_open_batch(state: &mut AppState) {
    let Some(path) = crate::widgets::pick_layout_file() else {
        return;
    };

    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            state.status_message = format!("Open error: {e}");
            return;
        }
    };

    match ptouch_render::document::LabelBatch::from_toml_str(&text) {
        Ok(batch) => {
            apply_loaded_batch(state, batch);
            state.status_message =
                format!("Opened batch with {} label(s)", state.batch_items.len());
        }
        Err(e) => {
            state.status_message = format!("Parse batch error: {e}");
        }
    }
}

pub(crate) fn apply_loaded_batch(state: &mut AppState, batch: ptouch_render::document::LabelBatch) {
    state.batch_items.clear();
    for (idx, entry) in batch.labels.into_iter().enumerate() {
        let id = state.next_batch_item_id;
        state.next_batch_item_id += 1;
        let title = if entry.title.is_empty() {
            format!("Label {}", idx + 1)
        } else {
            entry.title
        };
        state.batch_items.push(crate::state::BatchItem {
            id,
            title,
            copies: entry.copies,
            document: entry.document,
            preview_bitmap: None,
            preview_texture: None,
            dirty: true,
        });
    }
    state.active_batch_index = 0;
    if !state.batch_items.is_empty() {
        let doc = state.batch_items[0].document.clone();
        if !state.printer_target.is_bluetooth() {
            state.tape_width_mm = doc.tape_width_mm;
            state.update_tape_pixels();
        }
        state.font_name = doc.font_name;
        state.font_margin = doc.font_margin;
        state.overall_flip_h = doc.flip_h;
        state.overall_flip_v = doc.flip_v;
        state.margin_mm = doc.margin_mm;
        state.elements = doc.elements;
        state.selected_element = if state.elements.is_empty() {
            None
        } else {
            Some(0)
        };
        state.mark_dirty();
    }
}
