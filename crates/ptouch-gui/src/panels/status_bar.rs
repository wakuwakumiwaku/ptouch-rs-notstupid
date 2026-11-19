// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Huang Rui <vowstar@gmail.com>

//! Bottom status bar panel.

use crate::state::{AppState, format_tape_width_mm};
use crate::widgets;

/// Render the bottom status bar.
pub fn show_status_bar(ui: &mut egui::Ui, state: &AppState) {
    ui.horizontal(|ui| {
        // Printer status
        let printer = state.printer_model.as_deref().unwrap_or("--");
        ui.label(format!("Printer: {}", printer));

        ui.separator();

        // Tape info & cartridge remaining
        let tape_label = format_tape_width_mm(state.tape_width_mm);
        if let Some(total_m) = state.cartridge_total_length_m {
            let remaining = (total_m - state.tape_printed_meters).max(0.0);
            ui.label(format!(
                "Tape: {} | Cartridge: {:.2}m rem / {:.1}m",
                tape_label, remaining, total_m
            ));
        } else {
            ui.label(format!("Tape: {}", tape_label));
        }

        ui.separator();

        // Label dimensions
        if let Some(ref bmp) = state.preview_bitmap {
            ui.label(format!("{}x{} px", bmp.width(), bmp.height()));
        } else {
            ui.label("-- x -- px");
        }

        ui.separator();

        // Zoom level
        ui.label(format!("Zoom: {:.0}%", state.zoom * 100.0));

        ui.separator();

        // Status message (right-aligned)
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if state.operation_in_progress {
                ui.spinner();
                ui.label(
                    egui::RichText::new(&state.status_message)
                        .color(widgets::accent_blue(ui))
                        .strong(),
                );
            } else if state.status_message.to_lowercase().contains("error") {
                ui.label(
                    egui::RichText::new(&state.status_message)
                        .color(widgets::accent_red(ui))
                        .strong(),
                );
            } else {
                ui.label(&state.status_message);
            }
        });
    });
}
