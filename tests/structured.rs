//! Offline terms, admission and bounded discovery; not a search-eligibility claim.
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use wpalt::structured::declaration::{Definition, scalar};
use wpalt::structured::vocabulary;

fn fixtures() -> (wpalt::schema::Registry, BTreeMap<String, Definition>) {
    let registry = wpalt::schema::Registry {
        common: serde_json::from_value(json!({})).unwrap(),
        models: BTreeMap::from([
            ("location".into(),serde_json::from_value(json!({"label":"Locations","fields":{
                "name":{"kind":"string"},
                "address":{"kind":"object","fields":{"street":{"kind":"string"},"city":{"kind":"string"}}},
                "hours":{"kind":"repeater","max_items":7,"fields":{
                    "day":{"kind":"choice","choices":{"mon":"Monday","tue":"Tuesday"}},
                    "opens":{"kind":"string"},"closes":{"kind":"string"}}}
            }})).unwrap()),
            ("author".into(),serde_json::from_value(json!({"label":"Public authors","fields":{"name":{"kind":"string"}}})).unwrap()),
            ("article".into(),serde_json::from_value(json!({"label":"Stories","fields":{"author":{"kind":"relationship","target":"author"}}})).unwrap()),
        ]),
    };
    let definitions = BTreeMap::from([
        ("location".into(),serde_json::from_value(json!({"vocabulary":"30.1","primary":"location","entities":{
            "location":{"types":["Restaurant"],"properties":{
                "name":{"kind":"field","path":["name"],"datatype":"Text"},
                "address":{"kind":"entity","entity":"address"},
                "openingHoursSpecification":{"kind":"entity","entity":"hours"}}},
            "address":{"types":["PostalAddress"],"scope":["address"],"properties":{
                "streetAddress":{"kind":"field","path":["street"],"datatype":"Text"},
                "addressLocality":{"kind":"field","path":["city"],"datatype":"Text"}}},
            "hours":{"types":["OpeningHoursSpecification"],"scope":["hours"],"properties":{
                "dayOfWeek":{"kind":"enumeration","path":["day"],"choices":{"mon":"Monday","tue":"Tuesday"}},
                "opens":{"kind":"field","path":["opens"],"datatype":"Time"},
                "closes":{"kind":"field","path":["closes"],"datatype":"Time"}}}
        }})).unwrap()),
        ("author".into(),serde_json::from_value(json!({"vocabulary":"30.1","primary":"person","entities":{
            "person":{"types":["Person"],"properties":{"name":{"kind":"field","path":["name"],"datatype":"Text"}}}
        }})).unwrap()),
        ("article".into(),serde_json::from_value(json!({"vocabulary":"30.1","primary":"article","entities":{
            "article":{"types":["Article"],"properties":{
                "headline":{"kind":"publication","attribute":"title"},
                "datePublished":{"kind":"publication","attribute":"published_at"},
                "author":{"kind":"relationship","path":["author"],"entity":"person"}}}
        }})).unwrap()),
    ]);
    (registry, definitions)
}

#[test]
fn typed_location_hours_and_public_authors_admit_semantics_without_account_inference() {
    let (registry, definitions) = fixtures();
    registry.validate().unwrap();
    for (name, d) in &definitions {
        d.validate(&registry, name, &definitions).unwrap();
    }
    let values = json!({"name":"Downtown café","address":{"street":"1 Main St","city":"Tokyo"},"hours":[
        {"day":"mon","opens":"09:30","closes":"18:00"},{"day":"tue","opens":"10:00:00","closes":"17:00:00"}
    ]});
    definitions["location"].validate_values(&values).unwrap();
    let mut invalid = values.clone();
    invalid["hours"][1]["opens"] = "tomorrow".into();
    assert!(definitions["location"].validate_values(&invalid).is_err());
    assert_eq!(
        scalar(&json!("First line\n</script> & café"), "Text").unwrap(),
        json!("First line\n</script> & café")
    );
    assert!(scalar(&json!("secret\u{0000}"), "Text").is_err());
    assert!(scalar(&json!("javascript:alert(1)"), "URL").is_err());
    assert!(scalar(&json!("https://name:password@example.test"), "URL").is_err());
    assert!(scalar(&json!("2026-02-29"), "Date").is_err());
    assert!(scalar(&json!("2026-10-11T11:30:00"), "DateTime").is_err());
    assert!(scalar(&json!(true), "Integer").is_err());
}

