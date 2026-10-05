//! A content migration journey: ordered projection, exact loss review and hostile exports.
use serde_json::json;
use wpalt::platform::elementor;

#[test]
fn owner_can_review_projected_content_and_every_unsupported_element() {
    let source = json!({"title":"Reference landing page","type":"page","version":"0.4",
        "page_settings":{"background_color":"#eeeeee"},"content":[
        {"id":"layout","elType":"container","settings":{"flex_direction":"column"},"elements":[
            {"id":"title","elType":"widget","widgetType":"heading","settings":{"title":"<script>literal</script>","header_size":"h1","title_color":"red"},"elements":[]},
            {"id":"copy","elType":"widget","widgetType":"text-editor","settings":{"editor":"<p>A <strong>clear</strong> paragraph.</p><script>alert(1)</script>"},"elements":[]},
            {"id":"dynamic","elType":"widget","widgetType":"posts","settings":{"query_id":"members"},"elements":[
                {"id":"nested","elType":"widget","widgetType":"heading","settings":{"title":"Nested content"},"elements":[]}]}]}]});
    let projected = elementor::project(&serde_json::to_vec(&source).unwrap()).unwrap();
    let markdown = projected.document.markdown();
    assert!(markdown.contains("literal"));
    assert!(markdown.contains("clear"));
    assert!(markdown.contains("Nested content"));
    assert!(!markdown.contains("alert(1)"));
    assert!(markdown.find("literal").unwrap() < markdown.find("clear").unwrap());
    assert!(markdown.find("clear").unwrap() < markdown.find("Nested").unwrap());
    assert_eq!(projected.document.root.content[0].kind, "heading");
    assert_eq!(projected.report["elements"], 5);
    assert_eq!(projected.report["mappings"].as_array().unwrap().len(), 4);
    let warnings = projected.report["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w["element"] == "dynamic" && w["code"] == "unsupported_element")
    );
    assert!(
        warnings
            .iter()
            .any(|w| w["element"] == "layout" && w["code"] == "layout_settings_not_mapped")
    );
    assert!(
        warnings
            .iter()
            .any(|w| w["element"] == "title" && w["keys"] == json!(["title_color"]))
    );
    assert_eq!(
        projected.report["page_settings_not_mapped"],
        json!(["background_color"])
    );
    assert_eq!(
        projected.report,
        elementor::project(&serde_json::to_vec(&source).unwrap())
            .unwrap()
            .report
    );
    let mut duplicate = source.clone();
    duplicate["content"][0]["elements"][1]["id"] = json!("title");
    assert!(elementor::project(&serde_json::to_vec(&duplicate).unwrap()).is_err());
    let mut hostile = source.clone();
    hostile["content"][0]["elements"][1]["settings"]["editor"] = json!("<div>".repeat(10000));
    assert!(elementor::project(&serde_json::to_vec(&hostile).unwrap()).is_err());
    for bypass in ["<div></p>".repeat(200), "<div/>".repeat(200)] {
        hostile["content"][0]["elements"][1]["settings"]["editor"] = json!(bypass);
        assert!(elementor::project(&serde_json::to_vec(&hostile).unwrap()).is_err());
    }
    let mut unknown_version = source;
    unknown_version["version"] = json!("0.5");
    assert!(elementor::project(&serde_json::to_vec(&unknown_version).unwrap()).is_err());
    assert!(elementor::project(&vec![b' '; elementor::MAX_BYTES + 1]).is_err());
}
