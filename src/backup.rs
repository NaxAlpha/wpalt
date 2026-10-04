use crate::{
    App,
    auth::digest,
    error::{Error, Result},
};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Any, Execute, QueryBuilder, Row};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

/// Bound the read itself: metadata alone cannot prevent growth between stat and read.
pub async fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let file = tokio::fs::File::open(path).await?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes).await?;
    if bytes.len() > limit {
        return Err(Error::invalid(
            "Stored file exceeds its supported size or backup budget.",
        ));
    }
    Ok(bytes)
}

// Types and table names are an allowlist, never supplied by an archive.
pub(crate) const TABLES: &[(&str, &[(&str, bool)])] = &[
    (
        "user_passkeys",
        &[
            ("credential_id", false),
            ("user_id", false),
            ("definition", false),
            ("version", true),
        ],
    ),
    (
        "user_factors",
        &[
            ("user_id", false),
            ("secret", false),
            ("pending", false),
            ("pending_until", true),
            ("last_step", true),
            ("recovery", false),
        ],
    ),
    (
        "discovery_settings",
        &[("id", true), ("definition", false), ("version", true)],
    ),
    (
        "redirects",
        &[
            ("source", false),
            ("target", false),
            ("code", true),
            ("version", true),
        ],
    ),
    (
        "settings",
        &[
            ("id", true),
            ("title", false),
            ("description", false),
            ("theme", false),
            ("navigation", false),
            ("field_schema", false),
        ],
    ),
    (
        "content_models",
        &[("id", false), ("definition", false), ("version", true)],
    ),
    (
        "site_design",
        &[
            ("id", true),
            ("draft_options", false),
            ("live_options", false),
            ("version", true),
            ("published_version", true),
        ],
    ),
    (
        "themes",
        &[
            ("id", false),
            ("name", false),
            ("draft", false),
            ("live", false),
            ("version", true),
            ("published_version", true),
            ("updated_at", true),
        ],
    ),
    (
        "theme_revisions",
        &[
            ("id", false),
            ("theme_id", false),
            ("version", true),
            ("package", false),
            ("published", true),
            ("created_at", true),
        ],
    ),
    (
        "users",
        &[
            ("id", false),
            ("email", false),
            ("name", false),
            ("role", false),
            ("password_hash", false),
            ("created_at", true),
        ],
    ),
    (
        "posts",
        &[
            ("id", false),
            ("slug", false),
            ("kind", false),
            ("title", false),
            ("body", false),
            ("document", false),
            ("fields", false),
            ("blocks", false),
            ("status", false),
            ("version", true),
            ("published_slug", false),
            ("published_title", false),
            ("published_body", false),
            ("published_document", false),
            ("published_fields", false),
            ("published_blocks", false),
            ("publish_at", true),
            ("published_at", true),
            ("updated_at", true),
            ("author_id", false),
            ("locale", false),
            ("translation_group", false),
            ("seo", false),
            ("published_locale", false),
            ("published_translation_group", false),
            ("published_seo", false),
        ],
    ),
    (
        "revisions",
        &[
            ("id", false),
            ("post_id", false),
            ("version", true),
            ("snapshot", false),
            ("created_at", true),
        ],
    ),
    (
        "terms",
        &[
            ("id", false),
            ("name", false),
            ("slug", false),
            ("kind", false),
        ],
    ),
    ("post_terms", &[("post_id", false), ("term_id", false)]),
    (
        "published_post_terms",
        &[("post_id", false), ("term_id", false)],
    ),
    (
        "media",
        &[
            ("id", false),
            ("filename", false),
            ("original_name", false),
            ("mime", false),
            ("alt", false),
            ("visibility", false),
            ("size", true),
            ("sha256", false),
            ("created_at", true),
        ],
    ),
    (
        "comments",
        &[
            ("id", false),
            ("post_id", false),
            ("name", false),
            ("body", false),
            ("status", false),
            ("created_at", true),
        ],
    ),
    (
        "business_forms",
        &[
            ("id", false),
            ("owner_id", false),
            ("draft", false),
            ("live", false),
            ("version", true),
            ("published_version", true),
            ("updated_at", true),
            ("entry_count", true),
        ],
    ),
    (
        "form_publications",
        &[
            ("form_id", false),
            ("version", true),
            ("definition", false),
            ("created_at", true),
        ],
    ),
    (
        "form_entries",
        &[
            ("id", false),
            ("form_id", false),
            ("request_key", false),
            ("request_hash", false),
            ("form_version", true),
            ("values_json", false),
            ("created_at", true),
        ],
    ),
    (
        "audience_contacts",
        &[
            ("id", false),
            ("email", false),
            ("name", false),
            ("attributes", false),
            ("version", true),
            ("suppressed", true),
            ("created_at", true),
        ],
    ),
    (
        "audience_lists",
        &[
            ("id", false),
            ("title", false),
            ("purpose", false),
            ("policy", false),
            ("created_at", true),
        ],
    ),
    (
        "audience_memberships",
        &[
            ("contact_id", false),
            ("list_id", false),
            ("state", false),
            ("policy", false),
            ("nonce_hash", false),
            ("withdraw_hash", false),
            ("expires_at", true),
            ("confirmed_at", true),
            ("created_at", true),
        ],
    ),
    (
        "audience_consent_events",
        &[
            ("id", false),
            ("contact_id", false),
            ("list_id", false),
            ("action", false),
            ("policy", false),
            ("purpose", false),
            ("created_at", true),
        ],
    ),
    (
        "mail_jobs",
        &[
            ("id", false),
            ("dedupe", false),
            ("contact_id", false),
            ("list_id", false),
            ("kind", false),
            ("recipient", false),
            ("sender", false),
            ("subject", false),
            ("html", false),
            ("plain", false),
            ("message_id", false),
            ("state", false),
            ("attempts", true),
            ("next_at", true),
            ("lease_owner", false),
            ("lease_until", true),
            ("last_code", false),
            ("created_at", true),
        ],
    ),
    (
        "mail_attempts",
        &[
            ("id", false),
            ("job_id", false),
            ("outcome", false),
            ("created_at", true),
        ],
    ),
    (
        "business_campaigns",
        &[
            ("id", false),
            ("title", false),
            ("subject", false),
            ("document", false),
            ("segment", false),
            ("trigger_kind", false),
            ("list_id", false),
            ("state", false),
            ("version", true),
            ("send_at", true),
            ("cutoff", true),
            ("cursor", false),
            ("created_at", true),
        ],
    ),
    (
        "audience_withdrawal_tokens",
        &[
            ("hash", false),
            ("contact_id", false),
            ("list_id", false),
            ("created_at", true),
        ],
    ),
    (
        "form_drafts",
        &[
            ("form_id", false),
            ("token_hash", false),
            ("form_version", true),
            ("revision", true),
            ("values_json", false),
            ("expires_at", true),
        ],
    ),
    (
        "form_upload_usage",
        &[("form_id", false), ("bytes", true), ("files", true)],
    ),
    (
        "form_attachments",
        &[
            ("id", false),
            ("form_id", false),
            ("field_name", false),
            ("form_version", true),
            ("token_hash", false),
            ("filename", false),
            ("original_name", false),
            ("mime", false),
            ("size", true),
            ("sha256", false),
            ("entry_id", false),
            ("expires_at", true),
            ("created_at", true),
        ],
    ),
    (
        "form_entry_workflows",
        &[
            ("entry_id", false),
            ("notes", false),
            ("assignee", false),
            ("version", true),
            ("updated_at", true),
        ],
    ),
    (
        "form_entry_search",
        &[("entry_id", false), ("search_text", false)],
    ),
    ("business_secrets", &[("id", false), ("value", false)]),
    (
        "audience_suppressions",
        &[("hash", false), ("suppressed", true), ("created_at", true)],
    ),
    (
        "registration_requests",
        &[
            ("id", false),
            ("entry_id", false),
            ("email", false),
            ("name", false),
            ("token_hash", false),
            ("password_hash", false),
            ("state", false),
            ("expires_at", true),
            ("version", true),
            ("created_at", true),
        ],
    ),
    (
        "form_contributions",
        &[("entry_id", false), ("post_id", false)],
    ),
    (
        "business_usage",
        &[("kind", false), ("items", true), ("bytes", true)],
    ),
    (
        "engagement_settings",
        &[
            ("id", true),
            ("enabled", true),
            ("recording", true),
            ("purpose", false),
            ("version", true),
        ],
    ),
    (
        "engagement_sessions",
        &[
            ("hash", false),
            ("policy", true),
            ("purpose", false),
            ("recording", true),
            ("events", true),
            ("frames", true),
            ("expires_at", true),
            ("created_at", true),
        ],
    ),
    (
        "engagement_event_names",
        &[("name", false), ("label", false)],
    ),
    (
        "engagement_events",
        &[
            ("id", false),
            ("session_hash", false),
            ("path", false),
            ("name", false),
            ("dimensions", false),
            ("frame", false),
            ("created_at", true),
        ],
    ),
    (
        "engagement_usage",
        &[("id", true), ("events", true), ("sessions", true)],
    ),
    (
        "business_promotions",
        &[
            ("id", false),
            ("title", false),
            ("document_a", false),
            ("document_b", false),
            ("experiment", true),
            ("wheel", true),
            ("target", false),
            ("active", true),
            ("version", true),
            ("created_at", true),
        ],
    ),
    (
        "promotion_rewards",
        &[
            ("id", false),
            ("promotion_id", false),
            ("label", false),
            ("weight", true),
            ("remaining", true),
            ("issued", true),
        ],
    ),
    (
        "promotion_impressions",
        &[
            ("promotion_id", false),
            ("session_hash", false),
            ("variant", false),
            ("path", false),
            ("count", true),
            ("last_at", true),
        ],
    ),
    (
        "promotion_claims",
        &[
            ("id", false),
            ("promotion_id", false),
            ("session_hash", false),
            ("reward_id", false),
            ("label", false),
            ("code", false),
            ("created_at", true),
        ],
    ),
    (
        "engagement_dimension_values",
        &[("name", false), ("value", false)],
    ),
    (
        "member_groups",
        &[
            ("id", false),
            ("title", false),
            ("manager_id", false),
            ("seat_limit", true),
            ("version", true),
        ],
    ),
    (
        "member_policies",
        &[
            ("id", false),
            ("title", false),
            ("entitlement", false),
            ("group_id", false),
            ("enabled", true),
            ("version", true),
        ],
    ),
    (
        "member_group_users",
        &[
            ("group_id", false),
            ("user_id", false),
            ("created_at", true),
        ],
    ),
    (
        "member_grants",
        &[
            ("id", false),
            ("user_id", false),
            ("entitlement", false),
            ("starts_at", true),
            ("expires_at", true),
            ("revoked", true),
            ("origin", false),
            ("version", true),
            ("created_at", true),
        ],
    ),
    (
        "member_resources",
        &[
            ("kind", false),
            ("resource_id", false),
            ("policy_id", false),
            ("opens_at", true),
            ("delay_seconds", true),
            ("course_id", false),
            ("lesson_id", false),
        ],
    ),
    (
        "member_profiles",
        &[("user_id", false), ("biography", false), ("version", true)],
    ),
    (
        "member_courses",
        &[
            ("id", false),
            ("title", false),
            ("published_title", false),
            ("policy_id", false),
            ("draft", false),
            ("live", false),
            ("version", true),
            ("published_version", true),
            ("created_at", true),
        ],
    ),
    (
        "member_course_versions",
        &[
            ("course_id", false),
            ("version", true),
            ("definition", false),
            ("created_at", true),
        ],
    ),
    (
        "member_progress",
        &[
            ("course_id", false),
            ("course_version", true),
            ("lesson_id", false),
            ("user_id", false),
            ("attempts", true),
            ("best_score", true),
            ("completed_at", true),
        ],
    ),
    (
        "member_attempts",
        &[
            ("id", false),
            ("user_id", false),
            ("course_id", false),
            ("course_version", true),
            ("lesson_id", false),
            ("request_key", false),
            ("score", true),
            ("passed", true),
            ("created_at", true),
        ],
    ),
    (
        "member_assignments",
        &[
            ("id", false),
            ("course_id", false),
            ("course_version", true),
            ("lesson_id", false),
            ("lesson_title", false),
            ("user_id", false),
            ("body", false),
            ("state", false),
            ("feedback", false),
            ("version", true),
            ("created_at", true),
        ],
    ),
    (
        "member_certificates",
        &[
            ("id", false),
            ("user_id", false),
            ("course_id", false),
            ("course_version", true),
            ("issued_at", true),
            ("revoked", true),
        ],
    ),
    (
        "member_discussions",
        &[
            ("id", false),
            ("group_id", false),
            ("user_id", false),
            ("body", false),
            ("state", false),
            ("created_at", true),
        ],
    ),
    (
        "member_gifts",
        &[
            ("id", false),
            ("token_hash", false),
            ("entitlement", false),
            ("expires_at", true),
            ("duration_seconds", true),
            ("claimed_by", false),
            ("created_at", true),
        ],
    ),
    (
        "member_referrals",
        &[
            ("id", false),
            ("user_id", false),
            ("title", false),
            ("visits", true),
            ("created_at", true),
        ],
    ),
    (
        "member_commissions",
        &[
            ("id", false),
            ("referral_id", false),
            ("reference", false),
            ("amount_minor", true),
            ("currency", false),
            ("state", false),
            ("created_at", true),
        ],
    ),
    (
        "member_identities",
        &[("issuer", false), ("subject", false), ("user_id", false)],
    ),
    (
        "shop_settings",
        &[
            ("id", true),
            ("currency", false),
            ("tax_bps", true),
            ("shipping_minor", true),
            ("tax_shipping", true),
            ("version", true),
        ],
    ),
    (
        "shop_products",
        &[
            ("id", false),
            ("slug", false),
            ("title", false),
            ("description", false),
            ("kind", false),
            ("entitlement", false),
            ("access_seconds", true),
            ("download_id", false),
            ("published", true),
            ("version", true),
            ("created_at", true),
        ],
    ),
    (
        "shop_variants",
        &[
            ("id", false),
            ("product_id", false),
            ("title", false),
            ("sku", false),
            ("price_minor", true),
            ("member_price_minor", true),
            ("member_key", false),
            ("stock_total", true),
            ("held", true),
            ("sold", true),
            ("billing_interval", false),
            ("active", true),
            ("version", true),
        ],
    ),
    (
        "shop_discounts",
        &[
            ("code", false),
            ("title", false),
            ("bps", true),
            ("starts_at", true),
            ("expires_at", true),
            ("max_uses", true),
            ("held", true),
            ("used", true),
            ("member_key", false),
            ("product_id", false),
            ("reward_id", false),
            ("active", true),
            ("version", true),
        ],
    ),
    (
        "shop_carts",
        &[("user_id", false), ("version", true), ("updated_at", true)],
    ),
    (
        "shop_cart_lines",
        &[
            ("user_id", false),
            ("variant_id", false),
            ("slot_id", false),
            ("quantity", true),
        ],
    ),
    (
        "shop_resources",
        &[
            ("id", false),
            ("title", false),
            ("staff_id", false),
            ("active", true),
            ("version", true),
            ("created_at", true),
        ],
    ),
    (
        "shop_slots",
        &[
            ("id", false),
            ("resource_id", false),
            ("variant_id", false),
            ("starts_at", true),
            ("ends_at", true),
            ("capacity", true),
            ("held", true),
            ("booked", true),
            ("active", true),
            ("version", true),
        ],
    ),
    (
        "shop_subscriptions",
        &[
            ("id", false),
            ("user_id", false),
            ("variant_id", false),
            ("entitlement", false),
            ("price_minor", true),
            ("billing_interval", false),
            ("period_start", true),
            ("period_end", true),
            ("state", false),
            ("provider", false),
            ("provider_ref", false),
            ("provider_cancel_pending", true),
            ("provider_cancel_at", true),
            ("grant_id", false),
            ("next_variant", false),
            ("next_price_minor", true),
            ("version", true),
            ("created_at", true),
        ],
    ),
    (
        "shop_orders",
        &[
            ("id", false),
            ("user_id", false),
            ("request_key", false),
            ("request_digest", false),
            ("cart_version", true),
            ("customer_name", false),
            ("customer_email", false),
            ("shipping_address", false),
            ("currency", false),
            ("subtotal_minor", true),
            ("discount_minor", true),
            ("discount_bps", true),
            ("discount_base_minor", true),
            ("tax_minor", true),
            ("shipping_minor", true),
            ("total_minor", true),
            ("tax_bps", true),
            ("tax_shipping", true),
            ("discount_code", false),
            ("reward_claim", false),
            ("referral_id", false),
            ("commission_bps", true),
            ("provider", false),
            ("provider_ref", false),
            ("payment_ref", false),
            ("payment_state", false),
            ("fulfillment", false),
            ("paid_minor", true),
            ("refunded_minor", true),
            ("expires_at", true),
            ("subscription_id", false),
            ("period_start", true),
            ("period_end", true),
            ("target_price_minor", true),
            ("purpose", false),
            ("version", true),
            ("created_at", true),
        ],
    ),
    (
        "shop_order_lines",
        &[
            ("id", false),
            ("order_id", false),
            ("variant_id", false),
            ("product_id", false),
            ("title", false),
            ("sku", false),
            ("kind", false),
            ("quantity", true),
            ("unit_minor", true),
            ("line_minor", true),
            ("slot_id", false),
            ("entitlement", false),
            ("access_seconds", true),
            ("allocation", false),
            ("grant_id", false),
            ("billing_interval", false),
        ],
    ),
    (
        "shop_payments",
        &[
            ("id", false),
            ("order_id", false),
            ("provider", false),
            ("reference", false),
            ("amount_minor", true),
            ("currency", false),
            ("created_at", true),
        ],
    ),
    (
        "shop_refunds",
        &[
            ("id", false),
            ("order_id", false),
            ("request_key", false),
            ("amount_minor", true),
            ("reason", false),
            ("restock", true),
            ("state", false),
            ("provider_ref", false),
            ("created_at", true),
        ],
    ),
    (
        "shop_notifications",
        &[
            ("id", false),
            ("order_id", false),
            ("kind", false),
            ("slot_id", false),
            ("due_at", true),
            ("state", false),
            ("message_id", false),
            ("created_at", true),
        ],
    ),
    (
        "shop_provider_events",
        &[
            ("id", false),
            ("digest", false),
            ("body", false),
            ("state", false),
            ("attempts", true),
            ("next_at", true),
            ("created_at", true),
        ],
    ),
    (
        "shop_reward_redemptions",
        &[("claim_id", false), ("order_id", false), ("state", false)],
    ),
    (
        "shop_payouts",
        &[
            ("id", false),
            ("user_id", false),
            ("amount_minor", true),
            ("currency", false),
            ("reference", false),
            ("created_at", true),
        ],
    ),
    (
        "shop_history",
        &[
            ("id", false),
            ("order_id", false),
            ("actor", false),
            ("action", false),
            ("amount_minor", true),
            ("created_at", true),
        ],
    ),
];
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema: i64,
    created_at: i64,
    tables: BTreeMap<String, Vec<BTreeMap<String, Value>>>,
    files: Vec<MediaFile>,
    private_files: Vec<MediaFile>,
    audit_history: Vec<crate::operations::audit::Event>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MediaFile {
    filename: String,
    data: Vec<u8>,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    sha256: String,
    payload: String,
}

