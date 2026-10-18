// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Huang Rui <vowstar@gmail.com>

//! Left sidebar panel: printer info, tape selection, and element list.

use log::info;

use ptouch_core::protocol::PrintQuality;
use ptouch_core::tape;

use crate::state::{AppState, CutMarginMode, PrinterCommand};

/// Render the left sidebar.
pub fn show_sidebar(ui: &mut egui::Ui, state: &mut AppState) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        show_printer_section(ui, state);
        ui.separator();
        show_tape_section(ui, state);
        ui.separator();
        show_print_options(ui, state);
        ui.separator();
        show_mirror_section(ui, state);
        ui.separator();
        show_elements_section(ui, state);
    });
}

/// Printer connection section.
fn show_printer_section(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Printer");
    ui.add_space(4.0);

    let old_target = state.printer_target.clone();
    ui.add_enabled_ui(!state.is_printer_busy(), |ui| {
        egui::ComboBox::from_label("Connection")
            .selected_text(state.printer_target.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut state.printer_target,
                    crate::state::PrinterTarget::Usb,
                    "USB (automatic)",
                );
                for target in &state.bluetooth_targets {
                    ui.selectable_value(&mut state.printer_target, target.clone(), target.label());
                }
            });
    });
    if state.printer_target != old_target {
        state.printer_connected = false;
        state.printer_model = None;
        state.printer_status = Some("Connecting...".to_string());
        state.connecting = true;
        state.auto_cut = !state.printer_target.is_bluetooth();
        if let Some(ref tx) = state.printer_cmd_tx {
            let _ = tx.send(PrinterCommand::Poll(state.printer_target.clone()));
        }
    }

    let model_text = state.printer_model.as_deref().unwrap_or("Not connected");
    ui.label(format!("Model: {}", model_text));

    let status_text = state.printer_status.as_deref().unwrap_or("Scanning...");
    ui.label(format!("Status: {}", status_text));

    ui.add_space(4.0);
    if ui
        .add_enabled(!state.is_printer_busy(), egui::Button::new("Refresh"))
        .clicked()
        && let Some(ref tx) = state.printer_cmd_tx
    {
        state.connecting = true;
        let _ = tx.send(PrinterCommand::Poll(state.printer_target.clone()));
        let _ = tx.send(PrinterCommand::DiscoverBluetooth);
        info!("Manual printer refresh requested");
    }
}

/// Tape width selection section.
fn show_tape_section(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Tape");
    ui.add_space(4.0);

    let tapes = tape::supported_tapes(state.printer_dpi);
    let current_label = format!("{} mm ({} px)", state.tape_width_mm, state.tape_width_px);

    ui.add_enabled_ui(!state.printer_target.is_bluetooth(), |ui| {
        egui::ComboBox::from_label("Width")
            .selected_text(&current_label)
            .show_ui(ui, |ui| {
                for t in tapes {
                    let label = format!("{} mm ({} px)", t.width_mm, t.pixels);
                    if ui
                        .selectable_value(&mut state.tape_width_mm, t.width_mm, &label)
                        .clicked()
                    {
                        state.update_tape_pixels();
                        state.mark_dirty();
                        info!("Tape changed: {} mm", t.width_mm);
                    }
                }
            });

        ui.add_space(4.0);
        if ui.button("🏷 Change Tape / Cartridge...").clicked() {
            state.show_setup_modal = true;
        }

        if let Some(total_m) = state.cartridge_total_length_m {
            let remaining = (total_m - state.tape_printed_meters).max(0.0);
            ui.label(
                egui::RichText::new(format!(
                    "Cartridge: {:.2}m rem / {:.1}m",
                    remaining, total_m
                ))
                .small()
                .color(ui.visuals().weak_text_color()),
            );
        }
    });
}

