// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Huang Rui <vowstar@gmail.com>

//! Quick batch generator modal dialog.
//!
//! Allows generating multiple labels at once via line-by-line text lists
//! or sequential number series.

use eframe::egui;
use ptouch_render::document::{DOCUMENT_VERSION, LabelDocument, LabelElement};
use ptouch_render::text::TextAlign;

use crate::state::{AppState, BatchItem, ViewMode};

/// Generation mode tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratorTab {
    TextLines,
    NumberSeries,
}

#[derive(Clone)]
pub struct BatchGeneratorState {
    pub tab: GeneratorTab,
    pub text_lines_input: String,
    pub prefix: String,
    pub suffix: String,
    pub start_num: i32,
    pub end_num: i32,
    pub digits: usize,
    pub copies_per_label: u32,
    pub replace_existing: bool,
}

impl Default for BatchGeneratorState {
    fn default() -> Self {
        Self {
            tab: GeneratorTab::TextLines,
            text_lines_input: "Label 1\nLabel 2\nLabel 3".to_string(),
            prefix: "Item-".to_string(),
            suffix: String::new(),
            start_num: 1,
            end_num: 10,
            digits: 2,
            copies_per_label: 1,
            replace_existing: false,
        }
    }
}

// Persistent generator state stored in egui memory
pub fn show_generator_modal(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_generator_modal {
        return;
    }

    let mut gen_state: BatchGeneratorState = ctx
        .data_mut(|d| d.get_temp(egui::Id::new("batch_generator_state")))
        .unwrap_or_default();

    let mut open = state.show_generator_modal;

    let mut should_close = false;

    egui::Window::new(
        egui::RichText::new("⚡ Quick Generate Multiple Labels")
            .strong()
            .size(16.0),
    )
    .open(&mut open)
    .collapsible(false)
    .resizable(false)
    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
    .fixed_size(egui::vec2(480.0, 360.0))
    .show(ctx, |ui| {
        ui.add_space(4.0);

        // Tab bar
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut gen_state.tab,
                GeneratorTab::TextLines,
                "📝 Text Lines (One per label)",
            );
            ui.selectable_value(
                &mut gen_state.tab,
                GeneratorTab::NumberSeries,
                "🔢 Number Sequence (1..N)",
            );
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        match gen_state.tab {
            GeneratorTab::TextLines => {
                ui.label(
                    egui::RichText::new(
                        "Paste or type your text lines below (each line becomes one label):",
                    )
                    .small(),
                );
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut gen_state.text_lines_input)
                                .desired_width(f32::INFINITY)
                                .desired_rows(6)
                                .font(egui::TextStyle::Monospace),
                        );
                    });

                let non_empty_count = gen_state
                    .text_lines_input
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .count();
                ui.small(format!("Will generate {non_empty_count} label(s)"));
            }
            GeneratorTab::NumberSeries => {
                ui.label(egui::RichText::new("Configure sequential numbering pattern:").small());
                ui.add_space(6.0);

                egui::Grid::new("series_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Prefix:");
                        ui.text_edit_singleline(&mut gen_state.prefix);
                        ui.end_row();

                        ui.label("Start Number:");
                        ui.add(egui::DragValue::new(&mut gen_state.start_num).range(0..=99999));
                        ui.end_row();

                        ui.label("End Number:");
                        ui.add(egui::DragValue::new(&mut gen_state.end_num).range(0..=99999));
                        ui.end_row();

                        ui.label("Min Digits (Zero Padding):");
                        ui.add(egui::DragValue::new(&mut gen_state.digits).range(1..=6));
                        ui.end_row();

                        ui.label("Suffix:");
                        ui.text_edit_singleline(&mut gen_state.suffix);
                        ui.end_row();
                    });

                let count = (gen_state.end_num - gen_state.start_num + 1).max(0);
                let sample_start = format!(
                    "{}{:0width$}{}",
                    gen_state.prefix,
                    gen_state.start_num,
                    gen_state.suffix,
                    width = gen_state.digits
                );
                let sample_end = format!(
                    "{}{:0width$}{}",
                    gen_state.prefix,
                    gen_state.end_num,
                    gen_state.suffix,
                    width = gen_state.digits
                );

                ui.add_space(4.0);
                ui.small(format!(
                    "Preview: {sample_start}  ...  {sample_end} ({count} labels)"
                ));
            }
        }

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        // General options
        ui.horizontal(|ui| {
            ui.label("Initial NUMBER TO PRINT per label:");
            ui.add(egui::DragValue::new(&mut gen_state.copies_per_label).range(1..=99));
            ui.label("copies");
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.radio_value(
                &mut gen_state.replace_existing,
                false,
                "Append to current batch",
            );
            ui.radio_value(
                &mut gen_state.replace_existing,
                true,
                "Replace current batch",
            );
        });

        ui.add_space(12.0);

        // Action buttons
        ui.horizontal(|ui| {
            if ui
                .button(
                    egui::RichText::new("⚡ Generate Labels")
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .clicked()
            {
                execute_generation(state, &gen_state);
                should_close = true;
            }

            if ui.button("Cancel").clicked() {
                should_close = true;
            }
        });
    });

    if should_close {
        open = false;
    }

    ctx.data_mut(|d| d.insert_temp(egui::Id::new("batch_generator_state"), gen_state));
    state.show_generator_modal = open;
}