pub fn safe_filename(name: &str) -> bool {
    let Some((id, ext)) = name.rsplit_once('.') else {
        return false;
    };
    uuid::Uuid::parse_str(id).is_ok()
        && ["png", "jpg", "webp", "gif"].contains(&ext)
        && !name.contains('/')
        && !name.contains('\\')
}
pub async fn capture(app: &App) -> Result<Vec<u8>> {
    let _guard = app.mutation().await;
    let mut tx = app.db.pool.begin().await?;
    if app.db.postgres {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    let mut tables = BTreeMap::new();
    let mut budget = 0_usize;
    for (name, columns) in TABLES {
        let sql = format!(
            "SELECT {} FROM {}",
            columns
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join(","),
            name
        );
        let mut rows = sqlx::query(&sql).fetch(&mut *tx);
        let mut records = Vec::new();
        while let Some(row) = rows.try_next().await? {
            let mut record = BTreeMap::new();
            for (column, number) in *columns {
                let value = if *number {
                    Value::from(row.get::<i64, _>(*column))
                } else {
                    Value::from(row.get::<String, _>(*column))
                };
                budget += value.to_string().len();
                if budget > app.config.max_backup_bytes / 2 {
                    return Err(Error::invalid("Backup exceeds the configured size limit."));
                }
                record.insert((*column).into(), value);
            }
            records.push(record);
        }
        tables.insert((*name).into(), records);
    }
    let mut files = Vec::new();
    for row in tables.get("media").expect("media table collected") {
        let filename = row["filename"]
            .as_str()
            .ok_or(Error::invalid("Invalid media metadata."))?;
        if !safe_filename(filename) {
            return Err(Error::invalid("Unsafe stored media filename."));
        }
        let remaining = (app.config.max_backup_bytes / 2).saturating_sub(budget) / 5;
        let data = read_bounded(
            &app.config.data_dir.join("media").join(filename),
            remaining.min(32 * 1024 * 1024),
        )
        .await?;
        budget += data.len() * 5;
        if budget > app.config.max_backup_bytes / 2 {
            return Err(Error::invalid("Backup exceeds the configured size limit."));
        }
        let hash = digest(&data);
        if row["sha256"].as_str() != Some(hash.as_str()) {
            return Err(Error::invalid("Stored media failed its integrity check."));
        }
        files.push(MediaFile {
            filename: filename.into(),
            data,
            sha256: hash,
        });
    }
    tx.commit().await?;
    let mut private_files = Vec::new();
    for row in &tables["form_attachments"] {
        let filename = row["filename"].as_str().unwrap();
        if !crate::business::attachments::safe_filename(filename) {
            return Err(Error::invalid("Unsafe private attachment path."));
        }
        let path = app.config.data_dir.join("attachments").join(filename);
        if tokio::fs::metadata(&path).await?.len() > 2 * 1024 * 1024 {
            return Err(Error::invalid(
                "Private attachment exceeds its maximum file size.",
            ));
        }
        let remaining = app.config.max_backup_bytes.saturating_sub(budget) / 5;
        let data = read_bounded(&path, remaining.min(2 * 1024 * 1024)).await?;
        budget += data.len() * 5;
        if budget > app.config.max_backup_bytes
            || data.len() as i64 != row["size"].as_i64().unwrap()
            || digest(&data) != row["sha256"].as_str().unwrap()
        {
            return Err(Error::invalid(
                "Private attachment integrity or backup budget failed.",
            ));
        }
        private_files.push(MediaFile {
            filename: filename.into(),
            sha256: digest(&data),
            data,
        });
    }
    let audit_history = crate::operations::audit::read(app).await?;
    let snapshot = Snapshot {
        audit_history,
        private_files,
        schema: 10,
        created_at: crate::now(),
        tables,
        files,
    };
    let payload = serde_json::to_string(&snapshot)
        .map_err(|_| Error::invalid("Backup serialization failed."))?;
    let encoded = serde_json::to_vec(&Envelope {
        format: "wpalt-backup-v10".into(),
        sha256: digest(payload.as_bytes()),
        payload,
    })
    .map_err(|_| Error::invalid("Backup serialization failed."))?;
    if encoded.len() > app.config.max_backup_bytes {
        return Err(Error::invalid("Backup exceeds the configured size limit."));
    }
    tracing::info!(event = "backup_created", bytes = encoded.len());
    Ok(encoded)
}
fn validate(config: &crate::config::Config, encoded: &[u8]) -> Result<Snapshot> {
    if encoded.len() > config.max_backup_bytes {
        return Err(Error::invalid("Backup exceeds the configured size limit."));
    }
    let envelope: Envelope =
        serde_json::from_slice(encoded).map_err(|_| Error::invalid("Invalid backup envelope."))?;
    if envelope.format != "wpalt-backup-v10"
        || digest(envelope.payload.as_bytes()) != envelope.sha256
    {
        return Err(Error::invalid("Backup checksum or format is invalid."));
    }
    let snapshot: Snapshot = serde_json::from_str(&envelope.payload)
        .map_err(|_| Error::invalid("Invalid backup payload."))?;
    if snapshot.schema != 10
        || snapshot.tables.len() != TABLES.len()
        || TABLES
            .iter()
            .any(|(name, _)| !snapshot.tables.contains_key(*name))
    {
        return Err(Error::invalid("Unsupported backup schema or table set."));
    }
    if snapshot.audit_history.len() > 200
        || snapshot
            .audit_history
            .iter()
            .any(|event| !crate::operations::audit::valid(event))
    {
        return Err(Error::invalid("Invalid archived audit history."));
    }
    // Validate every row before any writes; no archive SQL or paths are executed.
    for (name, columns) in TABLES {
        for row in &snapshot.tables[*name] {
            if row.len() != columns.len()
                || columns.iter().any(|(column, number)| {
                    !row.get(*column).is_some_and(|v| {
                        if *number {
                            v.as_i64().is_some()
                        } else {
                            v.is_string()
                        }
                    })
                })
            {
                return Err(Error::invalid("Backup row has an invalid shape."));
            }
        }
    }
    let user_ids: HashSet<&str> = snapshot.tables["users"]
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    let mut passkey_ids = HashSet::new();
    let mut passkey_counts = BTreeMap::<&str, usize>::new();
    for row in &snapshot.tables["user_passkeys"] {
        let user = row["user_id"].as_str().unwrap();
        let id = row["credential_id"].as_str().unwrap();
        let raw = row["definition"].as_str().unwrap();
        if raw.len() > 64 * 1024
            || !user_ids.contains(user)
            || !passkey_ids.insert(id)
            || row["version"].as_i64().unwrap() < 1
        {
            return Err(Error::invalid("Invalid passkey graph."));
        }
        let key: webauthn_rs::prelude::Passkey =
            serde_json::from_str(raw).map_err(|_| Error::invalid("Invalid passkey definition."))?;
        if hex::encode(key.cred_id().as_ref()) != id {
            return Err(Error::invalid("Passkey identity mismatch."));
        }
        let count = passkey_counts.entry(user).or_default();
        *count += 1;
        if *count > 8 {
            return Err(Error::invalid("Account passkey budget exceeded."));
        }
    }
    let mut factor_users = HashSet::new();
    for row in &snapshot.tables["user_factors"] {
        let user = row["user_id"].as_str().unwrap();
        let secret = row["secret"].as_str().unwrap();
        let pending = row["pending"].as_str().unwrap();
        let valid_secret = |value: &str| {
            value.is_empty() || (value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        };
        let recovery: Vec<String> = serde_json::from_str(row["recovery"].as_str().unwrap())
            .map_err(|_| Error::invalid("Invalid backup recovery codes."))?;
        if !user_ids.contains(user)
            || !factor_users.insert(user)
            || !valid_secret(secret)
            || !valid_secret(pending)
            || row["last_step"].as_i64().unwrap() < -1
            || row["pending_until"].as_i64().unwrap() < 0
            || recovery.len() > 8
            || recovery
                .iter()
                .any(|h| h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()))
            || recovery.iter().collect::<HashSet<_>>().len() != recovery.len()
        {
            return Err(Error::invalid("Invalid authenticator recovery graph."));
        }
    }
    for row in &snapshot.tables["users"] {
        crate::auth::valid_user(
            row["email"].as_str().unwrap(),
            row["name"].as_str().unwrap(),
            row["role"].as_str().unwrap(),
        )?;
        let email = row["email"].as_str().unwrap();
        if email != email.to_ascii_lowercase() {
            return Err(Error::invalid(
                "Backup account emails must use canonical lowercase addresses.",
            ));
        }
        if !crate::auth::supported_password_hash(row["password_hash"].as_str().unwrap()) {
            return Err(Error::invalid(
                "Backup contains an unsupported or excessive-cost password hash.",
            ));
        }
    }
    if !snapshot.tables["users"]
        .iter()
        .any(|row| row["role"] == "admin")
    {
        return Err(Error::invalid(
            "Backup must preserve at least one active administrator.",
        ));
    }
    for row in &snapshot.tables["posts"] {
        for key in ["document", "published_document"] {
            crate::document::Document::parse(
                row[key]
                    .as_str()
                    .ok_or(Error::invalid("Missing document"))?,
            )?;
        }
    }
    for row in &snapshot.tables["revisions"] {
        let value: Value = serde_json::from_str(row["snapshot"].as_str().unwrap())
            .map_err(|_| Error::invalid("Invalid revision"))?;
        for key in ["document", "published_document"] {
            crate::document::Document::parse(
                value["post"][key]
                    .as_str()
                    .ok_or(Error::invalid("Missing revision document"))?,
            )?;
        }
    }
    let discovery_rows = &snapshot.tables["discovery_settings"];
    if discovery_rows.len() != 1
        || discovery_rows[0]["id"] != 1
        || discovery_rows[0]["version"].as_i64().is_none_or(|v| v < 1)
    {
        return Err(Error::invalid(
            "Backup must contain one versioned discovery definition.",
        ));
    }
    let definition: crate::discovery::Definition = serde_json::from_str(
        discovery_rows[0]["definition"]
            .as_str()
            .ok_or(Error::invalid("Invalid discovery definition."))?,
    )
    .map_err(|_| Error::invalid("Invalid discovery definition."))?;
    definition.validate()?;
    let mut groups = BTreeMap::new();
    let mut public_paths = HashSet::from(["/".to_owned(), "/search".to_owned()]);
    for language in &definition.languages {
        public_paths.insert(format!("/{}", language.code));
        public_paths.insert(definition.path(&language.code, ""));
        public_paths.insert(definition.path(&language.code, "search"));
    }
    for row in &snapshot.tables["posts"] {
        for prefix in ["", "published_"] {
            let text = |key: &str| {
                row[&format!("{prefix}{key}")]
                    .as_str()
                    .ok_or(Error::invalid("Invalid discovery content metadata."))
            };
            let locale = text("locale")?;
            let group = text("translation_group")?;
            definition
                .language(locale)
                .map_err(|_| Error::invalid("Content uses an unconfigured language."))?;
            crate::discovery::Seo::parse(text("seo")?)?;
            if group.len() > 80 || (!group.is_empty() && !crate::schema::identifier(group)) {
                return Err(Error::invalid("Invalid translation group."));
            }
            if !group.is_empty() {
                let kind = row["kind"]
                    .as_str()
                    .ok_or(Error::invalid("Invalid content type."))?;
                if groups
                    .insert(group, kind)
                    .is_some_and(|previous| previous != kind)
                {
                    return Err(Error::invalid("Translation group mixes content types."));
                }
            }
        }
        if row["status"] == "published" || row["status"] == "scheduled" {
            let prefix = if row["status"] == "published" {
                "published_"
            } else {
                ""
            };
            let locale = row[&format!("{prefix}locale")].as_str().unwrap();
            let slug = row[&format!("{prefix}slug")]
                .as_str()
                .ok_or(Error::invalid("Invalid published slug."))?;
            if !crate::content::valid_slug(slug) {
                return Err(Error::invalid(
                    "Content conflicts with a reserved or invalid route.",
                ));
            }
            if definition.languages.iter().any(|l| l.code == slug) {
                return Err(Error::invalid("Content conflicts with a language route."));
            }
            public_paths.insert(definition.path(locale, slug));
            public_paths.insert(format!("/{slug}"));
        }
    }
    let mut redirects = BTreeMap::new();
    for row in &snapshot.tables["redirects"] {
        let source = row["source"]
            .as_str()
            .ok_or(Error::invalid("Invalid redirect source."))?;
        let target = row["target"]
            .as_str()
            .ok_or(Error::invalid("Invalid redirect target."))?;
        if !matches!(row["code"].as_i64(), Some(301 | 302))
            || row["version"].as_i64().is_none_or(|v| v < 1)
            || public_paths.contains(source)
            || redirects
                .insert(source.to_owned(), target.to_owned())
                .is_some()
        {
            return Err(Error::invalid("Invalid or conflicting redirect rule."));
        }
    }
    crate::discovery::validate_redirect_graph(&redirects)?;
    let settings = snapshot.tables["settings"]
        .first()
        .ok_or(Error::invalid("Backup has no site settings."))?;
    if snapshot.tables["settings"].len() != 1 {
        return Err(Error::invalid("Backup must contain one site."));
    }
    let setting_string = |key: &str| {
        settings
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or(Error::invalid("Invalid backup settings."))
    };
    crate::content::validate_settings(&crate::model::Settings {
        business_enabled: config.business_enabled,
        membership_enabled: config.membership_enabled,
        commerce_enabled: config.commerce.enabled,
        engagement_available: config.engagement.enabled,
        analytics: None,
        title: setting_string("title")?,
        description: setting_string("description")?,
        theme: setting_string("theme")?,
        navigation: setting_string("navigation")?,
        field_schema: setting_string("field_schema")?,
    })?;
    let mut expected = BTreeMap::new();
    for row in &snapshot.tables["media"] {
        let name = row
            .get("filename")
            .and_then(Value::as_str)
            .ok_or(Error::invalid("Invalid media metadata."))?;
        let hash = row
            .get("sha256")
            .and_then(Value::as_str)
            .ok_or(Error::invalid("Invalid media metadata."))?;
        let size = row
            .get("size")
            .and_then(Value::as_i64)
            .ok_or(Error::invalid("Invalid media metadata."))?;
        if !safe_filename(name) || expected.insert(name, (hash, size)).is_some() {
            return Err(Error::invalid("Unsafe or duplicate media filename."));
        }
    }
    if expected.len() != snapshot.files.len() {
        return Err(Error::invalid("Backup media is incomplete."));
    }
    let mut seen = HashSet::new();
    for file in &snapshot.files {
        let Some((hash, size)) = expected.get(file.filename.as_str()) else {
            return Err(Error::invalid("Unexpected media file."));
        };
        if !seen.insert(&file.filename)
            || !safe_filename(&file.filename)
            || file.data.len() > config.max_upload_bytes
            || digest(&file.data) != file.sha256
            || file.sha256 != *hash
            || file.data.len() as i64 != *size
        {
            return Err(Error::invalid("Backup media failed validation."));
        }
    }
    let expected_private: BTreeMap<_, _> = snapshot.tables["form_attachments"]
        .iter()
        .map(|row| {
            (
                row["filename"].as_str().unwrap(),
                (
                    row["sha256"].as_str().unwrap(),
                    row["size"].as_i64().unwrap(),
                ),
            )
        })
        .collect();
    if expected_private.len() != snapshot.tables["form_attachments"].len()
        || expected_private.len() != snapshot.private_files.len()
    {
        return Err(Error::invalid("Private attachment backup is incomplete."));
    }
    let mut seen = HashSet::new();
    for file in &snapshot.private_files {
        if !crate::business::attachments::safe_filename(&file.filename)
            || !seen.insert(&file.filename)
            || file.data.len() > 2 * 1024 * 1024
            || digest(&file.data) != file.sha256
            || expected_private.get(file.filename.as_str())
                != Some(&(file.sha256.as_str(), file.data.len() as i64))
        {
            return Err(Error::invalid("Invalid private attachment backup."));
        }
    }
    let registry = crate::schema::Registry {
        common: serde_json::from_str(&setting_string("field_schema")?)
            .map_err(|_| Error::invalid("Invalid backup field definitions."))?,
        models: snapshot.tables["content_models"]
            .iter()
            .map(|row| {
                let id = row["id"]
                    .as_str()
                    .ok_or(Error::invalid("Invalid model identifier."))?;
                let definition = serde_json::from_str(
                    row["definition"]
                        .as_str()
                        .ok_or(Error::invalid("Invalid model definition."))?,
                )
                .map_err(|_| Error::invalid("Invalid model definition."))?;
                Ok((id.to_owned(), definition))
            })
            .collect::<Result<_>>()?,
    };
    registry.validate()?;
    for row in &snapshot.tables["business_forms"] {
        let draft: crate::business::forms::FormDefinition =
            serde_json::from_str(row["draft"].as_str().unwrap())
                .map_err(|_| Error::invalid("Invalid backup form."))?;
        draft.validate(&registry.common)?;
        if row["published_version"].as_i64().unwrap() > 0 {
            let live: crate::business::store::PublishedForm =
                serde_json::from_str(row["live"].as_str().unwrap())
                    .map_err(|_| Error::invalid("Invalid backup published form."))?;
            live.form.validate(&live.common())?;
        }
    }
    let mut publications = BTreeMap::new();
    for row in &snapshot.tables["form_publications"] {
        let published: crate::business::store::PublishedForm =
            serde_json::from_str(row["definition"].as_str().unwrap())
                .map_err(|_| Error::invalid("Invalid backup publication."))?;
        published.form.validate(&published.common())?;
        let identity = (
            row["form_id"].as_str().unwrap(),
            row["version"].as_i64().unwrap(),
        );
        if publications.insert(identity, published).is_some() {
            return Err(Error::invalid("Duplicate form publication."));
        }
    }
    for row in &snapshot.tables["business_forms"] {
        let version = row["published_version"].as_i64().unwrap();
        if version > 0 {
            let live = publications
                .get(&(row["id"].as_str().unwrap(), version))
                .ok_or(Error::invalid("Backup form lacks its live publication."))?;
            let snapshot: Value = serde_json::from_str(row["live"].as_str().unwrap())
                .map_err(|_| Error::invalid("Invalid live form snapshot."))?;
            if serde_json::to_value(live)
                .map_err(|_| Error::invalid("Invalid live form snapshot."))?
                != snapshot
            {
                return Err(Error::invalid("Backup live form and publication disagree."));
            }
        }
    }
    for row in &snapshot.tables["form_entries"] {
        let identity = (
            row["form_id"].as_str().unwrap(),
            row["form_version"].as_i64().unwrap(),
        );
        let live = publications
            .get(&identity)
            .ok_or(Error::invalid("Backup entry lacks its publication."))?;
        let values: Value = serde_json::from_str(row["values_json"].as_str().unwrap())
            .map_err(|_| Error::invalid("Invalid backup entry values."))?;
        if live.form.evaluate(&live.common(), &values, false)? != values {
            return Err(Error::invalid("Backup entry has non-authoritative values."));
        }
    }
    let mut relationships = BTreeMap::new();
    let mut references = Vec::new();
    for row in &snapshot.tables["posts"] {
        let fields = registry.fields_for(row["kind"].as_str().unwrap())?;
        for column in ["fields", "published_fields"] {
            if column == "published_fields" && row["published_slug"].as_str().unwrap().is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(row[column].as_str().unwrap())
                .map_err(|_| Error::invalid("Invalid backup structured values."))?;
            registry.validate_values(&fields, &value)?;
            registry.references(&fields, &value, &mut relationships, &mut references)?;
        }
    }
    if snapshot.tables["site_design"].len() != 1 {
        return Err(Error::invalid("Backup needs one design state."));
    }
    for row in &snapshot.tables["site_design"] {
        for column in ["draft_options", "live_options"] {
            let value: Value = serde_json::from_str(row[column].as_str().unwrap())
                .map_err(|_| Error::invalid("Invalid option values."))?;
            registry.validate_values(&registry.common.options, &value)?;
            registry.references(
                &registry.common.options,
                &value,
                &mut relationships,
                &mut references,
            )?;
        }
    }
    crate::business::backup_validation::validate(&snapshot.tables)?;
    crate::membership::backup::validate(&snapshot.tables)?;
    crate::commerce::backup::validate(&snapshot.tables, &config.commerce)?;
    let post_kinds: BTreeMap<_, _> = snapshot.tables["posts"]
        .iter()
        .map(|row| (row["id"].as_str().unwrap(), row["kind"].as_str().unwrap()))
        .collect();
    let media_ids: HashSet<_> = snapshot.tables["media"]
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    if relationships
        .iter()
        .any(|(id, kind)| post_kinds.get(id.as_str()) != Some(&kind.as_str()))
        || references.iter().any(|id| !media_ids.contains(id.as_str()))
    {
        return Err(Error::invalid(
            "Backup has missing or mistyped structured references.",
        ));
    }
    let active = setting_string("theme")?;
    let mut active_found = false;
    if snapshot.tables["themes"].len() > 32 {
        return Err(Error::invalid("Backup has too many themes."));
    }
    for row in &snapshot.tables["themes"] {
        if !crate::schema::identifier(row["id"].as_str().unwrap()) {
            return Err(Error::invalid("Invalid backup theme identifier."));
        }
        for column in ["draft", "live"] {
            let raw = row[column].as_str().unwrap();
            if !raw.is_empty() {
                crate::theme::Package::parse(raw, &registry)?;
            }
        }
        let version = row["version"].as_i64().unwrap();
        let published = row["published_version"].as_i64().unwrap();
        if version < 1 || published < 0 || published > version {
            return Err(Error::invalid("Backup has invalid theme versions."));
        }
        if published > 0
            && !snapshot.tables["theme_revisions"].iter().any(|history| {
                history["theme_id"] == row["id"]
                    && history["version"].as_i64() == Some(published)
                    && history["published"].as_i64() == Some(1)
                    && history["package"] == row["live"]
            })
        {
            return Err(Error::invalid(
                "Backup publication history does not match its live theme.",
            ));
        }
        if row["id"].as_str() == Some(&active)
            && row["published_version"].as_i64().unwrap() > 0
            && !row["live"].as_str().unwrap().is_empty()
        {
            active_found = true;
        }
    }
    if !active_found {
        return Err(Error::invalid("Backup lacks its active published theme."));
    }
    // Old revisions can retain removed fields; validate executable/style grammar using
    // the current registry before exposing any historical publication stylesheet.
    for row in &snapshot.tables["theme_revisions"] {
        crate::theme::Package::parse_historical(row["package"].as_str().unwrap(), &registry)?;
    }
    Ok(snapshot)
}

pub async fn restore(app: &App, encoded: &[u8]) -> Result<()> {
    let snapshot = validate(&app.config, encoded)?;
    let _guard = app.mutation().await;
    let mut tx = app.db.pool.begin().await?;
    for (name, _) in TABLES {
        let count: i64 = if *name == "discovery_settings" {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM discovery_settings WHERE version<>1 OR definition<>$1",
            )
            .bind(serde_json::to_string(&crate::discovery::Definition::default()).unwrap())
            .fetch_one(&mut *tx)
            .await?
        } else if *name == "shop_settings" {
            sqlx::query_scalar("SELECT COUNT(*) FROM shop_settings WHERE tax_bps<>0 OR shipping_minor<>0 OR tax_shipping<>0 OR version<>1").fetch_one(&mut *tx).await?
        } else if *name == "business_usage" {
            sqlx::query_scalar("SELECT COUNT(*) FROM business_usage WHERE items<>0 OR bytes<>0")
                .fetch_one(&mut *tx)
                .await?
        } else if *name == "engagement_settings" {
            sqlx::query_scalar("SELECT COUNT(*) FROM engagement_settings WHERE enabled<>0 OR recording<>0 OR version<>1 OR purpose<>'Understand and improve this site using local interaction data.'").fetch_one(&mut *tx).await?
        } else if *name == "engagement_dimension_values" {
            sqlx::query_scalar("SELECT COUNT(*) FROM engagement_dimension_values WHERE NOT ((name='device' AND value IN ('mobile','desktop')) OR (name='referrer' AND value IN ('direct','same_site','external')))").fetch_one(&mut *tx).await?
        } else if *name == "engagement_usage" {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM engagement_usage WHERE events<>0 OR sessions<>0",
            )
            .fetch_one(&mut *tx)
            .await?
        } else if *name == "engagement_event_names" {
            sqlx::query_scalar("SELECT COUNT(*) FROM engagement_event_names WHERE NOT ((name='pageview' AND label='Page view') OR (name='form_submit' AND label='Accepted form response') OR (name='offer_claim' AND label='Offer claimed') OR (name='interaction' AND label='Masked interaction'))").fetch_one(&mut *tx).await?
        } else {
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {name}"))
                .fetch_one(&mut *tx)
                .await?
        };
        if count != 0 {
            return Err(Error::invalid(
                "Restore requires an empty target. Back up and use a fresh database/data directory.",
            ));
        }
    }
    sqlx::query("DELETE FROM discovery_settings")
        .execute(&mut *tx)
        .await?;
    sqlx::raw_sql("DELETE FROM shop_settings; DELETE FROM business_usage; DELETE FROM engagement_settings; DELETE FROM engagement_usage; DELETE FROM engagement_event_names; DELETE FROM engagement_dimension_values;").execute(&mut *tx).await?;
    for (name, columns) in TABLES
        .iter()
        .filter(|(name, _)| *name != "user_factors" && *name != "user_passkeys")
        .chain(
            TABLES
                .iter()
                .filter(|(name, _)| *name == "user_factors" || *name == "user_passkeys"),
        )
    {
        for row in &snapshot.tables[*name] {
            let mut q = QueryBuilder::<Any>::new(format!(
                "INSERT INTO {name}({}) VALUES(",
                columns
                    .iter()
                    .map(|(n, _)| *n)
                    .collect::<Vec<_>>()
                    .join(",")
            ));
            let mut values = q.separated(",");
            for (column, number) in *columns {
                if *number {
                    values.push_bind(row[*column].as_i64().unwrap());
                } else {
                    values.push_bind(row[*column].as_str().unwrap());
                }
            }
            values.push_unseparated(")");
            let mut query = q.build();
            let sql = crate::db::Db::numbered(query.sql());
            let args = query
                .take_arguments()
                .map_err(sqlx::Error::Encode)?
                .unwrap_or_default();
            sqlx::query_with(&sql, args).execute(&mut *tx).await?;
        }
    }
    // Restored leases cannot belong to a live worker. SMTP acceptance may have
    // happened before capture; keep it uncertain. Local spools are reproducible.
    sqlx::query("UPDATE mail_jobs SET state='uncertain',lease_owner='',lease_until=0,last_code='restored_lease' WHERE state='leased'").execute(&mut *tx).await?;
    sqlx::query("UPDATE mail_jobs SET state='pending',next_at=$1 WHERE state='spooled'")
        .bind(crate::now())
        .execute(&mut *tx)
        .await?;
    // Files precede commit: interruption cannot commit a site whose files are missing.
    // A failed fresh restore may leave orphan files; a retry overwrites only validated UUID paths.
    for file in snapshot.files {
        tokio::fs::write(
            app.config.data_dir.join("media").join(file.filename),
            file.data,
        )
        .await?;
    }
    for file in snapshot.private_files {
        let path = app.config.data_dir.join("attachments").join(file.filename);
        tokio::fs::write(&path, file.data).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).await?;
        }
    }
    // Preserve recent source history without overwriting the fresh host's own
    // restore intent. Imported records remain operational evidence, not a ledger.
    for event in snapshot.audit_history.into_iter().rev() {
        crate::operations::audit::append(app, event).await?;
    }
    tx.commit().await?;
    tracing::info!(event = "backup_restored");
    Ok(())
}
pub fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Inspect a complete current-format recovery graph without writing it.
pub fn inspect(config: &crate::config::Config, encoded: &[u8]) -> Result<serde_json::Value> {
    let snapshot = validate(config, encoded)?;
    Ok(
        serde_json::json!({"schema":snapshot.schema,"created_at":snapshot.created_at,"tables":snapshot.tables.iter().map(|(name,rows)|(name.clone(),rows.len())).collect::<BTreeMap<_,_>>(),"media_files":snapshot.files.len(),"private_files":snapshot.private_files.len()}),
    )
}
/// Selective file recovery to a NEW private owner-selected path. Does not inject
/// rows into a live site or broaden the original protected-resource graph.
pub fn extract_file(
    config: &crate::config::Config,
    encoded: &[u8],
    id: &str,
    output: &Path,
) -> Result<()> {
    uuid::Uuid::parse_str(id).map_err(|_| Error::invalid("Use a media or attachment UUID."))?;
    let snapshot = validate(config, encoded)?;
    let filename = snapshot.tables["media"]
        .iter()
        .chain(snapshot.tables["form_attachments"].iter())
        .find(|r| r["id"].as_str() == Some(id))
        .and_then(|r| r["filename"].as_str())
        .ok_or_else(Error::not_found)?;
    let file = snapshot
        .files
        .iter()
        .chain(snapshot.private_files.iter())
        .find(|f| f.filename == filename)
        .ok_or_else(Error::not_found)?;
    write_private(output, &file.data)
        .map_err(|_| Error::invalid("Cannot create a new private recovered file."))?;
    Ok(())
}
/// One-off pre-adoption archive migration, outside ordinary request/restore code.
pub fn migrate_m6(config: &crate::config::Config, encoded: &[u8]) -> Result<Vec<u8>> {
    if encoded.len() > config.max_backup_bytes {
        return Err(Error::invalid("Backup exceeds configured budget."));
    }
    let envelope: Envelope =
        serde_json::from_slice(encoded).map_err(|_| Error::invalid("Invalid M6 archive."))?;
    if envelope.format != "wpalt-backup-v8"
        || digest(envelope.payload.as_bytes()) != envelope.sha256
    {
        return Err(Error::invalid("Invalid M6 archive format/checksum."));
    }
    // A dedicated one-off converter parses byte vectors directly, avoiding a
    // Value tree with an allocation per archived byte. Ordinary restore has no legacy parser.
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct M6 {
        schema: i64,
        created_at: i64,
        tables: BTreeMap<String, Vec<BTreeMap<String, Value>>>,
        files: Vec<MediaFile>,
        private_files: Vec<MediaFile>,
    }
    let old: M6 = serde_json::from_str(&envelope.payload)
        .map_err(|_| Error::invalid("Invalid M6 archive graph."))?;
    let mut snapshot = Snapshot {
        schema: old.schema,
        created_at: old.created_at,
        tables: old.tables,
        files: old.files,
        private_files: old.private_files,
        audit_history: vec![],
    };
    if snapshot.schema != 9
        || snapshot.tables.contains_key("user_factors")
        || snapshot.tables.contains_key("user_passkeys")
    {
        return Err(Error::invalid(
            "Archive is not an unmigrated M6 recovery point.",
        ));
    }
    snapshot.schema = 10;
    snapshot.tables.insert("user_factors".into(), vec![]);
    snapshot.tables.insert("user_passkeys".into(), vec![]);
    let payload = serde_json::to_string(&snapshot)
        .map_err(|_| Error::invalid("Archive migration failed."))?;
    let output = serde_json::to_vec(&Envelope {
        format: "wpalt-backup-v10".into(),
        sha256: digest(payload.as_bytes()),
        payload,
    })
    .map_err(|_| Error::invalid("Archive migration failed."))?;
    validate(config, &output)?;
    Ok(output)
}
