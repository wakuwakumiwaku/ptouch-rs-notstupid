// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Huang Rui <vowstar@gmail.com>

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use ptouch_core::protocol::PrintQuality;
use ptouch_render::bitmap::LabelBitmap;

use ptouch_render::document::{DOCUMENT_VERSION, LabelDocument};

pub use ptouch_render::document::LabelElement;

/// High-level view mode for the GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// Visual single-label canvas editor.
    #[default]
    Designer,
    /// Batch overview showing all labels side-by-side / in a list with "NUMBER TO PRINT".
    Batch,
}

/// A label item within the multi-label batch project.
#[derive(Clone)]
pub struct BatchItem {
    pub id: u64,
    pub title: String,
    /// Number of copies to print for this label (NUMBER TO PRINT).
    pub copies: u32,
    pub document: LabelDocument,
    pub preview_bitmap: Option<LabelBitmap>,
    pub preview_texture: Option<egui::TextureHandle>,
    pub dirty: bool,
}

/// Printer connection selected in the GUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrinterTarget {
    Usb,
    #[cfg(any(target_os = "macos", test))]
    Bluetooth {
        name: String,
        address: String,
    },
}

impl PrinterTarget {
    pub fn label(&self) -> String {
        match self {
            Self::Usb => "USB (automatic)".to_string(),
            #[cfg(any(target_os = "macos", test))]
            Self::Bluetooth { name, address } => format!("{name} ({address})"),
        }
    }

    pub fn is_bluetooth(&self) -> bool {
        match self {
            Self::Usb => false,
            #[cfg(any(target_os = "macos", test))]
            Self::Bluetooth { .. } => true,
        }
    }
}

/// Cut & margin behavior for tape printers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CutMarginMode {
    /// Symmetrical label using hardware lead (~24.5mm) as front margin,
    /// centering the text with equal trailing margin. Single cut at end. Zero scrap snippets.
    /// Exact match for typing on the printer itself.
    #[default]
    CenteredFull,
    /// Pre-trims the ~24.5mm lead as scrap, prints compact label with safe margins (default 3mm).
    /// Two cuts: Pretrim cut + final cut. Ideal for warning signs and tight badges.
    PretrimCut,
    /// Continuous printing without automatic cut. Gaps between copies are minimal (0mm).
    ChainPrint,
}

/// Commands sent from the UI thread to the printer worker.
pub enum PrinterCommand {
    /// Find paired Bluetooth printers supported by this application.
    DiscoverBluetooth,
    /// Poll for a connected printer (query status only, no init).
    Poll(PrinterTarget),
    /// Print raster data for a single label.
    Print {
        raster_lines: Vec<Vec<u8>>,
        chain_print: bool,
        precut: bool,
        copies: u32,
        quality: PrintQuality,
        target: PrinterTarget,
    },
    /// Print multiple labels in a batch. Each label carries its raster lines and precut flag.
    PrintBatch {
        labels: Vec<(Vec<Vec<u8>>, bool)>,
        cut_each: bool,
        quality: PrintQuality,
        target: PrinterTarget,
    },
    /// Feed tape forward and cut.
    FeedAndCut(PrinterTarget),
}

/// A worker response and the printer it belongs to. Discovery is global.
pub struct PrinterEvent {
    pub target: Option<PrinterTarget>,
    pub response: PrinterResponse,
}

/// Responses sent from the printer worker back to the UI thread.
pub enum PrinterResponse {
    /// Paired PT-P300BT printers found by macOS.
    BluetoothDevices(Vec<PrinterTarget>),
    /// A printer was found and its status queried.
    Connected {
        model_name: String,
        media_width: u8,
        media_type: String,
        max_px: u16,
        dpi: u16,
        quality_modes: bool,
        tape_width_px: u16,
    },
    /// No printer found or previously connected printer lost.
    Disconnected,
    /// Print job completed successfully.
    PrintDone,
    /// Batch progress update: current label (1-indexed) out of total prints.
    BatchProgress { current: u32, total: u32 },
    /// Feed and cut completed successfully.
    FeedAndCutDone,
    /// An operation failed.
    Error(String),
}

