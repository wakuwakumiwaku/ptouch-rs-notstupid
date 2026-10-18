// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Huang Rui <vowstar@gmail.com>

//! Central canvas panel for label preview display, full tape geometry, and cut indicators.

use crate::state::{AppState, CutMarginMode};

/// Render the central preview canvas.
pub fn show_canvas(ui: &mut egui::Ui, state: &mut AppState) {
    let dpi = if state.printer_dpi > 0 {
        state.printer_dpi as f32
    } else {
        180.0
    };
    let px_per_mm = dpi / 25.4;
    let copies = state.copies.max(1);

    state.ensure_batch_initialized();

    // Arrow keys navigation between batch labels when not actively editing text
    if !ui.ctx().egui_wants_keyboard_input() {
        if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) && state.active_batch_index > 0 {
            state.switch_active_batch(state.active_batch_index - 1);
        } else if ui.input(|i| i.key_pressed(egui::Key::ArrowRight))
            && state.active_batch_index + 1 < state.batch_items.len()
        {
            state.switch_active_batch(state.active_batch_index + 1);
        }
    }

    // Multi-label batch navigation bar
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("📋 Project Batch:")
                .strong()
                .color(egui::Color32::from_rgb(30, 64, 175)),
        );

        let total_items = state.batch_items.len();
        let cur_idx = state.active_batch_index;

        if ui
            .add_enabled(cur_idx > 0, egui::Button::new("◀ Prev"))
            .on_hover_text("Previous label (Left Arrow ←)")
            .clicked()
        {
            state.switch_active_batch(cur_idx - 1);
        }

        ui.label(egui::RichText::new(format!("Label {} of {}", cur_idx + 1, total_items)).strong());

        if ui
            .add_enabled(cur_idx + 1 < total_items, egui::Button::new("Next ▶"))
            .on_hover_text("Next label (Right Arrow →)")
            .clicked()
        {
            state.switch_active_batch(cur_idx + 1);
        }

        ui.separator();

        // NUMBER TO PRINT for this label
        if cur_idx < state.batch_items.len() {
            ui.label(
                egui::RichText::new("NUMBER TO PRINT:")
                    .strong()
                    .color(egui::Color32::from_rgb(20, 30, 45)),
            );
            if ui.button("➖").clicked() {
                state.batch_items[cur_idx].copies =
                    state.batch_items[cur_idx].copies.saturating_sub(1);
            }
            ui.add(
                egui::DragValue::new(&mut state.batch_items[cur_idx].copies)
                    .range(0..=999)
                    .speed(0.2),
            );
            if ui.button("➕").clicked() {
                state.batch_items[cur_idx].copies =
                    state.batch_items[cur_idx].copies.saturating_add(1);
            }
            ui.label("copies");
            state.copies = state.batch_items[cur_idx].copies.max(1);
        }

        ui.separator();

        if ui.button("➕ Add Label").clicked() {
            state.add_blank_batch_item();
        }

        if ui.button("📋 Duplicate").clicked() {
            state.duplicate_batch_item(cur_idx);
        }

        if total_items > 1 && ui.button("🗑 Delete").clicked() {
            state.remove_batch_item(cur_idx);
        }

        ui.separator();

        if ui
            .button(
                egui::RichText::new(format!("📋 View All ({total_items}) Labels & Print All"))
                    .strong()
                    .color(egui::Color32::from_rgb(22, 101, 52)),
            )
            .clicked()
        {
            state.view_mode = crate::state::ViewMode::Batch;
        }
    });

    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);

    // Quick text editor and zoom controls
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("✏ Text:")
                .strong()
                .color(egui::Color32::from_rgb(30, 41, 59)),
        );

        let selected_qr_idx = match state.selected_element {
            Some(idx)
                if idx < state.elements.len()
                    && matches!(
                        state.elements[idx],
                        crate::state::LabelElement::QrCode { .. }
                    ) =>
            {
                Some(idx)
            }
            _ => None,
        };

        let target_text_idx = match state.selected_element {
            Some(idx)
                if idx < state.elements.len()
                    && matches!(state.elements[idx], crate::state::LabelElement::Text { .. }) =>
            {
                Some(idx)
            }
            _ => state
                .elements
                .iter()
                .position(|el| matches!(el, crate::state::LabelElement::Text { .. })),
        };

        if let Some(idx) = selected_qr_idx {
            ui.label(
                egui::RichText::new("📱 QR:")
                    .strong()
                    .color(egui::Color32::from_rgb(30, 41, 59)),
            );
            if let crate::state::LabelElement::QrCode {
                ref mut content,
                ref mut bitmap,
                target_height,
                ..
            } = state.elements[idx]
            {
                let resp = ui.add(
                    egui::TextEdit::singleline(content)
                        .desired_width(240.0)
                        .hint_text("Type URL or QR content..."),
                );
                if state.request_text_focus {
                    resp.request_focus();
                    state.request_text_focus = false;
                }
                if resp.changed() {
                    let h = target_height.unwrap_or(state.tape_width_px);
                    *bitmap = ptouch_render::qr::render_qr_code(content, h).ok();
                    state.selected_element = Some(idx);
                    state.mark_dirty();
                }
            }
            if state.elements.len() > 1 {
                ui.small(
                    egui::RichText::new(format!(
                        "(Element {} of {})",
                        idx + 1,
                        state.elements.len()
                    ))
                    .color(egui::Color32::from_rgb(100, 110, 120)),
                );
            }
        } else if let Some(idx) = target_text_idx {
            if let crate::state::LabelElement::Text {
                ref mut content, ..
            } = state.elements[idx]
            {
                let resp = ui.add(
                    egui::TextEdit::singleline(content)
                        .desired_width(240.0)
                        .hint_text("Type label text here..."),
                );
                if state.request_text_focus {
                    resp.request_focus();
                    state.request_text_focus = false;
                }
                if resp.changed() {
                    state.selected_element = Some(idx);
                    state.mark_dirty();
                }
            }
            if state.elements.len() > 1 {
                ui.small(
                    egui::RichText::new(format!(
                        "(Element {} of {})",
                        idx + 1,
                        state.elements.len()
                    ))
                    .color(egui::Color32::from_rgb(100, 110, 120)),
                );
            }
        } else {
            ui.label(
                egui::RichText::new("(No text or QR on this label)")
                    .italics()
                    .color(egui::Color32::GRAY),
            );
            if ui.button("➕ Add Text").clicked() {
                state.elements.push(crate::state::LabelElement::Text {
                    content: "Label".to_string(),
                    font_size: None,
                    align: ptouch_render::text::TextAlign::Center,
                    rotation: 0.0,
                    flip_h: false,
                    flip_v: false,
                });
                state.selected_element = Some(state.elements.len() - 1);
                state.mark_dirty();
            }
            if ui.button("📱 Add QR Code").clicked() {
                state
                    .elements
                    .push(crate::state::LabelElement::qr_from_content(
                        "https://example.com",
                    ));
                state.selected_element = Some(state.elements.len() - 1);
                state.mark_dirty();
            }
        }

        ui.separator();

        if ui.button("Fit").clicked() {
            state.zoom_fit = true;
        }
        if ui.button("1:1").clicked() {
            state.zoom = 1.0;
            state.zoom_fit = false;
        }
        if ui.button("+").clicked() {
            state.zoom = (state.zoom * 1.25).min(20.0);
            state.zoom_fit = false;
        }
        if ui.button("-").clicked() {
            state.zoom = (state.zoom / 1.25).max(0.1);
            state.zoom_fit = false;
        }
        ui.label(format!("Zoom: {:.0}%", state.zoom * 100.0));
    });

    ui.add_space(2.0);

    // Quick Margin & Label Length Confinement row
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("📏 Margin:")
                .strong()
                .color(egui::Color32::from_rgb(147, 197, 253)),
        );

        let is_pretrim = state.is_pretrim();
        let cur_margin = state.effective_margin_mm();

        let is_centered =
            !is_pretrim && (state.margin_mm.is_none() || state.margin_mm == Some(27.0));
        if ui
            .selectable_label(is_centered, "Centered (~27mm)")
            .clicked()
        {
            state.margin_mm = if state.max_length_mm.is_some() {
                Some(27.0)
            } else {
                None
            };
            state.cut_mode = CutMarginMode::CenteredFull;
            state.sync_active_to_batch();
            state.mark_dirty();
        }

        let is_3mm = (cur_margin - 3.0).abs() < 0.1;
        if ui.selectable_label(is_3mm, "✂ 3mm").clicked() {
            state.margin_mm = Some(3.0);
            state.cut_mode = CutMarginMode::PretrimCut;
            state.small_margin_mm = 3.0;
            state.sync_active_to_batch();
            state.mark_dirty();
        }

        let is_5mm = (cur_margin - 5.0).abs() < 0.1;
        if ui.selectable_label(is_5mm, "✂ 5mm").clicked() {
            state.margin_mm = Some(5.0);
            state.cut_mode = CutMarginMode::PretrimCut;
            state.small_margin_mm = 5.0;
            state.sync_active_to_batch();
            state.mark_dirty();
        }

        let mut custom_val = cur_margin;
        let drag_resp = ui.add(
            egui::DragValue::new(&mut custom_val)
                .range(1.5..=100.0)
                .speed(0.5)
                .suffix(" mm"),
        );
        if drag_resp.changed() {
            if (custom_val - 27.0).abs() < 0.1 {
                state.margin_mm = if state.max_length_mm.is_some() {
                    Some(27.0)
                } else {
                    None
                };
                state.cut_mode = CutMarginMode::CenteredFull;
            } else {
                state.margin_mm = Some(custom_val);
                if custom_val < 24.5 {
                    state.cut_mode = CutMarginMode::PretrimCut;
                    state.small_margin_mm = custom_val;
                } else {
                    state.cut_mode = CutMarginMode::CenteredFull;
                }
            }
            state.sync_active_to_batch();
            state.mark_dirty();
        }

        ui.separator();

        // Quick Label Length Confinement selector
        ui.label(
            egui::RichText::new("📐 Max Length:")
                .strong()
                .color(egui::Color32::from_rgb(147, 197, 253)),
        );

        let is_auto_len = state.max_length_mm.is_none();
        if ui
            .selectable_label(is_auto_len, "Auto")
            .on_hover_text("Unconstrained label length (grows with text)")
            .clicked()
        {
            state.set_max_length_mm(None);
        }

        for &preset_mm in &[50.0_f32, 75.0, 100.0, 150.0] {
            let is_preset = state
                .max_length_mm
                .map(|m| (m - preset_mm).abs() < 0.1)
                .unwrap_or(false);
            if ui
                .selectable_label(is_preset, format!("{:.0}mm", preset_mm))
                .on_hover_text(format!(
                    "Confine total label length to ≤ {:.0} mm",
                    preset_mm
                ))
                .clicked()
            {
                state.set_max_length_mm(Some(preset_mm));
            }
        }

        if let Some(cur_max) = state.max_length_mm {
            let mut max_val = cur_max;
            let len_resp = ui
                .add(
                    egui::DragValue::new(&mut max_val)
                        .range(10.0..=1000.0)
                        .speed(1.0)
                        .prefix("≤ ")
                        .suffix(" mm"),
                )
                .on_hover_text(
                    "Maximum total finished label length in mm.\n\
                     Text font size automatically adjusts so the label fits.",
                );
            if len_resp.changed() {
                state.set_max_length_mm(Some(max_val));
            }
        }

        let mut fit_largest = state.fit_largest_font;
        if ui
            .checkbox(&mut fit_largest, "🔠 Fit largest font in confined area")
            .on_hover_text(
                "Automatically scales text to the largest possible font size (using tight\n\
                 pixel ink bounds and balanced multi-line wrapping) inside the confined area.",
            )
            .changed()
        {
            state.set_fit_largest_font(fit_largest);
        }
    });

    ui.add_space(4.0);

    // Tape overview header
    ui.horizontal_wrapped(|ui| {
        if let Some(ref texture) = state.preview_texture {
            let content_mm = (texture.size_vec2().x / px_per_mm) * copies as f32;
            let lead_mm: f32 = 24.5;
            let is_pretrim = state.is_pretrim();
            let margin_mm = state.effective_margin_mm();

            if state.cut_mode == CutMarginMode::ChainPrint {
                ui.label(
                    egui::RichText::new(format!("Chain Length: {:.1} mm", content_mm))
                        .strong(),
                );
                ui.separator();
                ui.label(
                    egui::RichText::new("✂ Cuts: 0 (Continuous Chain • No Cut)")
                        .color(egui::Color32::from_rgb(0, 160, 90)),
                );
            } else if is_pretrim {
                let finished_label_mm = content_mm + (margin_mm * 2.0 * copies as f32);
                ui.label(
                    egui::RichText::new(format!(
                        "Finished Label: {:.1} mm (compact: {:.1}mm margins)  [+ {:.1} mm Pretrim Scrap]",
                        finished_label_mm, margin_mm, lead_mm
                    ))
                    .strong(),
                );
                ui.separator();
                ui.label(
                    egui::RichText::new("✂ Cuts: 2 (Pretrim Cut + Final Cut)")
                        .color(egui::Color32::from_rgb(230, 50, 50)),
                );
            } else {
                let total_tape_mm = margin_mm + content_mm + margin_mm;
                ui.label(
                    egui::RichText::new(format!(
                        "Label Length: {:.1} mm (Centered: {:.1}mm left + {:.1}mm text + {:.1}mm right)",
                        total_tape_mm, margin_mm, content_mm, margin_mm
                    ))
                    .strong(),
                );
                ui.separator();
                ui.label(
                    egui::RichText::new("✂ Cuts: 1 (Final Cut • Same Blank Space Both Sides • 0 Scrap)")
                        .color(egui::Color32::from_rgb(40, 100, 200)),
                );
            }

            if let Some(max_len) = state.max_length_mm {
                ui.separator();
                ui.label(
                    egui::RichText::new(format!(
                        "📐 Confined: ≤ {:.1} mm (Auto-fit font)",
                        max_len
                    ))
                    .strong()
                    .color(egui::Color32::from_rgb(96, 165, 250)),
                );
            }
        }
    });

    ui.separator();

    // Canvas area background: crisp white as requested
    let canvas_rect = ui.available_rect_before_wrap().intersect(ui.clip_rect());
    let canvas_size = canvas_rect.size();

    // Fill canvas background with white
    ui.painter()
        .rect_filled(canvas_rect, 0.0, egui::Color32::WHITE);

    // Intuitive banner explaining the background vs the outlined label box
    let info_text = "(this is the background, the label is in the outlined box)";
    let banner_font = egui::FontId::proportional(11.5);
    let banner_pos = egui::pos2(canvas_rect.center().x, canvas_rect.min.y + 10.0);
    let text_rect = ui.painter().text(
        banner_pos,
        egui::Align2::CENTER_TOP,
        info_text,
        banner_font.clone(),
        egui::Color32::TRANSPARENT,
    );
    let pill_rect = text_rect.expand2(egui::vec2(10.0, 3.0));
    ui.painter()
        .rect_filled(pill_rect, 4.0, egui::Color32::from_rgb(243, 246, 250));
    ui.painter().rect_stroke(
        pill_rect,
        4.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(210, 220, 235)),
        egui::StrokeKind::Outside,
    );
    ui.painter().text(
        banner_pos,
        egui::Align2::CENTER_TOP,
        info_text,
        banner_font,
        egui::Color32::from_rgb(85, 100, 120),
    );

    match state.preview_texture {
        Some(ref texture) => {
            let tex_size = texture.size_vec2();
            let content_w_px = tex_size.x;
            let content_h_px = tex_size.y;
            let tape_physical_h_px = (state.tape_width_mm as f32 * px_per_mm).max(content_h_px);

            let lead_mm: f32 = 24.5;
            let is_pretrim = state.is_pretrim();
            let margin_mm = state.effective_margin_mm();
            let margin_px = (margin_mm * px_per_mm).round();
            let lead_w_px = (lead_mm * px_per_mm).round();

            // Total strip width in printer pixels (unscaled)
            let total_strip_w_px = if state.cut_mode == CutMarginMode::ChainPrint {
                let chain_margin_px = (2.0 * px_per_mm).round();
                (chain_margin_px + content_w_px + chain_margin_px) * copies as f32
            } else if is_pretrim {
                lead_w_px + ((margin_px + content_w_px + margin_px) * copies as f32)
            } else {
                margin_px + (content_w_px * copies as f32) + margin_px
            };

            // Calculate zoom
            let zoom = if state.zoom_fit {
                let margin_x = 80.0;
                let margin_y = 110.0;
                let zoom_x = (canvas_size.x - margin_x) / total_strip_w_px.max(1.0);
                let zoom_y = (canvas_size.y - margin_y) / (tape_physical_h_px + 50.0).max(1.0);
                let fit_zoom = zoom_x.min(zoom_y).clamp(0.05, 10.0);
                state.zoom = fit_zoom;
                fit_zoom
            } else {
                state.zoom
            };

            let display_strip_w = total_strip_w_px * zoom;
            let display_tape_h = tape_physical_h_px * zoom;

            // Center the entire printed tape in the canvas
            let center = canvas_rect.center();
            let tape_rect =
                egui::Rect::from_center_size(center, egui::vec2(display_strip_w, display_tape_h));

            let painter = ui.painter();

            // 1. Draw tape shadow and physical tape body in outlined box
            let shadow_rect = tape_rect.translate(egui::vec2(2.0, 3.0));
            painter.rect_filled(
                shadow_rect,
                4.0,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 35),
            );
            painter.rect_filled(tape_rect, 3.0, egui::Color32::WHITE);
            // Crisp, prominent outline for the printed label box
            painter.rect_stroke(
                tape_rect,
                3.0,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(40, 50, 70)),
                egui::StrokeKind::Outside,
            );

            // 2. Printable vertical guidelines
            let printable_h = content_h_px * zoom;
            let printable_top = tape_rect.center().y - printable_h / 2.0;
            let printable_bottom = printable_top + printable_h;
            if tape_physical_h_px > content_h_px {
                let guideline_stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(230));
                painter.line_segment(
                    [
                        egui::pos2(tape_rect.min.x, printable_top),
                        egui::pos2(tape_rect.max.x, printable_top),
                    ],
                    guideline_stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(tape_rect.min.x, printable_bottom),
                        egui::pos2(tape_rect.max.x, printable_bottom),
                    ],
                    guideline_stroke,
                );
            }

            let y_top = tape_rect.min.y;
            let y_bottom = tape_rect.max.y;
            let ruler_y = y_top - 28.0;

            let mut cur_x = tape_rect.min.x;
            let single_content_w = content_w_px * zoom;
            let single_content_mm = content_w_px / px_per_mm;

            if state.cut_mode == CutMarginMode::ChainPrint {
                let margin_mm = 2.0;
                let margin_w = (margin_mm * px_per_mm).round() * zoom;

                for copy_idx in 0..copies {
                    cur_x += margin_w;

                    let copy_start_x = cur_x;
                    let copy_rect = egui::Rect::from_min_size(
                        egui::pos2(copy_start_x, printable_top),
                        egui::vec2(single_content_w, printable_h),
                    );
                    painter.image(
                        texture.id(),
                        copy_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                    cur_x += single_content_w;
                    cur_x += margin_w;

                    if copies > 1 && copy_idx + 1 < copies {
                        draw_no_cut_marker(painter, cur_x, y_top, y_bottom, "⛓ Chained (0 mm)");
                    }
                }

                draw_dimension(
                    painter,
                    tape_rect.min.x,
                    cur_x,
                    ruler_y,
                    &format!(
                        "{:.1} mm (Chain)",
                        single_content_mm * copies as f32 + (margin_mm * 2.0 * copies as f32)
                    ),
                    egui::Color32::from_rgb(0, 160, 90),
                );

                draw_continuous_indicator(painter, cur_x, y_top, y_bottom);
            } else if is_pretrim {
                let lead_w = lead_w_px * zoom;
                let margin_w = (margin_mm * px_per_mm).round() * zoom;

                // 1. Scrap Snippet area (automatically pre-trimmed)
                let scrap_rect = egui::Rect::from_min_max(
                    egui::pos2(cur_x, y_top),
                    egui::pos2(cur_x + lead_w, y_bottom),
                );
                painter.rect_filled(scrap_rect, 0.0, egui::Color32::from_rgb(255, 235, 235));
                painter.text(
                    scrap_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "✂ Pre-trim Scrap (24.5 mm)",
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgb(190, 40, 40),
                );
                draw_dimension(
                    painter,
                    cur_x,
                    cur_x + lead_w,
                    ruler_y,
                    &format!("{:.1} mm (Scrap Snippet)", lead_mm),
                    egui::Color32::from_rgb(210, 40, 40),
                );
                cur_x += lead_w;

                // Pretrim cut marker
                draw_cut_marker(
                    painter,
                    cur_x,
                    y_top,
                    y_bottom,
                    "✂ PRE-TRIM CUT (Scrap cut)",
                    true,
                );

                let finished_label_start = cur_x;

                // 2. Finished label copies
                for copy_idx in 0..copies {
                    // Leading safe margin
                    cur_x += margin_w;

                    let copy_start_x = cur_x;
                    let copy_rect = egui::Rect::from_min_size(
                        egui::pos2(copy_start_x, printable_top),
                        egui::vec2(single_content_w, printable_h),
                    );
                    painter.image(
                        texture.id(),
                        copy_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                    cur_x += single_content_w;

                    // Trailing safe margin
                    cur_x += margin_w;

                    if copies > 1 && copy_idx + 1 < copies {
                        draw_no_cut_marker(painter, cur_x, y_top, y_bottom, "⛓ Chained (0 mm)");
                    }
                }

                // Dimension of the finished label
                let finished_label_mm =
                    (single_content_mm * copies as f32) + (margin_mm * 2.0 * copies as f32);
                draw_dimension(
                    painter,
                    finished_label_start,
                    cur_x,
                    ruler_y,
                    &format!("{:.1} mm (Finished Label)", finished_label_mm),
                    egui::Color32::from_rgb(40, 120, 220),
                );

                // Final cut marker
                draw_cut_marker(painter, cur_x, y_top, y_bottom, "✂ FINAL CUT", true);
            } else {
                let margin_w = margin_px * zoom;

                // Leading blank margin (24.5 mm hardware lead + safety)
                let lead_rect = egui::Rect::from_min_max(
                    egui::pos2(cur_x, y_top),
                    egui::pos2(cur_x + margin_w, y_bottom),
                );
                painter.rect_filled(lead_rect, 0.0, egui::Color32::from_gray(248));
                painter.text(
                    lead_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{:.1} mm Blank Margin", margin_mm),
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_gray(130),
                );
                draw_dimension(
                    painter,
                    cur_x,
                    cur_x + margin_w,
                    ruler_y,
                    &format!("{:.1} mm (Margin)", margin_mm),
                    egui::Color32::from_rgb(60, 140, 220),
                );
                cur_x += margin_w;

                // Content copies (centered)
                for copy_idx in 0..copies {
                    let copy_start_x = cur_x;
                    let copy_rect = egui::Rect::from_min_size(
                        egui::pos2(copy_start_x, printable_top),
                        egui::vec2(single_content_w, printable_h),
                    );
                    painter.image(
                        texture.id(),
                        copy_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                    draw_dimension(
                        painter,
                        copy_start_x,
                        copy_start_x + single_content_w,
                        ruler_y,
                        &format!("{:.1} mm (Content)", single_content_mm),
                        egui::Color32::from_rgb(40, 100, 200),
                    );
                    cur_x += single_content_w;

                    if copies > 1 && copy_idx + 1 < copies {
                        draw_no_cut_marker(painter, cur_x, y_top, y_bottom, "⛓ Chained (0 mm)");
                    }
                }

                // Trailing blank margin (identical matching left side)
                let trail_rect = egui::Rect::from_min_max(
                    egui::pos2(cur_x, y_top),
                    egui::pos2(cur_x + margin_w, y_bottom),
                );
                painter.rect_filled(trail_rect, 0.0, egui::Color32::from_gray(248));
                painter.text(
                    trail_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{:.1} mm Blank Margin", margin_mm),
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_gray(130),
                );
                draw_dimension(
                    painter,
                    cur_x,
                    cur_x + margin_w,
                    ruler_y,
                    &format!("{:.1} mm (Margin)", margin_mm),
                    egui::Color32::from_rgb(60, 140, 220),
                );
                cur_x += margin_w;

                // Single Final Cut at the end
                draw_cut_marker(painter, cur_x, y_top, y_bottom, "✂ FINAL CUT", true);
            }

            // Draw Tape Width Callout on the left
            painter.text(
                egui::pos2(tape_rect.min.x - 10.0, tape_rect.center().y),
                egui::Align2::RIGHT_CENTER,
                format!("{} mm", state.tape_width_mm),
                egui::FontId::proportional(11.0),
                egui::Color32::from_rgb(60, 75, 95),
            );

            // Explanatory bottom legend
            let legend_y = y_bottom + 30.0;
            let legend_text = if state.cut_mode == CutMarginMode::ChainPrint {
                "💡 Chain Print: Continuous printing with 0mm gap between labels. Exactly 0 cuts. Click 'Feed & Cut' to finish."
            } else if is_pretrim {
                "💡 Auto Pre-trim Active (< 24.5 mm): Margin is smaller than printhead distance (24.5 mm). Printer will pre-trim scrap snippet, then print with safe margin. 2 cuts."
            } else {
                "💡 Centered Full (≥ 24.5 mm): Protected with symmetrical margins using natural hardware lead. Exactly 1 cut at end (0 scrap)."
            };
            painter.text(
                egui::pos2(center.x, legend_y),
                egui::Align2::CENTER_TOP,
                legend_text,
                egui::FontId::proportional(11.0),
                egui::Color32::from_rgb(70, 80, 100),
            );

            let tape_resp = ui.interact(
                tape_rect,
                ui.id().with("tape_canvas_card"),
                egui::Sense::click(),
            );
            if tape_resp.double_clicked() {
                if let Some(pos) = state
                    .elements
                    .iter()
                    .position(|el| matches!(el, crate::state::LabelElement::Text { .. }))
                {
                    state.selected_element = Some(pos);
                }
                state.request_text_focus = true;
                ui.ctx().request_repaint();
            }
            tape_resp
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Double-click label to edit text");

            let canvas_resp = ui.allocate_rect(canvas_rect, egui::Sense::click());
            if canvas_resp.double_clicked() {
                if let Some(pos) = state
                    .elements
                    .iter()
                    .position(|el| matches!(el, crate::state::LabelElement::Text { .. }))
                {
                    state.selected_element = Some(pos);
                }
                state.request_text_focus = true;
                ui.ctx().request_repaint();
            }
        }
        None => {
            let center = canvas_rect.center();
            ui.painter().text(
                center,
                egui::Align2::CENTER_CENTER,
                "Add elements to design your label\n(this is the background, the label will appear in the outlined box)",
                egui::FontId::proportional(15.0),
                egui::Color32::from_rgb(100, 115, 135),
            );
            ui.allocate_rect(canvas_rect, egui::Sense::hover());
        }
    }
}