fn execute_generation(state: &mut AppState, generator: &BatchGeneratorState) {
    state.sync_active_to_batch();

    let texts: Vec<String> = match generator.tab {
        GeneratorTab::TextLines => generator
            .text_lines_input
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        GeneratorTab::NumberSeries => {
            let mut list = Vec::new();
            let start = generator.start_num.min(generator.end_num);
            let end = generator.start_num.max(generator.end_num);
            for n in start..=end {
                list.push(format!(
                    "{}{:0width$}{}",
                    generator.prefix,
                    n,
                    generator.suffix,
                    width = generator.digits
                ));
            }
            list
        }
    };

    if texts.is_empty() {
        return;
    }

    if generator.replace_existing {
        state.batch_items.clear();
    }

    let initial_copies = generator.copies_per_label.max(1);

    for text in texts {
        let doc = LabelDocument {
            version: DOCUMENT_VERSION,
            tape_width_mm: state.tape_width_mm,
            dpi: state.printer_dpi,
            font_name: state.font_name.clone(),
            font_margin: state.font_margin,
            flip_h: false,
            flip_v: false,
            margin_mm: state.margin_mm,
            max_length_mm: state.max_length_mm,
            fit_largest_font: state.fit_largest_font,
            elements: vec![LabelElement::Text {
                content: text.clone(),
                font_size: None,
                align: TextAlign::Center,
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
            }],
        };

        let id = state.next_batch_item_id;
        state.next_batch_item_id += 1;

        state.batch_items.push(BatchItem {
            id,
            title: text,
            copies: initial_copies,
            document: doc,
            preview_bitmap: None,
            preview_texture: None,
            dirty: true,
        });
    }

    state.active_batch_index = 0;
    if !state.batch_items.is_empty() {
        let doc = state.batch_items[0].document.clone();
        state.tape_width_mm = doc.tape_width_mm;
        state.font_name = doc.font_name;
        state.font_margin = doc.font_margin;
        state.margin_mm = doc.margin_mm;
        state.max_length_mm = doc.max_length_mm;
        state.elements = doc.elements;
        state.selected_element = if state.elements.is_empty() {
            None
        } else {
            Some(0)
        };
        state.update_tape_pixels();
        state.mark_dirty();
    }

    // Switch to batch view so user immediately sees all generated labels with their NUMBER TO PRINT
    state.view_mode = ViewMode::Batch;
    state.status_message = format!("Generated batch with {} label(s)", state.batch_items.len());
}