#[test]
fn declaration_changes_refuse_wrong_ranges_missing_targets_and_incomplete_choice_maps() {
    let (registry, definitions) = fixtures();
    let article = serde_json::to_value(&definitions["article"]).unwrap();
    for (property, binding) in [
        ("author", json!({"kind":"entity","entity":"article"})),
        (
            "datePublished",
            json!({"kind":"publication","attribute":"title"}),
        ),
        (
            "author",
            json!({"kind":"relationship","path":["author"],"entity":"private_account"}),
        ),
        (
            "datePublished",
            json!({"kind":"publication","attribute":"url"}),
        ),
        (
            "unknownProperty",
            json!({"kind":"publication","attribute":"title"}),
        ),
        (
            "interactionCount",
            json!({"kind":"publication","attribute":"title"}),
        ),
    ] {
        let mut value = article.clone();
        value["entities"]["article"]["properties"][property] = binding;
        let d: Definition = serde_json::from_value(value).unwrap();
        assert!(
            d.validate(&registry, "article", &definitions).is_err(),
            "{property}"
        );
    }
    let mut location = serde_json::to_value(&definitions["location"]).unwrap();
    location["entities"]["hours"]["properties"]["dayOfWeek"]["choices"] = json!({"mon":"Monday"});
    let d: Definition = serde_json::from_value(location).unwrap();
    assert!(d.validate(&registry, "location", &definitions).is_err());
    let mut wrong = article;
    wrong["vocabulary"] = "future-live-vocabulary".into();
    let d: Definition = serde_json::from_value(wrong).unwrap();
    assert!(d.validate(&registry, "article", &definitions).is_err());
    assert!(
        serde_json::from_value::<Definition>(
            json!({"vocabulary":"30.1","primary":"x","entities":{},"raw_jsonld":{}})
        )
        .is_err()
    );
}