fn draw_dashed_v_line(
    painter: &egui::Painter,
    x: f32,
    y_min: f32,
    y_max: f32,
    stroke: egui::Stroke,
    dash: f32,
    gap: f32,
) {
    let mut y = y_min;
    while y < y_max {
        let y_end = (y + dash).min(y_max);
        painter.line_segment([egui::pos2(x, y), egui::pos2(x, y_end)], stroke);
        y += dash + gap;
    }
}

fn draw_cut_marker(
    painter: &egui::Painter,
    x: f32,
    y_top: f32,
    y_bottom: f32,
    label: &str,
    is_active_cut: bool,
) {
    let color = if is_active_cut {
        egui::Color32::from_rgb(230, 50, 50) // Vivid Red for CUT
    } else {
        egui::Color32::from_gray(140)
    };
    let stroke = egui::Stroke::new(2.0, color);

    draw_dashed_v_line(painter, x, y_top - 16.0, y_bottom + 16.0, stroke, 5.0, 3.0);

    let font_id = egui::FontId::proportional(11.0);
    let badge_pos = egui::pos2(x, y_top - 18.0);
    let text_rect = painter.text(
        badge_pos,
        egui::Align2::CENTER_BOTTOM,
        label,
        font_id.clone(),
        egui::Color32::TRANSPARENT,
    );
    let bg_rect = text_rect.expand2(egui::vec2(6.0, 3.0));
    painter.rect_filled(bg_rect, 3.0, color);
    painter.text(
        badge_pos,
        egui::Align2::CENTER_BOTTOM,
        label,
        font_id,
        egui::Color32::WHITE,
    );
}