/// Central application state shared across all panels.
pub struct AppState {
    /// List of label elements in composition order.
    pub elements: Vec<LabelElement>,
    /// Index of the currently selected element, if any.
    pub selected_element: Option<usize>,
    /// Current tape width in millimeters.
    pub tape_width_mm: u8,
    /// Current tape width in pixels (derived from tape_width_mm).
    pub tape_width_px: u32,
    /// Font name used for text rendering.
    pub font_name: String,
    /// Font top/bottom margin in pixels.
    pub font_margin: u32,
    /// Mirror the whole composed label left-right (horizontal).
    pub overall_flip_h: bool,
    /// Mirror the whole composed label top-bottom (vertical).
    pub overall_flip_v: bool,
    /// Cached list of available system font family names.
    pub available_fonts: Vec<String>,
    /// The rendered preview bitmap (1-bit).
    pub preview_bitmap: Option<LabelBitmap>,
    /// The preview texture uploaded to the GPU.
    pub preview_texture: Option<egui::TextureHandle>,
    /// Flag indicating the preview needs to be re-rendered.
    pub needs_rerender: bool,
    /// Current zoom level (1.0 = 100%).
    pub zoom: f32,
    /// Whether zoom should auto-fit to the canvas.
    pub zoom_fit: bool,
    /// Printer connection status message.
    pub printer_status: Option<String>,
    /// Detected printer model name.
    pub printer_model: Option<String>,
    /// Status bar message for transient feedback.
    pub status_message: String,
    /// Buffer for manual rotation angle input in properties panel.
    pub rotation_input: String,
    /// Buffer for font search/filter in properties panel.
    pub font_search: String,
    /// Cut & margin strategy (CenteredFull, PretrimCut, ChainPrint).
    pub cut_mode: CutMarginMode,
    /// Safe margin for PretrimCut in millimeters (default: 3.0 mm).
    pub small_margin_mm: f32,
    /// Whether the initial tape & cartridge setup modal is open.
    pub show_setup_modal: bool,
    /// Total length of current tape cartridge in meters (optional, e.g. 8.0).
    pub cartridge_total_length_m: Option<f32>,
    /// Text buffer for cartridge length in the setup modal.
    pub cartridge_length_input: String,
    /// Cumulative meters of tape printed in this session.
    pub tape_printed_meters: f32,
    /// Auto-cut after printing. When false, chain print mode (no cut).
    pub auto_cut: bool,
    /// Number of copies to print. Chained automatically to eliminate waste.
    pub copies: u32,
    /// Whether a printer is currently connected (detected by background poll).
    pub printer_connected: bool,
    /// Target selected for polling and printing.
    pub printer_target: PrinterTarget,
    /// Paired PT-P300BT targets discovered on macOS.
    pub bluetooth_targets: Vec<PrinterTarget>,
    /// Whether a printer operation (print, feed & cut) is in progress.
    pub operation_in_progress: bool,
    /// Whether the selected printer's status is being requested.
    pub connecting: bool,
    /// Maximum printable pixels of the last connected printer (0 initially).
    pub printer_max_px: u16,
    /// Print resolution of the last connected printer (180 initially).
    /// Kept across disconnects so the canvas does not resize on a
    /// transient USB glitch.
    pub printer_dpi: u16,
    /// Whether the last connected printer supports print quality modes.
    pub printer_quality_modes: bool,
    /// Selected print quality for the next print job.
    pub print_quality: PrintQuality,
    /// Channel sender for commands to the printer worker thread.
    pub printer_cmd_tx: Option<mpsc::Sender<PrinterCommand>>,
    /// Active view mode (single-label designer or multi-label batch overview).
    pub view_mode: ViewMode,
    /// Multi-label batch items in this project.
    pub batch_items: Vec<BatchItem>,
    /// Index of the active label being designed.
    pub active_batch_index: usize,
    /// Monotonic ID counter for batch items.
    pub next_batch_item_id: u64,
    /// Whether the batch series/text generator modal is visible.
    pub show_generator_modal: bool,
    /// Flag signaling cancellation of an active print operation.
    pub cancel_flag: Arc<AtomicBool>,
    /// Flag requesting keyboard focus on the primary text edit field.
    pub request_text_focus: bool,
    /// Margin in millimeters for the currently active label being edited.
    /// None => Centered Full (~27mm symmetrical margin, 0mm scrap).
    /// Some(mm) => Custom margin. If < 24.5mm, automatically triggers pre-trimming the leader scrap!
    pub margin_mm: Option<f32>,
    /// Optional maximum total finished label length in millimeters (including left & right margins).
    /// None => Auto length (grows with content).
    /// Some(mm) => Confines total label length to <= mm by auto-shrinking text font size when needed.
    pub max_length_mm: Option<f32>,
    /// Fit largest possible font in the confined area (tight ink bounding box + balanced multi-line wrap).
    pub fit_largest_font: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            elements: Vec::new(),
            selected_element: None,
            tape_width_mm: 12,
            tape_width_px: 76,
            font_name: "Inter".to_string(),
            font_margin: 0,
            overall_flip_h: false,
            overall_flip_v: false,
            available_fonts: Vec::new(),
            preview_bitmap: None,
            preview_texture: None,
            needs_rerender: true,
            zoom: 1.0,
            zoom_fit: true,
            printer_status: None,
            printer_model: None,
            status_message: "Ready".to_string(),
            rotation_input: String::new(),
            font_search: String::new(),
            cut_mode: CutMarginMode::CenteredFull,
            small_margin_mm: 3.0,
            show_setup_modal: true,
            cartridge_total_length_m: None,
            cartridge_length_input: String::new(),
            tape_printed_meters: 0.0,
            auto_cut: true,
            copies: 1,
            printer_connected: false,
            printer_target: PrinterTarget::Usb,
            bluetooth_targets: Vec::new(),
            operation_in_progress: false,
            connecting: false,
            printer_max_px: 0,
            printer_dpi: 180,
            printer_quality_modes: false,
            print_quality: PrintQuality::Standard,
            printer_cmd_tx: None,
            view_mode: ViewMode::Designer,
            batch_items: Vec::new(),
            active_batch_index: 0,
            next_batch_item_id: 1,
            show_generator_modal: false,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            request_text_focus: false,
            margin_mm: None,
            max_length_mm: None,
            fit_largest_font: true,
        }
    }
}

