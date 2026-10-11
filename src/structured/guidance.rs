//! Selected provider checks reviewed 2026-10-11. No live eligibility claim.
use super::{declaration, vocabulary::Vocabulary};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn is_a(node: &Value, v: &Vocabulary, ancestor: &str) -> bool {
    declaration::items(&node["@type"])
        .iter()
        .any(|t| t.as_str().is_some_and(|t| v.is_a(t, ancestor)))
}
pub fn warnings(nodes: &[Value], v: &Vocabulary) -> Vec<String> {
    let identities: BTreeMap<_, _> = nodes
        .iter()
        .filter_map(|n| n["@id"].as_str().map(|id| (id, n)))
        .collect();
    let resolve = |value: &Value| -> Option<&Value> {
        value["@id"]
            .as_str()
            .and_then(|id| identities.get(id).copied())
    };
    let mut warnings = Vec::new();
    for node in nodes {
        let label = node["@id"].as_str().unwrap_or("Published entity");
        let mut warn = |message: &str| warnings.push(format!("{label}: {message}"));
        if is_a(node, v, "Article") {
            for name in ["headline", "author", "datePublished", "image"] {
                if node.get(name).is_none() {
                    warn(&format!(
                        "Google Article recommendation: review missing {name} if applicable; it is not universally required."
                    ));
                }
            }
            for date in ["datePublished", "dateModified"] {
                if let Some(value) = node.get(date)
                    && value
                        .as_str()
                        .is_none_or(|s| chrono::DateTime::parse_from_rfc3339(s).is_err())
                {
                    warn(&format!(
                        "Google Article recommendation: review {date} as a factual ISO date/time with timezone."
                    ));
                }
            }
            for author in declaration::items(&node["author"]) {
                if let Some(author) = resolve(author) {
                    if !(is_a(author, v, "Person") || is_a(author, v, "Organization"))
                        || author["name"].as_str().is_none_or(str::is_empty)
                    {
                        warn(
                            "Google Article recommendation: identify each public author as a named Person or Organization.",
                        );
                    }
                    if author.get("url").is_none() && author.get("sameAs").is_none() {
                        warn(
                            "Google Article recommendation: review a factual public author profile URL or sameAs link.",
                        );
                    }
                } else {
                    warn(
                        "Google Article recommendation: author text/URL fallbacks do not supply the recommended named Person/Organization graph.",
                    );
                }
            }
        }
        if is_a(node, v, "LocalBusiness") {
            if node["name"].as_str().is_none_or(str::is_empty) {
                warn(
                    "Google LocalBusiness profile requires a factual business name; vocabulary admission alone does not satisfy that profile.",
                );
            }
            let addresses = declaration::items(&node["address"]);
            if addresses.is_empty()
                || addresses.iter().any(|a| {
                    resolve(a).is_none_or(|n| {
                        !is_a(n, v, "PostalAddress")
                            || ![
                                "streetAddress",
                                "addressLocality",
                                "addressRegion",
                                "postalCode",
                                "addressCountry",
                            ]
                            .iter()
                            .any(|p| n.get(p).is_some_and(|v| !declaration::items(v).is_empty()))
                    })
                })
            {
                warn(
                    "Google LocalBusiness profile requires an actual physical PostalAddress; add factual address details rather than an empty entity or text fallback.",
                );
            }
            if declaration::items(&node["@type"])
                .iter()
                .any(|t| t.as_str() == Some("LocalBusiness"))
            {
                warn(
                    "Google LocalBusiness guidance recommends the most specific accurate business subtype.",
                );
            }
        } else if is_a(node, v, "Organization") && node.get("name").is_none() {
            warn(
                "Google Organization recommendation: review a factual public organization name; there are no universally required properties.",
            );
        }
        if is_a(node, v, "BreadcrumbList") {
            let refs = declaration::items(&node["itemListElement"]);
            if refs.len() < 2 {
                warn("Google Breadcrumb profile needs at least two ListItems.");
            }
            let mut positions = BTreeSet::new();
            for (i, item) in refs.iter().enumerate() {
                let Some(item) = resolve(item) else {
                    warn("Google Breadcrumb profile expects named ListItem entities.");
                    continue;
                };
                if !is_a(item, v, "ListItem")
                    || item["position"]
                        .as_i64()
                        .is_none_or(|p| p < 1 || !positions.insert(p))
                {
                    warn(
                        "Review distinct positive integer positions on ordered breadcrumb ListItems.",
                    );
                }
                if item["name"].as_str().is_none_or(str::is_empty)
                    && resolve(&item["item"])
                        .is_none_or(|target| target["name"].as_str().is_none_or(str::is_empty))
                {
                    warn("Review a factual breadcrumb name, or a named Thing item.");
                }
                if i + 1 < refs.len() && item.get("item").is_none() {
                    warn(
                        "Google Breadcrumb profile needs a target item URL before the final breadcrumb.",
                    );
                }
            }
        }
        if is_a(node, v, "FAQPage") {
            warn("FAQPage remains a vocabulary type; Google FAQ rich results retired in May 2026.");
        }
    }
    warnings
}
