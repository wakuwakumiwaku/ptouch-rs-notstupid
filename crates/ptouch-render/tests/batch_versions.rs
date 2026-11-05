use ptouch_render::document::{BatchEntry, DOCUMENT_VERSION, LabelBatch, LabelDocument};

fn document(version: u32) -> LabelDocument {
    LabelDocument::from_toml_str(
        "version = 1\ntape_width_mm = 12\nfont_name = \"Inter\"\nfont_margin = 0\n\
         [[elements]]\ntype = \"padding\"\npixels = 8\n",
    )
    .map(|mut doc| {
        doc.version = version;
        doc
    })
    .unwrap()
}

fn batch(versions: &[u32]) -> LabelBatch {
    LabelBatch {
        version: DOCUMENT_VERSION,
        tape_width_mm: 12,
        dpi: 180,
        labels: versions
            .iter()
            .map(|&version| BatchEntry {
                title: "Synthetic label".into(),
                copies: 2,
                document: document(version),
            })
            .collect(),
    }
}

#[test]
fn rejects_unsupported_embedded_document_versions() {
    for version in [0, DOCUMENT_VERSION + 1, u32::MAX] {
        for index in 0..3 {
            let mut versions = [DOCUMENT_VERSION; 3];
            versions[index] = version;
            let text = batch(&versions).to_toml_string().unwrap();
            let result = LabelBatch::from_toml_str(&text);
            assert!(
                result.is_err(),
                "accepted unsupported document version {version} at label {index}"
            );
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("unsupported layout version"),
                "expected the document version diagnostic"
            );
        }
    }
}

#[test]
fn standalone_and_embedded_documents_follow_the_same_version_contract() {
    for version in [0, DOCUMENT_VERSION, DOCUMENT_VERSION + 1] {
        let standalone = document(version).to_toml_string().unwrap();
        let embedded = batch(&[version]).to_toml_string().unwrap();
        assert_eq!(
            LabelBatch::from_toml_str(&embedded).is_ok(),
            LabelDocument::from_toml_str(&standalone).is_ok(),
            "document version {version} must not depend on its container"
        );
    }
}

#[test]
fn supported_batch_preserves_labels_and_defaults() {
    let text = batch(&[DOCUMENT_VERSION; 3]).to_toml_string().unwrap();
    let loaded = LabelBatch::from_toml_str(&text).unwrap();
    assert_eq!(loaded.labels.len(), 3);
    assert_eq!(loaded.labels[1].copies, 2);
    assert_eq!(loaded.labels[1].document.version, DOCUMENT_VERSION);
    assert_eq!(loaded.labels[1].document.dpi, 180);
}

#[test]
fn single_document_fallback_keeps_version_validation() {
    for version in [0, DOCUMENT_VERSION, DOCUMENT_VERSION + 1] {
        let text = document(version).to_toml_string().unwrap();
        assert_eq!(
            LabelBatch::from_toml_str(&text).is_ok(),
            version == DOCUMENT_VERSION
        );
    }
}

#[test]
fn outer_batch_version_is_still_validated() {
    for version in [0, DOCUMENT_VERSION + 1] {
        let mut value = batch(&[DOCUMENT_VERSION]);
        value.version = version;
        assert!(LabelBatch::from_toml_str(&value.to_toml_string().unwrap()).is_err());
    }
}
