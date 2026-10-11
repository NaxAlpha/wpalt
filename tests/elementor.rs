//! A content migration journey: ordered projection, exact loss review and hostile exports.
use serde_json::json;
use wpalt::platform::elementor;

#[test]
fn design_review_accounts_for_atomic_styles_and_refuses_unsafe_graphs() {
    use wpalt::{
        platform::elementor_design::{self, Request},
        schema::{Definition, Model, Registry},
        theme::Package,
    };
    let registry = Registry {
        common: Definition::initial(),
        models: [("post".into(), Model::initial("Posts"))].into(),
    };
    let base: Package = serde_json::from_value(json!({"format":2,"name":"Review base","tokens":{"background":"#ffffff","panel":"#ffffff","text":"#111111","muted":"#555555","accent":"#006655","font":"system"},"header":{"id":"head","kind":"section"},"footer":{"id":"foot","kind":"section"},"templates":{"home":{"id":"home","kind":"section"},"search":{"id":"search","kind":"section"},"content":{"id":"content","kind":"body","text":{"bind":"post.body"}}}})).unwrap();
    let source = json!({"title":"Atomic landing","type":"page","version":"0.4","page_settings":[],"content":[{"id":"layout","elType":"container","settings":{"flex_direction":"column","gap":{"unit":"px","size":24}},"elements":[{"id":"title","elType":"widget","widgetType":"e-heading","settings":{"title":{"$$type":"string","value":"<script>Literal title</script>"},"tag":{"$$type":"string","value":"h1"},"typography_font_size":{"unit":"px","size":32},"unknown_control":"retained in source"},"styles":{"unsafe":"raw css is never executed"},"interactions":[{"trigger":"load"}],"elements":[]},{"id":"button","elType":"widget","widgetType":"button","settings":{"text":"Explore","link":{"url":"/explore","is_external":true}},"elements":[]}]}]});
    let request: Request = serde_json::from_value(
        json!({"source":source,"component":"imported-design","target":"home"}),
    )
    .unwrap();
    let review =
        elementor_design::review("paper", base.clone(), 1, request.clone(), &registry).unwrap();
    let container = &review.package.components["imported-design"].root.children[0];
    assert_eq!(container.style.gap, 24);
    let heading = &container.children[0];
    assert_eq!(heading.kind, "heading");
    assert_eq!(heading.level, 1);
    assert_eq!(heading.style.font_size, 32);
    assert_eq!(
        heading.text, "<script>Literal title</script>",
        "Native headings escape source text."
    );
    let losses = review.report["losses"].as_array().unwrap();
    for key in ["styles", "interactions", "unknown_control"] {
        assert!(
            losses.iter().any(|l| l["detail"].to_string().contains(key)),
            "Missing loss for {key}"
        );
    }
    assert_ne!(
        review.fingerprint,
        elementor_design::review("another-theme", base.clone(), 1, request.clone(), &registry)
            .unwrap()
            .fingerprint
    );
    assert_ne!(
        review.fingerprint,
        elementor_design::review("paper", base.clone(), 2, request.clone(), &registry)
            .unwrap()
            .fingerprint
    );
    let mut styled_base = base.clone();
    styled_base.styles.insert(
        "brand".into(),
        serde_json::from_value(json!({"color":"#225544"})).unwrap(),
    );
    let mut global_request = request.clone();
    global_request.source["content"][0]["elements"][0]["settings"]["__globals__"] =
        json!({"title_color":"globals/colors?id=primary"});
    let missing = elementor_design::review(
        "paper",
        styled_base.clone(),
        1,
        global_request.clone(),
        &registry,
    )
    .unwrap();
    assert!(
        missing.report["losses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["code"] == "global_style_mapping_required")
    );
    global_request
        .global_styles
        .insert("globals/colors?id=primary".into(), "brand".into());
    let mapped =
        elementor_design::review("paper", styled_base, 1, global_request, &registry).unwrap();
    assert_eq!(
        mapped.package.components["imported-design"].root.children[0].children[0]
            .style
            .color,
        "#225544"
    );
    assert!(
        mapped.report["mappings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["native_style"] == "brand")
    );
    let mut rich = request.clone();
    rich.source["content"][0]["elements"][1]["widgetType"] = json!("text-editor");
    rich.source["content"][0]["elements"][1]["settings"] = json!({"editor":"<p>![smuggled](https://remote.invalid/pixel) &lt;img src=\"https://remote.invalid/escaped\"&gt;</p><p><a href=\"/a)![smuggled](https://remote.invalid/link)\">Trusted link</a></p><img src=\"https://remote.invalid/actual\" alt=\"Image omitted\"/><pre>`````\n![code](https://remote.invalid/code)</pre>"});
    let safe = elementor_design::review("paper", base.clone(), 1, rich, &registry).unwrap();
    let text = safe.package.components["imported-design"].root.children[0].children[1]
        .text
        .as_str()
        .unwrap();
    let rendered = wpalt::content::markdown(text);
    assert!(
        !rendered.contains("<img"),
        "Literal Markdown, encoded HTML, link destinations and code cannot create foreign image requests: {rendered}"
    );
    assert!(rendered.contains("Trusted link") && rendered.contains("Image omitted"));
    assert!(
        safe.report["losses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["code"] == "inline_image_not_imported")
    );
    let mut nested = request.clone();
    nested.source["type"] = json!("section");
    nested.source["content"][0]["elements"][0]["elements"] = json!([{"id":"nestedchild","elType":"widget","widgetType":"button","settings":{"text":"Visible child","link":{"url":"/child"}},"elements":[]}]);
    nested.source["content"][0]["elements"][0]["settings"]["typography_font_family"] =
        json!("Source font");
    nested.fonts.insert("Source font".into(), "serif".into());
    let lifted = elementor_design::review("paper", base.clone(), 1, nested, &registry).unwrap();
    let group = &lifted.package.components["imported-design"].root.children[0].children[0];
    assert_eq!(group.kind, "section");
    assert_eq!(group.children[0].style.font, "serif");
    assert_eq!(group.children[1].text, "Visible child");
    assert!(
        lifted.report["losses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["code"] == "leaf_children_lifted")
    );
    let mut hostile = request.clone();
    hostile.source["content"][0]["elements"][1]["settings"]["link"]["url"] =
        json!("javascript:alert(1)");
    assert!(elementor_design::review("paper", base.clone(), 1, hostile, &registry).is_err());
    let mut duplicate = request.clone();
    duplicate.source["content"][0]["elements"][1]["id"] = json!("title");
    assert!(elementor_design::review("paper", base.clone(), 1, duplicate, &registry).is_err());
    let mut wide = request.clone();
    let widget = wide.source["content"][0]["elements"][0].clone();
    wide.source["content"] = json!(
        (0..513)
            .map(|i| {
                let mut w = widget.clone();
                w["id"] = json!(format!("id{i}"));
                w
            })
            .collect::<Vec<_>>()
    );
    assert!(elementor_design::review("paper", base.clone(), 1, wide, &registry).is_err());
    let mut unmapped = request;
    unmapped.source["content"][0]["elements"][1]["widgetType"] = json!("form");
    unmapped.source["content"][0]["elements"][1]["settings"] =
        json!({"html":"<script>execute()</script>"});
    let refused = elementor_design::review("paper", base, 1, unmapped, &registry).unwrap();
    assert!(
        refused.report["losses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["code"] == "widget_not_mapped")
    );
    assert!(
        !serde_json::to_string(&refused.package)
            .unwrap()
            .contains("execute()")
    );
}

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