impl AppState {
    /// Signal the printer worker to abort any active print operation.
    pub fn request_cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    pub fn is_printer_busy(&self) -> bool {
        self.operation_in_progress || self.connecting
    }

    /// Effective margin in millimeters for the active label, accounting for `max_length_mm`.
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

    /// True if the active label requires pre-trimming the 24.5mm hardware leader scrap.
    pub fn is_pretrim(&self) -> bool {
        if self.margin_mm.is_none() && self.max_length_mm.is_none() {
            return false;
        }
        self.effective_margin_mm() < 24.5
    }

    /// Set `max_length_mm` for the active label, automatically switching to a 3.0mm compact
    /// margin if `fit_largest_font` is enabled (and no custom margin was set) or if the
    /// current/default (~27mm) margin would not fit within `max_len`.
    pub fn set_max_length_mm(&mut self, max_len: Option<f32>) {
        if let Some(len) = max_len {
            let clamped = len.clamp(10.0, 1000.0);
            self.max_length_mm = Some(clamped);
            if self.cut_mode != CutMarginMode::ChainPrint {
                let cur_margin = self.margin_mm.unwrap_or(27.0);
                if (self.fit_largest_font && self.margin_mm.is_none())
                    || cur_margin * 2.0 + 10.0 > clamped
                {
                    let compact = 3.0_f32.min((clamped * 0.2).max(1.5));
                    self.margin_mm = Some(compact);
                    self.cut_mode = CutMarginMode::PretrimCut;
                    self.small_margin_mm = compact;
                }
            }
        } else {
            self.max_length_mm = None;
        }
        self.sync_active_to_batch();
        self.mark_dirty();
    }

    /// Toggle the "Fit largest possible font in confined area" option.
    pub fn set_fit_largest_font(&mut self, enabled: bool) {
        self.fit_largest_font = enabled;
        if enabled
            && let Some(clamped) = self.max_length_mm
            && self.margin_mm.is_none()
            && self.cut_mode != CutMarginMode::ChainPrint
        {
            let compact = 3.0_f32.min((clamped * 0.2).max(1.5));
            self.margin_mm = Some(compact);
            self.cut_mode = CutMarginMode::PretrimCut;
            self.small_margin_mm = compact;
        }
        self.sync_active_to_batch();
        self.mark_dirty();
    }

    /// Maximum printable content width in pixels for the active label, if `max_length_mm` is set.
    pub fn max_content_width_px(&self) -> Option<u32> {
        let side_margin = if self.cut_mode == CutMarginMode::ChainPrint {
            2.0
        } else {
            self.effective_margin_mm()
        };
        LabelDocument::max_content_width_px_with_margin(
            self.max_length_mm,
            side_margin,
            self.printer_dpi,
        )
    }