fn draw_no_cut_marker(painter: &egui::Painter, x: f32, y_top: f32, y_bottom: f32, label: &str) {
    let color = egui::Color32::from_rgb(50, 130, 220); // Blue
    let stroke = egui::Stroke::new(1.0, color);

    draw_dashed_v_line(painter, x, y_top - 10.0, y_bottom + 10.0, stroke, 3.0, 3.0);

    let font_id = egui::FontId::proportional(10.0);
    let badge_pos = egui::pos2(x, y_top - 14.0);
    let text_rect = painter.text(
        badge_pos,
        egui::Align2::CENTER_BOTTOM,
        label,
        font_id.clone(),
        egui::Color32::TRANSPARENT,
    );
    let bg_rect = text_rect.expand2(egui::vec2(4.0, 2.0));
    painter.rect_filled(bg_rect, 3.0, color);
    painter.text(
        badge_pos,
        egui::Align2::CENTER_BOTTOM,
        label,
        font_id,
        egui::Color32::WHITE,
    );
}

fn draw_continuous_indicator(painter: &egui::Painter, x: f32, y_top: f32, y_bottom: f32) {
    let color = egui::Color32::from_rgb(0, 160, 90); // Green
    let font_id = egui::FontId::proportional(11.0);
    let label = "➔ CONTINUOUS (NO CUT)";

    let stroke = egui::Stroke::new(2.0, color);
    draw_dashed_v_line(painter, x, y_top - 16.0, y_bottom + 16.0, stroke, 4.0, 3.0);

    let badge_pos = egui::pos2(x + 8.0, y_top - 18.0);
    let text_rect = painter.text(
        badge_pos,
        egui::Align2::LEFT_BOTTOM,
        label,
        font_id.clone(),
        egui::Color32::TRANSPARENT,
    );
    let bg_rect = text_rect.expand2(egui::vec2(6.0, 3.0));
    painter.rect_filled(bg_rect, 3.0, color);
    painter.text(
        badge_pos,
        egui::Align2::LEFT_BOTTOM,
        label,
        font_id,
        egui::Color32::WHITE,
    );
}

fn draw_dimension(
    painter: &egui::Painter,
    x_start: f32,
    x_end: f32,
    y: f32,
    text: &str,
    color: egui::Color32,
) {
    let w = (x_end - x_start).abs();
    if w < 12.0 {
        return;
    }
    let stroke = egui::Stroke::new(1.0, color);
    painter.line_segment([egui::pos2(x_start, y), egui::pos2(x_end, y)], stroke);
    painter.line_segment(
        [egui::pos2(x_start, y - 3.0), egui::pos2(x_start, y + 3.0)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(x_end, y - 3.0), egui::pos2(x_end, y + 3.0)],
        stroke,
    );
    painter.text(
        egui::pos2((x_start + x_end) / 2.0, y - 4.0),
        egui::Align2::CENTER_BOTTOM,
        text,
        egui::FontId::proportional(11.0),
        color,
    );
}
