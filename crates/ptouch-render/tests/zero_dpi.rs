// SPDX-License-Identifier: MIT

use ptouch_render::document::{LabelBatch, LabelDocument};

fn document(dpi: Option<u16>) -> String {
    let dpi = dpi.map_or(String::new(), |dpi| format!("dpi = {dpi}\n"));
    format!(
        "version = 1\ntape_width_mm = 12\n{dpi}font_name = \"Inter\"\n\
         font_margin = 0\nelements = []\n"
    )
}

fn batch(dpi: Option<u16>, label_dpi: Option<u16>) -> String {
    let dpi = dpi.map_or(String::new(), |dpi| format!("dpi = {dpi}\n"));
    format!(
        "version = 1\ntape_width_mm = 12\n{dpi}\n[[labels]]\n\
         [labels.document]\n{}",
        document(label_dpi)
    )
}

#[test]
fn single_document_and_single_as_batch_reject_zero_dpi() {
    let text = document(Some(0));
    let error = LabelDocument::from_toml_str(&text).unwrap_err();
    assert!(
        error.to_string().contains("dpi must be greater than zero"),
        "{error}"
    );
    assert!(LabelBatch::from_toml_str(&text).is_err());
}

#[test]
fn batch_metadata_and_each_label_reject_zero_dpi() {
    for (dpi, label_dpi) in [(Some(0), Some(180)), (Some(180), Some(0))] {
        let text = batch(dpi, label_dpi);
        let error = LabelBatch::from_toml_str(&text).unwrap_err();
        assert!(
            error.to_string().contains("dpi must be greater than zero"),
            "{error}"
        );
    }
}

#[test]
fn omitted_and_all_positive_dpi_values_remain_accepted() {
    for dpi in [
        None,
        Some(1),
        Some(180),
        Some(300),
        Some(360),
        Some(u16::MAX),
    ] {
        let expected = dpi.unwrap_or(180);
        assert_eq!(
            LabelDocument::from_toml_str(&document(dpi)).unwrap().dpi,
            expected
        );
        let parsed = LabelBatch::from_toml_str(&batch(dpi, dpi)).unwrap();
        assert_eq!(parsed.dpi, expected);
        assert_eq!(parsed.labels[0].document.dpi, expected);
    }
}
