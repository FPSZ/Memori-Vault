//! Integration tests for legacy OLE2 Office formats (.doc / .ppt / .xls).
//!
//! Fixtures in `tests/fixtures/` are REAL Office 97-2003 binary files (produced
//! from the v2 corpus via Microsoft Office), so these tests exercise the actual
//! CFBF/BIFF/Word-FIB/PowerPoint-record parsing against genuine output, not
//! synthetic data.

use std::path::PathBuf;

use memori_parser::extract_document_text;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn extract_legacy_doc_recovers_chinese_body() {
    let text = extract_document_text(fixture("legacy_sample.doc")).expect("extract .doc");
    assert!(text.contains("极光账本"), "missing project name: {text}");
    assert!(text.contains("字段契约评审"), "missing buried fact: {text}");
    assert!(text.contains("林知远"), "missing owner: {text}");
}

#[test]
fn extract_legacy_xls_recovers_cell_values() {
    let text = extract_document_text(fixture("legacy_sample.xls")).expect("extract .xls");
    assert!(text.contains("18,600 张"), "missing cell value: {text}");
    assert!(
        text.contains("一期北极星指标"),
        "missing param label: {text}"
    );
}

#[test]
fn extract_legacy_ppt_recovers_slide_text() {
    let text = extract_document_text(fixture("legacy_sample.ppt")).expect("extract .ppt");
    assert!(
        text.contains("18,600 张已核销对账单"),
        "missing slide fact: {text}"
    );
    assert!(text.contains("极光账本"), "missing project name: {text}");
}
