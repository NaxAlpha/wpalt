//! Two reviewable clusters cover the trust boundary and imported authoring semantics.
use serde_json::json;
use wpalt::document::Document;
#[test]
fn document_import_and_render_preserve_meaning_without_executing_source() {
    let source = "# Ideas\n\nA **bold** and *gentle* [link](/garden).\n\n> Quote\n\n1. First\n2. Second\n\n```rust\nlet x = 1 < 2;\n```\n\n| Name | Value |\n|---|---|\n| café | 日本語 |\n\n<script>unsafe()</script>";
    let doc = wpalt::document::import(source, r#"[{"kind":"callout","text":"Remember"}]"#).unwrap();
    let html = Document::parse(&doc.encode()).unwrap().html();
    for expected in [
        "<h1>Ideas</h1>",
        "<strong>bold</strong>",
        "<em>gentle</em>",
        "href=\"/garden\"",
        "<blockquote>",
        "<ol start=\"1\">",
        "<table>",
        "<th scope=\"col\">",
        "café",
        "日本語",
        "class=\"callout\"",
        "Remember",
        "&lt;script&gt;",
    ] {
        assert!(html.contains(expected), "missing {expected}: {html}");
    }
    assert!(!html.contains("<script>"));
    assert!(doc.markdown().contains("Remember"));
}
#[test]
fn document_trust_boundary_rejects_unsafe_links_geometry_and_unbounded_work() {
    let valid = json!({"version":1,"root":{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"safe","marks":[{"type":"link","attrs":{"href":"/safe","title":null}}]}]}]}});
    Document::parse(&valid.to_string()).unwrap();
    for href in [
        "javascript:alert(1)",
        "data:text/html,x",
        "//remote.test",
        "https://user:secret@remote.test",
        "/bad\\path",
        "/bad\npath",
    ] {
        let mut v = valid.clone();
        v["root"]["content"][0]["content"][0]["marks"][0]["attrs"]["href"] = href.into();
        assert!(
            Document::parse(&v.to_string()).is_err(),
            "accepted {href:?}"
        );
    }
    let mut v = valid.clone();
    v["version"] = 2.into();
    assert!(Document::parse(&v.to_string()).is_err());
    v = valid.clone();
    v["root"]["content"][0]["attrs"] = json!({"onclick":"alert(1)"});
    assert!(Document::parse(&v.to_string()).is_err());
    v = valid;
    v["root"]["content"][0]["content"][0]["text"] = "x".repeat(512 * 1024 + 1).into();
    assert!(Document::parse(&v.to_string()).is_err());
    let mut nested = json!({"type":"paragraph"});
    for _ in 0..18 {
        nested = json!({"type":"blockquote","content":[nested]});
    }
    assert!(
        Document::parse(&json!({"version":1,"root":{"type":"doc","content":[nested]}}).to_string())
            .is_err()
    );
    let row = json!({"type":"table_row","content":[{"type":"table_cell","attrs":{"colspan":2,"rowspan":1,"colwidth":null},"content":[{"type":"paragraph"}]}]});
    assert!(Document::parse(&json!({"version":1,"root":{"type":"doc","content":[{"type":"table","content":[row]}]}}).to_string()).is_err());
    let html = wpalt::document::import("![description](/media/123)\n\n<script>x</script>", "[]")
        .unwrap()
        .html();
    assert!(html.contains("alt=\"description\""));
    assert!(!html.contains("<script>"));
}
