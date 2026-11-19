// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Huang Rui <vowstar@gmail.com>

//! Startup tape and cartridge setup modal dialog.

use crate::state::{AppState, format_tape_width_mm};
use crate::widgets;

/// Render the tape and cartridge setup dialog as a modal window.
pub fn show_tape_modal(ctx: &egui::Context, state: &mut AppState) {
    let style = ctx.style_of(ctx.theme());
    let (fill, stroke) = if style.visuals.dark_mode {
        (
            egui::Color32::from_rgb(30, 34, 42),
            egui::Stroke::new(1.0, egui::Color32::from_rgb(72, 82, 98)),
        )
    } else {
        (
            egui::Color32::WHITE,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 208, 220)),
        )
    };
    let frame = egui::Frame::window(&style).fill(fill).stroke(stroke);

    egui::Modal::new(egui::Id::new("tape_setup_modal"))
        .frame(frame)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.heading("Select Tape & Cartridge");
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("Choose your loaded tape size and optional cartridge length:")
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(8.0);

            // Printer detection notice
            if state.printer_connected {
                let model = state.printer_model.as_deref().unwrap_or("Connected");
                ui.label(
                    egui::RichText::new(format!("Connected: {model}"))
                        .color(widgets::accent_green(ui))
                        .strong(),
                );
            } else {
                ui.label(
                    egui::RichText::new(
                        "Printer not yet detected. You can select your tape width manually:",
                    )
                    .color(widgets::accent_amber(ui)),
                );
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            // Step 1: Size of Tape (protocol width 4 = 3.5 mm TZe tape)
            ui.label(egui::RichText::new("1. Size of your tape:").strong());
            ui.horizontal_wrapped(|ui| {
                for &width in &[4, 6, 9, 12, 18, 24, 36] {
                    let label = format_tape_width_mm(width);
                    let is_selected = state.tape_width_mm == width;
                    let text = if is_selected {
                        egui::RichText::new(label).strong()
                    } else {
                        egui::RichText::new(label)
                    };
                    if ui.selectable_label(is_selected, text).clicked() {
                        state.tape_width_mm = width;
                        state.update_tape_pixels();
                        state.mark_dirty();
                    }
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);

            // Step 2: Optional total length in meters
            ui.label(egui::RichText::new("2. Total length (optional, in meters):").strong());
            ui.label(
                egui::RichText::new("Tracks how much tape remains on your cartridge:")
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
            ui.horizontal(|ui| {
                ui.label("Length:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.cartridge_length_input)
                        .desired_width(70.0)
                        .hint_text("8.0"),
                );
                ui.label("meters");

                ui.add_space(8.0);
                if ui.button("8 m (Standard TZe)").clicked() {
                    state.cartridge_length_input = "8.0".to_string();
                }
                if ui.button("4 m (Sample)").clicked() {
                    state.cartridge_length_input = "4.0".to_string();
                }
                if !state.cartridge_length_input.is_empty() && ui.button("Clear").clicked() {
                    state.cartridge_length_input.clear();
                }
            });

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(6.0);

            // Confirm button
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(egui::RichText::new("  Start Designing Label  ").strong())
                        .clicked()
                    {
                        let trimmed = state.cartridge_length_input.trim();
                        if let Ok(val) = trimmed.parse::<f32>() {
                            if val > 0.0 {
                                state.cartridge_total_length_m = Some(val);
                            } else {
                                state.cartridge_total_length_m = None;
                            }
                        } else {
                            state.cartridge_total_length_m = None;
                        }

                        state.show_setup_modal = false;
                        state.mark_dirty();
                    }
                });
            });
        });
}