/// Print options: label length confinement, cut & margin mode, copies, and quality selection.
fn show_print_options(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Length & Margin");
    ui.add_space(4.0);

    // Label length confinement (works on both USB and Bluetooth)
    let mut limit_len = state.max_length_mm.is_some();
    if ui
        .checkbox(&mut limit_len, "Confine label length (auto-fit)")
        .on_hover_text(
            "When enabled, confines total finished label length to the target size in mm.\n\
             If the text is too long, its font size is automatically adjusted so it fits.\n\
             Shorter labels keep their natural length.",
        )
        .changed()
    {
        if limit_len {
            state.set_max_length_mm(Some(100.0));
        } else {
            state.set_max_length_mm(None);
        }
    }

    let mut fit_largest = state.fit_largest_font;
    if ui
        .checkbox(
            &mut fit_largest,
            "🔠 Fit largest possible font in confined area",
        )
        .on_hover_text(
            "When enabled, automatically scales text up or down to the largest possible\n\
             font size (using tight pixel ink bounds and balanced multi-line wrapping)\n\
             that fits inside the confined label area.",
        )
        .changed()
    {
        state.set_fit_largest_font(fit_largest);
    }

    ui.horizontal(|ui| {
        ui.label("Length:");
        let is_auto = state.max_length_mm.is_none();
        if ui.selectable_label(is_auto, "Auto").clicked() {
            state.set_max_length_mm(None);
        }
        for &preset_mm in &[50.0_f32, 75.0, 100.0, 150.0] {
            let selected = state
                .max_length_mm
                .map(|m| (m - preset_mm).abs() < 0.1)
                .unwrap_or(false);
            if ui
                .selectable_label(selected, format!("{:.0}mm", preset_mm))
                .clicked()
            {
                state.set_max_length_mm(Some(preset_mm));
            }
        }
    });

    if let Some(cur_max) = state.max_length_mm {
        let mut edit_max = cur_max;
        ui.horizontal(|ui| {
            ui.label("Max Length:");
            if ui
                .add(
                    egui::DragValue::new(&mut edit_max)
                        .range(10.0..=1000.0)
                        .speed(1.0)
                        .suffix(" mm"),
                )
                .changed()
            {
                state.set_max_length_mm(Some(edit_max));
            }
        });
        let mode_desc = if state.fit_largest_font {
            "largest possible font fitted"
        } else {
            "text auto-shrinks to fit"
        };
        ui.label(
            egui::RichText::new(format!(
                "📐 Confined to ≤ {:.1} mm ({})",
                state.max_length_mm.unwrap_or(cur_max),
                mode_desc
            ))
            .small()
            .color(egui::Color32::from_rgb(96, 165, 250)),
        );
    }

    ui.add_space(6.0);
    ui.add_enabled_ui(!state.printer_target.is_bluetooth(), |ui| {
        let is_chain = state.cut_mode == CutMarginMode::ChainPrint;

        ui.horizontal(|ui| {
            if ui.selectable_label(!is_chain, "Standard / Auto Pre-trim").clicked() {
                state.cut_mode = if state.is_pretrim() {
                    CutMarginMode::PretrimCut
                } else {
                    CutMarginMode::CenteredFull
                };
                state.auto_cut = true;
                state.mark_dirty();
            }
            if ui.selectable_label(is_chain, "Chain Print (Continuous)").clicked() {
                state.cut_mode = CutMarginMode::ChainPrint;
                state.auto_cut = false;
                state.mark_dirty();
            }
        });

        if state.cut_mode != CutMarginMode::ChainPrint {
            ui.add_space(4.0);
            let is_pretrim = state.is_pretrim();
            let cur_margin = state.effective_margin_mm();

            ui.horizontal(|ui| {
                ui.label("Presets:");
                let is_cent = !is_pretrim && (state.margin_mm.is_none() || state.margin_mm == Some(27.0));
                if ui.selectable_label(is_cent, "Centered (~27mm)").clicked() {
                    state.margin_mm = if state.max_length_mm.is_some() {
                        Some(27.0)
                    } else {
                        None
                    };
                    state.cut_mode = CutMarginMode::CenteredFull;
                    state.sync_active_to_batch();
                    state.mark_dirty();
                }
                let is_3 = (cur_margin - 3.0).abs() < 0.1;
                if ui.selectable_label(is_3, "✂ 3mm").clicked() {
                    state.margin_mm = Some(3.0);
                    state.cut_mode = CutMarginMode::PretrimCut;
                    state.small_margin_mm = 3.0;
                    state.sync_active_to_batch();
                    state.mark_dirty();
                }
                let is_5 = (cur_margin - 5.0).abs() < 0.1;
                if ui.selectable_label(is_5, "✂ 5mm").clicked() {
                    state.margin_mm = Some(5.0);
                    state.cut_mode = CutMarginMode::PretrimCut;
                    state.small_margin_mm = 5.0;
                    state.sync_active_to_batch();
                    state.mark_dirty();
                }
            });

            ui.horizontal(|ui| {
                ui.label("Margin (mm):");
                let mut custom_val = cur_margin;
                if ui
                    .add(
                        egui::DragValue::new(&mut custom_val)
                            .range(1.5..=100.0)
                            .speed(0.5)
                            .suffix(" mm"),
                    )
                    .changed()
                {
                    if (custom_val - 27.0).abs() < 0.1 {
                        state.margin_mm = None;
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
            });

            ui.add_space(2.0);
            if is_pretrim {
                ui.label(
                    egui::RichText::new(format!(
                        "✂ Auto Pre-trim Active: Margin ({:.1}mm) < 24.5mm printhead lead.\nPrinter will snip 24.5mm scrap snippet.",
                        cur_margin
                    ))
                    .small()
                    .color(egui::Color32::from_rgb(194, 65, 12)),
                );
            } else {
                ui.label(
                    egui::RichText::new(format!(
                        "📏 Full Lead: Symmetrical {:.1}mm margin using printer's natural lead. 0mm scrap snippet.",
                        cur_margin
                    ))
                    .small()
                    .color(egui::Color32::from_rgb(22, 101, 52)),
                );
            }
        } else {
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("ℹ Continuous printing without cuts. Click 'Feed & Cut' in the toolbar when finished.")
                    .small()
                    .color(egui::Color32::from_rgb(0, 140, 70)),
            );
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Copies:");
            let resp = ui.add(egui::DragValue::new(&mut state.copies).range(1..=99));
            if resp.changed() && state.active_batch_index < state.batch_items.len() {
                state.batch_items[state.active_batch_index].copies = state.copies;
            }
        });
    });

    if state.printer_target.is_bluetooth() {
        ui.label("Manual cutter");
    }

    if state.printer_quality_modes {
        ui.add_space(4.0);
        let quality_label = |q: PrintQuality| match q {
            PrintQuality::Standard => "Standard",
            PrintQuality::HighRes => "High resolution",
            PrintQuality::Draft => "Draft (high speed)",
        };
        egui::ComboBox::from_label("Quality")
            .selected_text(quality_label(state.print_quality))
            .show_ui(ui, |ui| {
                for q in [
                    PrintQuality::Standard,
                    PrintQuality::HighRes,
                    PrintQuality::Draft,
                ] {
                    ui.selectable_value(&mut state.print_quality, q, quality_label(q));
                }
            });
    }
}