#[test]
fn one_projection_drives_visible_facts_public_relationships_and_finite_identity_cycles() {
    use wpalt::structured::projection::{project, visible};
    let (registry, mut definitions) = fixtures();
    let mut cycle = serde_json::to_value(&definitions["location"]).unwrap();
    cycle["entities"]["location"]["properties"]["parentOrganization"] =
        json!({"kind":"entity","entity":"location"});
    definitions.insert("location".into(), serde_json::from_value(cycle).unwrap());
    definitions["location"]
        .validate(&registry, "location", &definitions)
        .unwrap();
    let root = json!({"id":"location-1","kind":"location","title":"Find us","url":"/downtown","language":"en","published_at":1791676800,
        "fields":{"name":"Café\n</script> & one","address":{"street":"1 Main St","city":"Tokyo"},"hours":[{"day":"mon","opens":"09:30","closes":"18:00"}]}});
    let out = project(
        &definitions,
        &root,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        "https://example.test",
    )
    .unwrap()
    .unwrap();
    let nodes = out.graph["@graph"].as_array().unwrap();
    assert_eq!(
        nodes.len(),
        3,
        "a self reference emits the existing entity once"
    );
    assert_eq!(nodes[0]["parentOrganization"]["@id"], nodes[0]["@id"]);
    assert_eq!(nodes[2]["opens"], "09:30");
    assert_eq!(nodes[2]["dayOfWeek"], "https://schema.org/Monday");
    let markup = visible(&out.graph).into_string();
    assert!(markup.contains("Café\n&lt;/script&gt; &amp; one"));
    assert!(markup.contains("1 Main St") && markup.contains("09:30"));
    assert!(!markup.contains("<script>"));
    let article = json!({"id":"article-1","kind":"article","title":"Original published headline","url":"/story","language":"en","published_at":1791676800,"fields":{"author":"author-1"}});
    let author = json!({"id":"author-1","kind":"author","title":"Author profile","url":"/author","fields":{"name":"Public pen name"}});
    let related = BTreeMap::from([("author-1".into(), author)]);
    let out = project(
        &definitions,
        &article,
        &related,
        &BTreeMap::new(),
        &BTreeMap::new(),
        "https://example.test",
    )
    .unwrap()
    .unwrap();
    assert_eq!(out.graph["@graph"].as_array().unwrap().len(), 2);
    assert_eq!(out.graph["@graph"][0]["headline"], article["title"]);
    assert!(
        out.graph["@graph"][0]["datePublished"]
            .as_str()
            .unwrap()
            .ends_with('Z')
    );
    assert_eq!(out.graph["@graph"][1]["name"], "Public pen name");
    assert!(out.article && !out.organization);
    let revoked = project(
        &definitions,
        &article,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        "https://example.test",
    )
    .unwrap()
    .unwrap();
    assert!(revoked.graph["@graph"][0].get("author").is_none());
    assert_eq!(revoked.graph["@graph"].as_array().unwrap().len(), 1);
    assert!(revoked.warnings.iter().any(|w| w.contains("author")));
    let unrelated = json!({"id":"plain","kind":"plain"});
    assert!(
        project(
            &definitions,
            &unrelated,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            "https://example.test"
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn graph_budget_fails_instead_of_silently_truncating_repeated_facts() {
    use wpalt::structured::projection::project;
    let (_, mut definitions) = fixtures();
    let mut location = serde_json::to_value(&definitions["location"]).unwrap();
    location["entities"]["location"]["properties"]["openingHoursSpecification"] =
        json!({"kind":"entity","entity":"hours"});
    location["entities"]["hours"]["properties"]["description"] =
        json!({"kind":"field","path":["text"],"datatype":"Text"});
    definitions.insert("location".into(), serde_json::from_value(location).unwrap());
    let root = json!({"id":"location","kind":"location","url":"/location","fields":{"name":"Site","hours":vec![json!({"text":"x".repeat(8000)});9]}});
    assert!(
        project(
            &definitions,
            &root,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            "https://example.test"
        )
        .is_err()
    );
    let mut location = serde_json::to_value(&definitions["location"]).unwrap();
    location["entities"]["hours2"] = location["entities"]["hours"].clone();
    location["entities"]["location"]["properties"]["description"] =
        json!({"kind":"entity","entity":"hours2"});
    definitions.insert("location".into(), serde_json::from_value(location).unwrap());
    let root = json!({"id":"location","kind":"location","url":"/location","fields":{"hours":vec![json!({});50]}});
    assert!(
        project(
            &definitions,
            &root,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            "https://example.test"
        )
        .is_err()
    );
}

#[test]
fn provider_recommendations_are_distinct_from_requirements_and_catalog_authority() {
    let nodes = vec![
        json!({"@id":"article","@type":"Article","headline":"Story","author":{"@id":"person"},"datePublished":"not a date"}),
        json!({"@id":"person","@type":"Person","name":"Public author"}),
        json!({"@id":"location","@type":"LocalBusiness","name":"Actual business","address":{"@id":"address"}}),
        json!({"@id":"address","@type":"PostalAddress","url":"https://example.test/profile"}),
        json!({"@id":"faq","@type":"FAQPage"}),
        json!({"@id":"breadcrumb","@type":"BreadcrumbList","itemListElement":{"@id":"crumb"}}),
        json!({"@id":"crumb","@type":"ListItem","name":"Page","position":1,"item":"https://example.test/actual-target"}),
    ];
    let warnings = wpalt::structured::guidance::warnings(&nodes, vocabulary::get().unwrap());
    assert!(warnings.iter().any(|w|w.contains("Article recommendation")&&w.contains("not universally required")));
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("datePublished") && w.contains("ISO"))
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("LocalBusiness profile requires") && w.contains("actual physical"))
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("at least two ListItems"))
    );
    assert!(warnings.iter().any(|w| w.contains("retired in May 2026")));
    let valid_breadcrumb = vec![
        json!({"@id":"breadcrumb","@type":"BreadcrumbList","itemListElement":[{"@id":"first"},{"@id":"last"}]}),
        json!({"@id":"first","@type":"ListItem","position":1,"item":{"@id":"named"}}),
        json!({"@id":"named","@type":"WebPage","name":"Named page","url":"https://example.test/first"}),
        json!({"@id":"last","@type":"ListItem","position":2,"name":"Last page"}),
    ];
    assert!(
        wpalt::structured::guidance::warnings(&valid_breadcrumb, vocabulary::get().unwrap())
            .is_empty(),
        "Named Thing breadcrumb items satisfy the selected name check"
    );
    let (mut registry, mut definitions) = fixtures();
    registry
        .models
        .get_mut("article")
        .unwrap()
        .fields
        .insert("price".into(), wpalt::schema::Field::primitive("number"));
    let mut offer:Definition=serde_json::from_value(json!({"vocabulary":"30.1","primary":"offer","entities":{"offer":{"types":["Offer"],"properties":{"name":{"kind":"publication","attribute":"title"}}}}})).unwrap();
    definitions.insert("article".into(), offer.clone());
    offer.validate(&registry, "article", &definitions).unwrap();
    offer.entities.get_mut("offer").unwrap().properties.insert(
        "price".into(),
        serde_json::from_value(json!({"kind":"field","path":["price"],"datatype":"Number"}))
            .unwrap(),
    );
    assert!(
        offer
            .validate(&registry, "article", &definitions)
            .unwrap_err()
            .1
            .contains("native catalog authority")
    );
}

