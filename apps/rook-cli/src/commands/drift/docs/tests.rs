use super::*;

#[test]
fn slugs_match_github() {
    assert_eq!(slug("The Terrain API"), "the-terrain-api");
    assert_eq!(
        slug("Surface joints (`MakeJoints`)"),
        "surface-joints-makejoints"
    );
    assert_eq!(
        slug("M4 Content: meshes, materials"),
        "m4-content-meshes-materials"
    );
}

#[test]
fn repeated_headings_get_numbered_anchors() {
    let doc = Doc::new(
        "a.md".into(),
        "# A\n## Method\n```\n## not\n```\n## Method\n",
    );
    let anchors = doc.anchors();
    assert!(anchors.contains("method") && anchors.contains("method-1"));
    assert!(!anchors.contains("not"));
}

#[test]
fn links_resolve_from_the_doc_and_skip_code() {
    let doc = Doc::new(
        "docs/behavior/gui/text.md".into(),
        "# T\nSee [layout](layout.md#drawing), [plan](../../roadmap/README.md) and `[x](y)`.\n",
    );
    let links = doc.links();
    assert_eq!(links.len(), 2);
    assert_eq!(
        links[0].path.as_deref(),
        Some("docs/behavior/gui/layout.md")
    );
    assert_eq!(links[0].anchor.as_deref(), Some("drawing"));
    assert_eq!(links[1].path.as_deref(), Some("docs/roadmap/README.md"));
}

#[test]
fn dates_count_days() {
    assert_eq!(days("1970-01-01"), Some(0));
    assert_eq!(
        days("2026-10-21")
            .zip(days("2026-10-20"))
            .map(|(a, b)| a - b),
        Some(1)
    );
    assert_eq!(days("2026-13-01"), None);
}

#[test]
fn checked_lines_parse() {
    assert!(behavior::checked("Checked against Studio 0.743 on 2026-10-21.").is_some());
    assert!(
        behavior::checked("Checked against Studio 0.742 on 2026-10-16, in a fresh Studio.")
            .is_some()
    );
    assert!(behavior::checked("Measured in Studio 0.742 on 2026-10-16.").is_none());
}