    /// Update the tape width in pixels based on the current tape_width_mm
    /// and the connected printer's resolution.
    pub fn update_tape_pixels(&mut self) {
        if let Some(tape) = ptouch_core::tape::find_tape(self.tape_width_mm, self.printer_dpi) {
            let px = u32::from(tape.pixels);
            self.tape_width_px = if self.printer_max_px > 0 {
                px.min(u32::from(self.printer_max_px))
            } else {
                px
            };
        }
    }

    /// Mark the preview as needing re-render.
    pub fn mark_dirty(&mut self) {
        self.needs_rerender = true;
    }

    /// Ensure the selected element index is valid, defaulting to the first element if available.
    pub fn validate_selection(&mut self) {
        if self.elements.is_empty() {
            self.selected_element = None;
        } else if let Some(idx) = self.selected_element {
            if idx >= self.elements.len() {
                self.selected_element = Some(self.elements.len() - 1);
            }
        } else {
            self.selected_element = Some(0);
        }
    }

    /// Total number of labels to print across all batch items.
    pub fn total_batch_prints(&self) -> u32 {
        self.batch_items.iter().map(|item| item.copies).sum()
    }

    /// Ensure `batch_items` has at least one item initialized.
    pub fn ensure_batch_initialized(&mut self) {
        if self.batch_items.is_empty() {
            let test_items: [(&str, u32, &str, Option<f32>); 4] = [
                ("Ä Ö Ü ä ö ü ß • € 24,50", 2, "DINish", None),
                ("ACHTUNG!\nHochspannung 230V", 1, "Frutiger", Some(3.0)),
                ("SKIP ME (0 COPIES)", 0, "Inter", None),
                ("é è ê ç ñ ¿ ¡ ™ ® ©", 1, "Inter", Some(5.0)),
            ];

            for (text, copies, font, margin) in test_items {
                let doc = ptouch_render::document::LabelDocument {
                    version: ptouch_render::document::DOCUMENT_VERSION,
                    tape_width_mm: self.tape_width_mm,
                    dpi: self.printer_dpi,
                    font_name: font.to_string(),
                    font_margin: 0,
                    flip_h: false,
                    flip_v: false,
                    margin_mm: margin,
                    max_length_mm: None,
                    fit_largest_font: true,
                    elements: vec![LabelElement::Text {
                        content: text.to_string(),
                        font_size: None,
                        align: ptouch_render::text::TextAlign::Center,
                        rotation: 0.0,
                        flip_h: false,
                        flip_v: false,
                    }],
                };
                let title = text.lines().next().unwrap_or("Label").to_string();
                self.batch_items.push(BatchItem {
                    id: self.next_batch_item_id,
                    title,
                    copies,
                    document: doc,
                    preview_bitmap: None,
                    preview_texture: None,
                    dirty: true,
                });
                self.next_batch_item_id += 1;
            }

            self.active_batch_index = 0;
            self.elements = self.batch_items[0].document.elements.clone();
            self.font_name = self.batch_items[0].document.font_name.clone();
            self.copies = self.batch_items[0].copies.max(1);
            self.margin_mm = self.batch_items[0].document.margin_mm;
            self.max_length_mm = self.batch_items[0].document.max_length_mm;
            self.fit_largest_font = self.batch_items[0].document.fit_largest_font;
            self.selected_element = Some(0);
            self.mark_dirty();
        }
    }

    /// Sync the active editor state into `batch_items[active_batch_index]`.
    pub fn sync_active_to_batch(&mut self) {
        if self.batch_items.is_empty() {
            self.ensure_batch_initialized();
            return;
        }

        if self.active_batch_index < self.batch_items.len() {
            let doc = self.create_document_from_state();
            let title = self.derive_label_title();
            let preview_bmp = self.preview_bitmap.clone();
            let preview_tex = self.preview_texture.clone();
            let item = &mut self.batch_items[self.active_batch_index];
            item.document = doc;
            item.title = title;
            item.preview_bitmap = preview_bmp;
            item.preview_texture = preview_tex;
            item.dirty = false;
        }
    }

    /// Switch active label in the batch to `new_idx`.
    pub fn switch_active_batch(&mut self, new_idx: usize) {
        if new_idx >= self.batch_items.len() {
            return;
        }
        if new_idx != self.active_batch_index {
            self.sync_active_to_batch();
            self.active_batch_index = new_idx;
        }
        let doc = self.batch_items[new_idx].document.clone();
        if !self.printer_target.is_bluetooth() {
            self.tape_width_mm = doc.tape_width_mm;
            self.update_tape_pixels();
        }
        self.font_name = doc.font_name;
        self.font_margin = doc.font_margin;
        self.overall_flip_h = doc.flip_h;
        self.overall_flip_v = doc.flip_v;
        self.margin_mm = doc.margin_mm;
        self.max_length_mm = doc.max_length_mm;
        self.fit_largest_font = doc.fit_largest_font;
        self.elements = doc.elements;
        self.selected_element = if self.elements.is_empty() {
            None
        } else {
            Some(0)
        };
        self.mark_dirty();
    }