/// Whole-label mirror section: flip the entire composed label.
///
/// This applies once after all elements are composed and is separate from the
/// per-element flip on the properties panel.
fn show_mirror_section(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Mirror");
    ui.add_space(4.0);

    if ui
        .checkbox(&mut state.overall_flip_h, "Flip horizontal (left-right)")
        .changed()
    {
        state.mark_dirty();
        info!("Whole-label horizontal flip: {}", state.overall_flip_h);
    }
    if ui
        .checkbox(&mut state.overall_flip_v, "Flip vertical (top-bottom)")
        .changed()
    {
        state.mark_dirty();
        info!("Whole-label vertical flip: {}", state.overall_flip_v);
    }
}

/// Element list section with reorder and delete controls.
fn show_elements_section(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Elements");
    ui.add_space(4.0);

    if state.elements.is_empty() {
        ui.label("(no elements)");
        return;
    }

    let mut action: Option<ElementAction> = None;

    for (idx, element) in state.elements.iter().enumerate() {
        let is_selected = state.selected_element == Some(idx);
        let label = format!("{}. {}", idx + 1, element.display_name());

        ui.horizontal(|ui| {
            if ui.selectable_label(is_selected, &label).clicked() {
                state.selected_element = Some(idx);
            }
            if ui
                .small_button("🗑")
                .on_hover_text("Delete this element")
                .clicked()
            {
                action = Some(ElementAction::Delete(idx));
            }
        });
    }

    ui.add_space(4.0);

    ui.horizontal(|ui| {
        let has_selection = state.selected_element.is_some();

        if ui
            .add_enabled(
                has_selection && state.selected_element.unwrap_or(0) > 0,
                egui::Button::new("Up"),
            )
            .clicked()
            && let Some(idx) = state.selected_element
        {
            action = Some(ElementAction::MoveUp(idx));
        }

        if ui
            .add_enabled(
                has_selection
                    && state
                        .selected_element
                        .map(|i| i + 1 < state.elements.len())
                        .unwrap_or(false),
                egui::Button::new("Down"),
            )
            .clicked()
            && let Some(idx) = state.selected_element
        {
            action = Some(ElementAction::MoveDown(idx));
        }

        if ui
            .add_enabled(has_selection, egui::Button::new("Delete"))
            .clicked()
            && let Some(idx) = state.selected_element
        {
            action = Some(ElementAction::Delete(idx));
        }
    });

    // Apply deferred action
    if let Some(act) = action {
        match act {
            ElementAction::MoveUp(idx) => {
                if idx > 0 {
                    state.elements.swap(idx, idx - 1);
                    state.selected_element = Some(idx - 1);
                    state.mark_dirty();
                }
            }
            ElementAction::MoveDown(idx) => {
                if idx + 1 < state.elements.len() {
                    state.elements.swap(idx, idx + 1);
                    state.selected_element = Some(idx + 1);
                    state.mark_dirty();
                }
            }
            ElementAction::Delete(idx) => {
                state.elements.remove(idx);
                state.validate_selection();
                state.mark_dirty();
            }
        }
    }
}

/// Deferred actions for element list manipulation.
enum ElementAction {
    MoveUp(usize),
    MoveDown(usize),
    Delete(usize),
}
