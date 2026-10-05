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
    let id = "b96dab5c-7b48-456a-943a-bc89163cb133";
    let mut form = json!({"version":1,"root":{"type":"doc","content":[{"type":"form","attrs":{"id":id,"title":"Participate"}}]}});
    let doc = Document::parse(&form.to_string()).unwrap();
    assert!(doc.html().contains("embedded=true"));
    assert!(
        !doc.preview_html().contains("iframe"),
        "Draft inspection cannot create a live collector."
    );
    let email = doc.mail_html("https://local.example");
    assert!(
        email.contains(&format!("https://local.example/forms/{id}")) && !email.contains("iframe")
    );
    form["root"]["content"][0]["attrs"]["id"] = "https://remote.example".into();
    assert!(
        Document::parse(&form.to_string()).is_err(),
        "Form atoms cannot become arbitrary remote embeds."
    );
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

#[test]
fn image_dimensions_and_priority_are_bounded_and_external_preloads_are_excluded() {
    let id = uuid::Uuid::new_v4();
    let mut value = json!({"version":1,"root":{"type":"doc","content":[{"type":"paragraph","content":[{"type":"image","attrs":{"src":format!("/media/{id}"),"alt":"Lead image","width":640,"height":480,"loading":"eager"}}]}]}});
    let doc = Document::parse(&value.to_string()).unwrap();
    assert!(
        doc.html().contains("width=\"640\" height=\"480\"")
            && doc.html().contains("fetchpriority=\"high\"")
    );
    assert_eq!(doc.priority_image(), Some(format!("/media/{id}")));
    value["root"]["content"][0]["content"][0]["attrs"]["src"] =
        "https://external.example/image.png".into();
    assert!(
        Document::parse(&value.to_string())
            .unwrap()
            .priority_image()
            .is_none()
    );
    value["root"]["content"][0]["content"][0]["attrs"]["height"] = 0.into();
    assert!(Document::parse(&value.to_string()).is_err());
    value["root"]["content"][0]["content"][0]["attrs"]["height"] = 480.into();
    value["root"]["content"][0]["content"][0]["attrs"]["loading"] = "execute".into();
    assert!(Document::parse(&value.to_string()).is_err());
}
