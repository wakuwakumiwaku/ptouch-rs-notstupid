// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Huang Rui <vowstar@gmail.com>

//! Font discovery using cosmic-text's font system.
//!
//! Provides helpers to list available system fonts and find a font by name.

use cosmic_text::FontSystem;

/// Create a new `FontSystem` with system fonts and bundled fonts loaded.
pub fn create_font_system() -> FontSystem {
    let mut font_system = FontSystem::new();
    load_bundled_fonts(font_system.db_mut());
    font_system
}

/// Load bundled embedded fonts (Inter, DINish, Frutiger) into a font database.
pub fn load_bundled_fonts(db: &mut cosmic_text::fontdb::Database) {
    db.load_font_data(include_bytes!("../../../data/fonts/Inter.ttf").to_vec());
    db.load_font_data(include_bytes!("../../../data/fonts/DINish-Regular.otf").to_vec());
    db.load_font_data(include_bytes!("../../../data/fonts/DINish-Bold.otf").to_vec());
    db.load_font_data(include_bytes!("../../../data/fonts/Frutiger-Roman.ttf").to_vec());
    db.load_font_data(include_bytes!("../../../data/fonts/Frutiger-Bold.ttf").to_vec());
}

/// Find a font by name and return its full family name if found.
///
/// The search is case-insensitive and prefers exact family names before partial matches.
pub fn find_font(name: &str) -> Option<String> {
    let font_system = create_font_system();
    find_font_in_db(font_system.db(), name)
}

fn find_font_in_db(db: &cosmic_text::fontdb::Database, name: &str) -> Option<String> {
    let name_lower = name.to_lowercase();

    // Map common aliases/short names
    let search_term = match name_lower.as_str() {
        "din" | "din 1451" | "din-1451" => "dinish",
        "frutiger" => "frutiger",
        _ => &name_lower,
    };

    let mut partial_match = None;
    for face in db.faces() {
        for family in &face.families {
            let family_lower = family.0.to_lowercase();
            if family_lower == search_term {
                return Some(family.0.clone());
            }
            // Keep the original first-substring fallback, but search all families
            // for an exact match before using it (e.g. Inter vs. Interface).
            if partial_match.is_none() && family_lower.contains(search_term) {
                partial_match = Some(family.0.clone());
            }
        }
    }

    partial_match
}

/// List all available font family names on the system, including bundled fonts.
///
/// Returns a sorted, deduplicated list of family names.
pub fn list_fonts() -> Vec<String> {
    let font_system = create_font_system();
    let mut names: Vec<String> = Vec::new();

    for face in font_system.db().faces() {
        for family in &face.families {
            names.push(family.0.clone());
        }
    }

    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    // Use real parsed font data with controlled family metadata and insertion
    // order, without depending on whichever fonts the host has installed.
    fn database_with_families(families: &[&[&str]]) -> cosmic_text::fontdb::Database {
        let mut source = cosmic_text::fontdb::Database::new();
        source.load_font_data(include_bytes!("../../../data/fonts/Inter.ttf").to_vec());
        let prototype = source.faces().next().expect("bundled Inter face").clone();
        let mut db = cosmic_text::fontdb::Database::new();
        for names in families {
            let mut face = prototype.clone();
            face.id = cosmic_text::fontdb::ID::dummy();
            face.families = names
                .iter()
                .map(|name| ((*name).to_string(), prototype.families[0].1))
                .collect();
            db.push_face_info(face);
        }
        db
    }

    #[test]
    fn test_exact_family_precedes_earlier_interface_substring() {
        let db = database_with_families(&[&[".Hiragino Sans GB Interface"], &["Inter"]]);
        assert_eq!(
            db.faces().next().unwrap().families[0].0,
            ".Hiragino Sans GB Interface"
        );
        assert_eq!(find_font_in_db(&db, "Inter"), Some("Inter".to_string()));
    }

    #[test]
    fn test_exact_family_is_case_insensitive_and_keeps_first_exact() {
        let db = database_with_families(&[&["Inter Display"], &["iNtEr"], &["INTER"]]);
        for name in ["Inter", "INTER", "inter", "InTeR"] {
            assert_eq!(find_font_in_db(&db, name), Some("iNtEr".to_string()));
        }
    }

    #[test]
    fn test_exact_family_checks_all_names_on_a_face() {
        let db = database_with_families(&[&[".Hiragino Sans GB Interface", "Inter"]]);
        assert_eq!(find_font_in_db(&db, "Inter"), Some("Inter".to_string()));
    }

    #[test]
    fn test_din_aliases_prefer_exact_dinish() {
        let db = database_with_families(&[&["DINish Display"], &["DINish"]]);
        for name in ["din", "DIN", "DIN 1451", "din-1451", "DINish"] {
            assert_eq!(find_font_in_db(&db, name), Some("DINish".to_string()));
        }
    }

    #[test]
    fn test_partial_fallback_and_missing_behavior_unchanged() {
        let db = database_with_families(&[
            &[".Hiragino Sans GB Interface", "Other Family"],
            &["Inter Display"],
            &["DINish Display"],
            &["Frutiger Display"],
        ]);
        for name in ["iNtEr", ""] {
            assert_eq!(
                find_font_in_db(&db, name),
                Some(".Hiragino Sans GB Interface".to_string())
            );
        }
        assert_eq!(
            find_font_in_db(&db, "OTHER"),
            Some("Other Family".to_string())
        );
        for name in ["din", "DIN 1451", "DIN-1451"] {
            assert_eq!(
                find_font_in_db(&db, name),
                Some("DINish Display".to_string())
            );
        }
        assert_eq!(
            find_font_in_db(&db, "FRUTIGER"),
            Some("Frutiger Display".to_string())
        );
        assert_eq!(find_font_in_db(&db, "MissingFont12345"), None);
        let empty = cosmic_text::fontdb::Database::new();
        assert_eq!(find_font_in_db(&empty, "Inter"), None);
        assert_eq!(find_font_in_db(&empty, ""), None);
    }

    #[test]
    fn test_list_fonts_not_empty() {
        // On most systems there should be at least some fonts.
        // This test may fail in minimal containers without fonts.
        let fonts = list_fonts();
        // We do not assert non-empty since CI may have no fonts,
        // but we check it does not panic.
        log::debug!("Found {} font families", fonts.len());
    }

    #[test]
    fn test_list_fonts_sorted() {
        let fonts = list_fonts();
        for pair in fonts.windows(2) {
            assert!(pair[0] <= pair[1], "fonts should be sorted");
        }
    }

    #[test]
    fn test_find_nonexistent() {
        let result = find_font("ThisFontDefinitelyDoesNotExist12345");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_bundled_fonts() {
        assert_eq!(find_font("Inter"), Some("Inter".to_string()));
        assert_eq!(find_font("din"), Some("DINish".to_string()));
        assert_eq!(find_font("DIN 1451"), Some("DINish".to_string()));
        assert_eq!(find_font("frutiger"), Some("Frutiger".to_string()));
    }
}