    /// Add a blank label to the batch.
    pub fn add_blank_batch_item(&mut self) {
        self.sync_active_to_batch();
        let new_num = self.batch_items.len() + 1;
        let doc = LabelDocument {
            version: DOCUMENT_VERSION,
            tape_width_mm: self.tape_width_mm,
            dpi: self.printer_dpi,
            font_name: self.font_name.clone(),
            font_margin: self.font_margin,
            flip_h: false,
            flip_v: false,
            margin_mm: self.margin_mm,
            max_length_mm: self.max_length_mm,
            fit_largest_font: self.fit_largest_font,
            elements: vec![LabelElement::Text {
                content: format!("Label {}", new_num),
                font_size: None,
                align: ptouch_render::text::TextAlign::Center,
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
            }],
        };
        let new_id = self.next_batch_item_id;
        self.next_batch_item_id += 1;
        let title = format!("Label {}", new_num);
        self.batch_items.push(BatchItem {
            id: new_id,
            title,
            copies: 1,
            document: doc,
            preview_bitmap: None,
            preview_texture: None,
            dirty: true,
        });
        self.switch_active_batch(self.batch_items.len() - 1);
        self.selected_element = Some(0);
    }

    /// Duplicate the batch item at `idx`.
    pub fn duplicate_batch_item(&mut self, idx: usize) {
        if idx >= self.batch_items.len() {
            return;
        }
        self.sync_active_to_batch();
        let mut cloned = self.batch_items[idx].clone();
        let new_id = self.next_batch_item_id;
        self.next_batch_item_id += 1;
        cloned.id = new_id;
        cloned.title = format!("{} (copy)", cloned.title);
        cloned.dirty = true;
        self.batch_items.insert(idx + 1, cloned);
        self.switch_active_batch(idx + 1);
    }

    /// Remove the batch item at `idx`.
    pub fn remove_batch_item(&mut self, idx: usize) {
        if self.batch_items.len() <= 1 {
            return; // keep at least 1 item
        }
        self.batch_items.remove(idx);
        let new_active = if self.active_batch_index >= self.batch_items.len() {
            self.batch_items.len() - 1
        } else if self.active_batch_index > idx {
            self.active_batch_index - 1
        } else {
            self.active_batch_index
        };
        self.active_batch_index = new_active;
        let doc = self.batch_items[new_active].document.clone();
        self.tape_width_mm = doc.tape_width_mm;
        self.font_name = doc.font_name;
        self.font_margin = doc.font_margin;
        self.overall_flip_h = doc.flip_h;
        self.overall_flip_v = doc.flip_v;
        self.margin_mm = doc.margin_mm;
        self.max_length_mm = doc.max_length_mm;
        self.fit_largest_font = doc.fit_largest_font;
        self.elements = doc.elements;
        self.selected_element = if self.elements.is_empty() {
            None
        } else {
            Some(0)
        };
        self.update_tape_pixels();
        self.mark_dirty();
    }

    /// Helper to create a LabelDocument from current state.
    pub fn create_document_from_state(&self) -> LabelDocument {
        LabelDocument {
            version: DOCUMENT_VERSION,
            tape_width_mm: self.tape_width_mm,
            dpi: self.printer_dpi,
            font_name: self.font_name.clone(),
            font_margin: self.font_margin,
            flip_h: self.overall_flip_h,
            flip_v: self.overall_flip_v,
            margin_mm: self.margin_mm,
            max_length_mm: self.max_length_mm,
            fit_largest_font: self.fit_largest_font,
            elements: self.elements.clone(),
        }
    }

    /// Derive a friendly label title from the first text element or fallback.
    pub fn derive_label_title(&self) -> String {
        for el in &self.elements {
            if let LabelElement::Text { content, .. } = el {
                let first_line = content.lines().next().unwrap_or("").trim();
                if !first_line.is_empty() {
                    return if first_line.len() > 24 {
                        format!("{}...", &first_line[..24])
                    } else {
                        first_line.to_string()
                    };
                }
            }
        }
        format!("Label {}", self.active_batch_index + 1)
    }
}
