//! One owner-readable module/ownership inventory. This is not a dynamic code loader.
use crate::config::Config;
use serde::Serialize;

#[derive(Serialize)]
pub struct Module {
    pub id: &'static str,
    pub enabled: bool,
    pub owns: &'static [&'static str],
    pub dependencies: &'static [&'static str],
}

/// Inventory reflects effective configuration, including parent feature admission.
/// Ownership names describe domain surfaces, not permission grants or runtime hooks.
pub fn inventory(c: &Config) -> Vec<Module> {
    vec![
        Module {
            id: "publishing",
            enabled: true,
            owns: &["content", "revisions", "taxonomy", "discovery", "themes"],
            dependencies: &[],
        },
        Module {
            id: "business",
            enabled: c.business_enabled,
            owns: &["forms", "audience", "campaigns", "bookings", "mail"],
            dependencies: &["publishing"],
        },
        Module {
            id: "engagement",
            enabled: c.business_enabled && c.engagement.enabled,
            owns: &["consent", "analytics", "experiments"],
            dependencies: &["business"],
        },
        Module {
            id: "membership",
            enabled: c.membership_enabled,
            owns: &[
                "policies",
                "entitlements",
                "courses",
                "progress",
                "referrals",
            ],
            dependencies: &["publishing"],
        },
        Module {
            id: "commerce",
            enabled: c.commerce.enabled,
            owns: &["catalog", "carts", "orders", "stock", "payments"],
            dependencies: &["publishing"],
        },
        Module {
            id: "operations",
            enabled: true,
            owns: &[
                "authentication",
                "media",
                "protection",
                "recovery",
                "maintenance",
            ],
            dependencies: &[],
        },
        Module {
            id: "extensions",
            enabled: true,
            owns: &["migration", "integration-grants", "content-events"],
            dependencies: &["publishing", "operations"],
        },
    ]
}

pub fn report(c: &Config) -> serde_json::Value {
    serde_json::json!({
        "format": "wpalt-module-inventory-v1",
        "modules": inventory(c),
        "runtime": if c.local_processes { "coordinated-local-processes" } else { "single-process" },
        "notice": "Inventory is configuration admission, not distributed certification, code unloading or permission authority."
    })
}