#[test]
fn pinned_vocabulary_supports_real_business_ancestry_and_truthful_status() {
    let v = vocabulary::get().unwrap();
    assert!(v.is_a("Restaurant", "Place") && v.is_a("Restaurant", "Organization"));
    assert!(v.associated(
        &["Restaurant".into()],
        v.property("openingHoursSpecification", false).unwrap()
    ));
    assert!(v.entity_type("Restaurant", false).is_ok());
    assert!(v.entity_type("Text", true).is_err());
    assert!(v.property("interactionCount", true).is_err());
    assert!(
        v.enumeration("Monday", &["DayOfWeek".into()], false)
            .is_ok()
    );
    assert!(v.enumeration("Monday", &["Person".into()], false).is_err());
    let pending = v
        .properties
        .iter()
        .find(|(_, t)| t.pending && t.superseded_by.is_empty())
        .unwrap()
        .0;
    assert!(v.property(pending, false).is_err());
    assert!(v.property(pending, true).is_ok());
    assert!(
        !v.property("author", false)
            .unwrap()
            .ranges
            .contains(&"Text".into())
    ); // A hint, not a closed ontology.
}

#[test]
fn offline_term_search_pages_without_duplicates_and_rejects_wrong_scope() {
    let v = vocabulary::get().unwrap();
    let business = v.search("type", "local bus", "", false).unwrap();
    assert!(business.items.iter().any(|t| t.name == "LocalBusiness"));
    let mut names = BTreeSet::new();
    let mut cursor = String::new();
    loop {
        let page = v.search("property", "", &cursor, true).unwrap();
        assert!(page.items.len() <= 40);
        for term in page.items {
            assert!(names.insert(term.name));
        }
        let Some(next) = page.after else {
            break;
        };
        assert_ne!(cursor, next);
        cursor = next;
    }
    assert_eq!(
        names.len(),
        v.properties
            .values()
            .filter(|p| p.superseded_by.is_empty())
            .count()
    );
    assert!(v.search("type", "article", "Restaurant", false).is_err());
    assert!(v.search("property", "</script>", "", false).is_err());
    assert!(v.search("unknown", "", "", false).is_err());
}
