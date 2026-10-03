//! Validate business archive semantics before importing any rows or private files.
use crate::error::{Error, Result};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
type Tables = BTreeMap<String, Vec<BTreeMap<String, Value>>>;
fn text<'a>(row: &'a BTreeMap<String, Value>, key: &str) -> &'a str {
    row[key].as_str().unwrap()
}
fn number(row: &BTreeMap<String, Value>, key: &str) -> i64 {
    row[key].as_i64().unwrap()
}
fn hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok()
}
pub fn validate(tables: &Tables) -> Result<()> {
    let fail = || Error::invalid("Business backup contains invalid or inconsistent state.");
    let contacts: HashSet<_> = tables["audience_contacts"]
        .iter()
        .map(|r| text(r, "id"))
        .collect();
    let lists: HashSet<_> = tables["audience_lists"]
        .iter()
        .map(|r| text(r, "id"))
        .collect();
    let users: HashSet<_> = tables["users"].iter().map(|r| text(r, "id")).collect();
    for row in &tables["audience_contacts"] {
        let attrs: super::audience::Attributes =
            serde_json::from_str(text(row, "attributes")).map_err(|_| fail())?;
        attrs.validate()?;
        if !id(text(row, "id"))
            || super::mail::email(text(row, "email"))? != text(row, "email")
            || text(row, "name").len() > 100
            || number(row, "version") < 1
            || ![0, 1].contains(&number(row, "suppressed"))
        {
            return Err(fail());
        }
    }
    for row in &tables["audience_lists"] {
        if !id(text(row, "id"))
            || text(row, "title").trim().is_empty()
            || text(row, "title").len() > 160
            || text(row, "purpose").trim().is_empty()
            || text(row, "purpose").len() > 1000
            || text(row, "policy").is_empty()
            || text(row, "policy").len() > 64
        {
            return Err(fail());
        }
    }
    for row in &tables["audience_memberships"] {
        if !contacts.contains(text(row, "contact_id"))
            || !lists.contains(text(row, "list_id"))
            || !["pending", "confirmed", "withdrawn"].contains(&text(row, "state"))
            || !hash(text(row, "nonce_hash"))
            || !hash(text(row, "withdraw_hash"))
            || number(row, "expires_at") < 0
            || number(row, "confirmed_at") < 0
        {
            return Err(fail());
        }
    }
    for row in &tables["business_secrets"] {
        if text(row, "id") != "audience_suppression" || !hash(text(row, "value")) {
            return Err(fail());
        }
    }
    if !tables["audience_suppressions"].is_empty() && tables["business_secrets"].len() != 1 {
        return Err(fail());
    }
    for row in &tables["audience_suppressions"] {
        if !hash(text(row, "hash")) || ![0, 1].contains(&number(row, "suppressed")) {
            return Err(fail());
        }
    }
    for row in &tables["business_campaigns"] {
        crate::document::Document::parse(text(row, "document"))?;
        let segment: super::audience::Segment =
            serde_json::from_str(text(row, "segment")).map_err(|_| fail())?;
        segment.validate()?;
        if !id(text(row, "id"))
            || !lists.contains(text(row, "list_id"))
            || text(row, "subject").is_empty()
            || text(row, "subject").len() > 200
            || text(row, "subject").contains(['\r', '\n'])
            || !["draft", "scheduled", "expanding", "complete", "cancelled"]
                .contains(&text(row, "state"))
            || number(row, "version") < 1
            || !["", "confirmation"].contains(&text(row, "trigger_kind"))
            || number(row, "send_at") < 0
        {
            return Err(fail());
        }
    }
    for row in &tables["mail_jobs"] {
        super::mail::email(text(row, "recipient"))?;
        super::mail::email(text(row, "sender"))?;
        if !id(text(row, "id"))
            || text(row, "dedupe").len() > 200
            || text(row, "subject").len() > 200
            || text(row, "subject").contains(['\r', '\n'])
            || text(row, "html").len() > 2 * 1024 * 1024
            || text(row, "plain").len() > 512 * 1024
            || text(row, "message_id").len() > 300
            || text(row, "message_id").contains(['\r', '\n'])
            || !["confirmation", "campaign", "notification"].contains(&text(row, "kind"))
            || ![
                "pending",
                "leased",
                "retry",
                "spooled",
                "sent",
                "uncertain",
                "dead",
                "cancelled",
            ]
            .contains(&text(row, "state"))
            || !(0..=10).contains(&number(row, "attempts"))
            || number(row, "created_at") < 0
            || number(row, "created_at") > crate::now() + 365 * 86400
            || (!text(row, "contact_id").is_empty() && !contacts.contains(text(row, "contact_id")))
        {
            return Err(fail());
        }
    }
    let entries: HashSet<_> = tables["form_entries"]
        .iter()
        .map(|r| text(r, "id"))
        .collect();
    let mut usage = BTreeMap::<&str, (i64, i64)>::new();
    for row in &tables["form_attachments"] {
        if !id(text(row, "id"))
            || !super::attachments::safe_filename(text(row, "filename"))
            || !hash(text(row, "token_hash"))
            || !hash(text(row, "sha256"))
            || !(1..=2 * 1024 * 1024).contains(&number(row, "size"))
            || !["text/plain", "application/pdf", "image/png"].contains(&text(row, "mime"))
            || (!text(row, "entry_id").is_empty() && !entries.contains(text(row, "entry_id")))
        {
            return Err(fail());
        }
        let entry = usage.entry(text(row, "form_id")).or_default();
        entry.0 += number(row, "size");
        entry.1 += 1;
    }
    for row in &tables["form_upload_usage"] {
        if usage.get(text(row, "form_id")).copied().unwrap_or_default()
            != (number(row, "bytes"), number(row, "files"))
        {
            return Err(fail());
        }
    }
    for row in &tables["form_entry_workflows"] {
        if !entries.contains(text(row, "entry_id"))
            || text(row, "notes").len() > 8000
            || number(row, "version") < 1
            || (!text(row, "assignee").is_empty() && !users.contains(text(row, "assignee")))
        {
            return Err(fail());
        }
    }
    if tables["engagement_settings"].len() != 1 || tables["engagement_usage"].len() != 1 {
        return Err(fail());
    }
    for row in &tables["engagement_settings"] {
        if number(row, "id") != 1
            || ![0, 1].contains(&number(row, "enabled"))
            || ![0, 1].contains(&number(row, "recording"))
            || number(row, "version") < 1
            || text(row, "purpose").trim().is_empty()
            || text(row, "purpose").len() > 1000
        {
            return Err(fail());
        }
    }
    for row in &tables["engagement_sessions"] {
        if !hash(text(row, "hash"))
            || number(row, "policy") < 1
            || ![0, 1].contains(&number(row, "recording"))
            || !(0..=500).contains(&number(row, "events"))
            || !(0..=60).contains(&number(row, "frames"))
            || text(row, "purpose").len() > 1000
        {
            return Err(fail());
        }
    }
    let mut session_counts: BTreeMap<&str, (i64, i64)> = BTreeMap::new();
    for row in &tables["engagement_events"] {
        let counts = session_counts.entry(text(row, "session_hash")).or_default();
        counts.0 += 1;
        counts.1 += i64::from(!text(row, "frame").is_empty());
        let dimensions: BTreeMap<String, String> =
            serde_json::from_str(text(row, "dimensions")).map_err(|_| fail())?;
        if !id(text(row, "id"))
            || !hash(text(row, "session_hash"))
            || text(row, "path").len() > 200
            || text(row, "path").contains(['?', '#'])
            || dimensions.len() > 4
            || dimensions.iter().any(|(name, value)| {
                !tables["engagement_dimension_values"]
                    .iter()
                    .any(|r| text(r, "name") == name && text(r, "value") == value)
            })
        {
            return Err(fail());
        }
        if !text(row, "frame").is_empty() {
            let frame: super::engagement::Frame =
                serde_json::from_str(text(row, "frame")).map_err(|_| fail())?;
            frame.validate()?;
        }
    }
    for row in &tables["engagement_sessions"] {
        if session_counts.remove(text(row, "hash")).unwrap_or_default()
            != (number(row, "events"), number(row, "frames"))
        {
            return Err(fail());
        }
    }
    if !session_counts.is_empty() {
        return Err(fail());
    }
    let usage = &tables["engagement_usage"][0];
    if number(usage, "events") != tables["engagement_events"].len() as i64
        || number(usage, "sessions") != tables["engagement_sessions"].len() as i64
    {
        return Err(fail());
    }
    let promotions: HashSet<_> = tables["business_promotions"]
        .iter()
        .map(|r| text(r, "id"))
        .collect();
    let rewards: BTreeMap<_, _> = tables["promotion_rewards"]
        .iter()
        .map(|r| (text(r, "id"), text(r, "promotion_id")))
        .collect();
    let sessions: HashSet<_> = tables["engagement_sessions"]
        .iter()
        .map(|r| text(r, "hash"))
        .collect();
    if promotions.len() > 100 {
        return Err(fail());
    }
    for r in &tables["business_promotions"] {
        let target: super::promotions::Target =
            serde_json::from_str(text(r, "target")).map_err(|_| fail())?;
        target.validate()?;
        crate::document::Document::parse(text(r, "document_a"))?;
        crate::document::Document::parse(text(r, "document_b"))?;
        if !id(text(r, "id"))
            || text(r, "title").is_empty()
            || text(r, "title").len() > 160
            || number(r, "version") < 1
            || ["active", "experiment", "wheel"]
                .iter()
                .any(|k| ![0, 1].contains(&number(r, k)))
        {
            return Err(fail());
        }
    }
    for r in &tables["promotion_rewards"] {
        if !id(text(r, "id"))
            || !promotions.contains(text(r, "promotion_id"))
            || text(r, "label").is_empty()
            || text(r, "label").len() > 160
            || !(1..=10000).contains(&number(r, "weight"))
            || !(0..=1000000).contains(&number(r, "remaining"))
            || !(0..=1000000).contains(&number(r, "issued"))
            || number(r, "remaining") + number(r, "issued") > 1000000
        {
            return Err(fail());
        }
    }
    for r in &tables["promotion_impressions"] {
        if !promotions.contains(text(r, "promotion_id"))
            || !sessions.contains(text(r, "session_hash"))
            || !["a", "b"].contains(&text(r, "variant"))
            || !(1..=10).contains(&number(r, "count"))
            || text(r, "path").len() > 200
            || !text(r, "path").starts_with('/')
            || text(r, "path").contains(['?', '#', '\\'])
        {
            return Err(fail());
        }
    }
    for r in &tables["promotion_claims"] {
        if !id(text(r, "id"))
            || !promotions.contains(text(r, "promotion_id"))
            || !sessions.contains(text(r, "session_hash"))
            || rewards.get(text(r, "reward_id")).copied() != Some(text(r, "promotion_id"))
            || text(r, "label").len() > 160
            || text(r, "code").len() > 64
        {
            return Err(fail());
        }
    }
    for r in &tables["registration_requests"] {
        super::mail::email(text(r, "email"))?;
        if !id(text(r, "id"))
            || !id(text(r, "entry_id"))
            || !hash(text(r, "token_hash"))
            || text(r, "name").is_empty()
            || text(r, "name").len() > 100
            || number(r, "version") < 1
            || !["pending", "verified", "approved", "rejected"].contains(&text(r, "state"))
        {
            return Err(fail());
        }
        if text(r, "state") == "verified" {
            if !crate::auth::supported_password_hash(text(r, "password_hash")) {
                return Err(fail());
            }
        } else if !text(r, "password_hash").is_empty() {
            return Err(fail());
        }
    }
    if tables["business_usage"].len() != 7 {
        return Err(fail());
    }
    for (kind, table, columns) in [
        ("forms", "business_forms", vec![]),
        ("entries", "form_entries", vec!["values_json"]),
        ("contacts", "audience_contacts", vec![]),
        ("drafts", "form_drafts", vec!["values_json"]),
        ("uploads", "form_attachments", vec![]),
        ("registrations", "registration_requests", vec![]),
        ("mail", "mail_jobs", vec!["html", "plain", "subject"]),
    ] {
        let counters = tables["business_usage"]
            .iter()
            .filter(|r| text(r, "kind") == kind)
            .collect::<Vec<_>>();
        if counters.len() != 1 {
            return Err(fail());
        }
        let bytes: i64 = if kind == "uploads" {
            tables[table].iter().map(|r| number(r, "size")).sum()
        } else {
            tables[table]
                .iter()
                .map(|r| columns.iter().map(|k| text(r, k).len() as i64).sum::<i64>())
                .sum()
        };
        if number(counters[0], "items") != tables[table].len() as i64
            || number(counters[0], "bytes") != bytes
        {
            return Err(fail());
        }
    }
    Ok(())
}
