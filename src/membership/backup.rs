//! Validate the whole learning graph before fresh recovery writes.
use super::{Course, uuid};
use crate::error::{Error, Result};
use serde_json::Value;
type Map<K, V> = BTreeMap<K, V>;
use std::collections::{BTreeMap, HashSet};
type Tables = BTreeMap<String, Vec<Map<String, Value>>>;
fn text<'a>(r: &'a Map<String, Value>, key: &str) -> &'a str {
    r[key].as_str().unwrap()
}
fn number(r: &Map<String, Value>, key: &str) -> i64 {
    r[key].as_i64().unwrap()
}
fn bad() -> Error {
    Error::invalid("Backup contains an invalid membership or learning graph.")
}
pub fn validate(t: &Tables) -> Result<()> {
    let users: HashSet<&str> = t["users"].iter().map(|r| text(r, "id")).collect();
    let policies: HashSet<&str> = t["member_policies"].iter().map(|r| text(r, "id")).collect();
    let groups: HashSet<&str> = t["member_groups"].iter().map(|r| text(r, "id")).collect();
    let posts: HashSet<&str> = t["posts"].iter().map(|r| text(r, "id")).collect();
    let media: HashSet<&str> = t["media"].iter().map(|r| text(r, "id")).collect();
    let courses: HashSet<&str> = t["member_courses"].iter().map(|r| text(r, "id")).collect();
    for name in [
        "member_policies",
        "member_groups",
        "member_grants",
        "member_courses",
        "member_assignments",
        "member_certificates",
        "member_discussions",
        "member_gifts",
        "member_referrals",
        "member_commissions",
        "member_attempts",
    ] {
        let mut ids = HashSet::new();
        for r in &t[name] {
            uuid(text(r, "id"))?;
            if !ids.insert(text(r, "id")) {
                return Err(bad());
            }
        }
    }
    for r in &t["member_groups"] {
        if text(r, "title").is_empty()
            || text(r, "title").len() > 160
            || !(0..=1000).contains(&number(r, "seat_limit"))
            || (!text(r, "manager_id").is_empty() && !users.contains(text(r, "manager_id")))
        {
            return Err(bad());
        }
    }
    for r in &t["member_policies"] {
        if text(r, "title").is_empty()
            || text(r, "title").len() > 160
            || number(r, "version") < 1
            || !(0..=1).contains(&number(r, "enabled"))
            || (!text(r, "group_id").is_empty() && !groups.contains(text(r, "group_id")))
            || text(r, "entitlement").len() > 80
            || (text(r, "entitlement").is_empty() && text(r, "group_id").is_empty())
        {
            return Err(bad());
        }
    }
    for r in &t["member_grants"] {
        if !users.contains(text(r, "user_id"))
            || text(r, "entitlement").is_empty()
            || text(r, "entitlement").len() > 80
            || number(r, "starts_at") < 0
            || (number(r, "expires_at") != 0 && number(r, "expires_at") <= number(r, "starts_at"))
            || !(0..=1).contains(&number(r, "revoked"))
            || number(r, "version") < 1
            || text(r, "origin").len() > 160
        {
            return Err(bad());
        }
    }
    let mut group_pairs = HashSet::new();
    for r in &t["member_group_users"] {
        if !groups.contains(text(r, "group_id"))
            || !users.contains(text(r, "user_id"))
            || number(r, "created_at") < 0
            || !group_pairs.insert((text(r, "group_id"), text(r, "user_id")))
        {
            return Err(bad());
        }
    }
    for r in &t["member_groups"] {
        let limit = number(r, "seat_limit");
        if group_pairs
            .iter()
            .filter(|(g, _)| *g == text(r, "id"))
            .count() as i64
            > if limit == 0 { 1000 } else { limit }
        {
            return Err(bad());
        }
    }
    let mut versions = BTreeMap::new();
    for r in &t["member_course_versions"] {
        let c: Course = serde_json::from_str(text(r, "definition")).map_err(|_| bad())?;
        c.validate()?;
        if !courses.contains(text(r, "course_id"))
            || !policies.contains(c.policy_id.as_str())
            || number(r, "version") < 1
            || versions
                .insert((text(r, "course_id"), number(r, "version")), c)
                .is_some()
        {
            return Err(bad());
        }
    }
    for r in &t["member_courses"] {
        let draft: Course = serde_json::from_str(text(r, "draft")).map_err(|_| bad())?;
        draft.validate()?;
        if !policies.contains(draft.policy_id.as_str())
            || !policies.contains(text(r, "policy_id"))
            || number(r, "version") < 1
            || number(r, "published_version") > number(r, "version")
            || number(r, "published_version") < 0
        {
            return Err(bad());
        }
        if number(r, "published_version") > 0 {
            let live: Course = serde_json::from_str(text(r, "live")).map_err(|_| bad())?;
            live.validate()?;
            let historical = versions
                .get(&(text(r, "id"), number(r, "published_version")))
                .ok_or_else(bad)?;
            if serde_json::to_value(historical).map_err(|_| bad())?
                != serde_json::to_value(&live).map_err(|_| bad())?
                || live.title != text(r, "published_title")
            {
                return Err(bad());
            }
            for l in &live.lessons {
                if !posts.contains(l.post_id.as_str()) {
                    return Err(bad());
                }
            }
        }
    }
    let mut resources = HashSet::new();
    for r in &t["member_resources"] {
        let kind = text(r, "kind");
        let id = text(r, "resource_id");
        if !policies.contains(text(r, "policy_id"))
            || number(r, "opens_at") < 0
            || !(0..=31536000).contains(&number(r, "delay_seconds"))
            || !resources.insert((kind, id))
        {
            return Err(bad());
        }
        let exists = match kind {
            "post" => posts.contains(id),
            "media" => media.contains(id),
            "course" => courses.contains(id),
            _ => false,
        };
        if !exists {
            return Err(bad());
        }
        let course = text(r, "course_id");
        if !course.is_empty() && (!courses.contains(course) || !["post", "media"].contains(&kind)) {
            return Err(bad());
        }
    }
    // A live course must retain every policy and lesson resource, so recovery cannot remove gates.
    for r in &t["member_courses"] {
        if number(r, "published_version") > 0 {
            let c = versions
                .get(&(text(r, "id"), number(r, "published_version")))
                .ok_or_else(bad)?;
            let rule = t["member_resources"]
                .iter()
                .find(|x| text(x, "kind") == "course" && text(x, "resource_id") == text(r, "id"))
                .ok_or_else(bad)?;
            if text(rule, "policy_id") != c.policy_id {
                return Err(bad());
            }
            for l in &c.lessons {
                let rule = t["member_resources"]
                    .iter()
                    .find(|x| text(x, "kind") == "post" && text(x, "resource_id") == l.post_id)
                    .ok_or_else(bad)?;
                if text(rule, "policy_id") != c.policy_id
                    || text(rule, "course_id") != text(r, "id")
                    || text(rule, "lesson_id") != l.id
                    || number(rule, "opens_at") != l.opens_at
                    || number(rule, "delay_seconds") != l.delay_seconds
                {
                    return Err(bad());
                }
            }
            for l in &c.lessons {
                for download in &l.downloads {
                    let rule = t["member_resources"]
                        .iter()
                        .find(|r| text(r, "kind") == "media" && text(r, "resource_id") == download)
                        .ok_or_else(bad)?;
                    if !media.contains(download.as_str())
                        || text(rule, "course_id") != text(r, "id")
                        || text(rule, "lesson_id") != l.id
                        || text(rule, "policy_id") != c.policy_id
                        || number(rule, "opens_at") != l.opens_at
                        || number(rule, "delay_seconds") != l.delay_seconds
                    {
                        return Err(bad());
                    }
                }
            }
        }
    }
    let mut progress = BTreeMap::new();
    for r in &t["member_progress"] {
        let c = versions
            .get(&(text(r, "course_id"), number(r, "course_version")))
            .ok_or_else(bad)?;
        let l = c
            .lessons
            .iter()
            .find(|l| l.id == text(r, "lesson_id"))
            .ok_or_else(bad)?;
        if !users.contains(text(r, "user_id"))
            || !(0..=l.max_attempts).contains(&number(r, "attempts"))
            || !(0..=100).contains(&number(r, "best_score"))
            || number(r, "completed_at") < 0
            || (number(r, "completed_at") > 0 && number(r, "best_score") < l.pass_percent)
            || progress
                .insert(
                    (
                        text(r, "course_id"),
                        number(r, "course_version"),
                        text(r, "lesson_id"),
                        text(r, "user_id"),
                    ),
                    r,
                )
                .is_some()
        {
            return Err(bad());
        }
    }
    for name in ["member_attempts", "member_assignments"] {
        for r in &t[name] {
            let c = versions
                .get(&(text(r, "course_id"), number(r, "course_version")))
                .ok_or_else(bad)?;
            if !users.contains(text(r, "user_id"))
                || !c.lessons.iter().any(|l| l.id == text(r, "lesson_id"))
            {
                return Err(bad());
            }
            if name == "member_attempts" {
                uuid(text(r, "request_key"))?;
                if !(0..=100).contains(&number(r, "score"))
                    || !(0..=1).contains(&number(r, "passed"))
                {
                    return Err(bad());
                }
            } else if text(r, "body").len() > 16000
                || text(r, "feedback").len() > 4000
                || !["submitted", "approved", "changes"].contains(&text(r, "state"))
                || number(r, "version") < 1
            {
                return Err(bad());
            }
        }
    }
    let mut active_certificates = HashSet::new();
    for r in &t["member_certificates"] {
        let c = versions
            .get(&(text(r, "course_id"), number(r, "course_version")))
            .ok_or_else(bad)?;
        if !users.contains(text(r, "user_id"))
            || (number(r, "revoked") == 0
                && !active_certificates.insert((
                    text(r, "user_id"),
                    text(r, "course_id"),
                    number(r, "course_version"),
                )))
            || !(0..=1).contains(&number(r, "revoked"))
            || (number(r, "revoked") == 0
                && !c.lessons.iter().all(|l| {
                    progress
                        .get(&(
                            text(r, "course_id"),
                            number(r, "course_version"),
                            l.id.as_str(),
                            text(r, "user_id"),
                        ))
                        .is_some_and(|p| number(p, "completed_at") > 0)
                }))
        {
            return Err(bad());
        }
    }
    for r in &t["member_profiles"] {
        if !users.contains(text(r, "user_id"))
            || text(r, "biography").len() > 2000
            || number(r, "version") < 1
        {
            return Err(bad());
        }
    }
    for r in &t["member_discussions"] {
        if !users.contains(text(r, "user_id"))
            || !groups.contains(text(r, "group_id"))
            || text(r, "body").is_empty()
            || text(r, "body").len() > 4000
            || !["pending", "approved", "rejected"].contains(&text(r, "state"))
        {
            return Err(bad());
        }
    }
    for r in &t["member_gifts"] {
        if text(r, "token_hash").len() != 64
            || !text(r, "token_hash").bytes().all(|b| b.is_ascii_hexdigit())
            || text(r, "entitlement").is_empty()
            || text(r, "entitlement").len() > 80
            || !(1..=31536000).contains(&number(r, "duration_seconds"))
            || (!text(r, "claimed_by").is_empty() && !users.contains(text(r, "claimed_by")))
        {
            return Err(bad());
        }
    }
    let referrals: HashSet<&str> = t["member_referrals"]
        .iter()
        .map(|r| text(r, "id"))
        .collect();
    for r in &t["member_referrals"] {
        if !users.contains(text(r, "user_id"))
            || text(r, "title").is_empty()
            || text(r, "title").len() > 160
        {
            return Err(bad());
        }
    }
    for r in &t["member_commissions"] {
        if !referrals.contains(text(r, "referral_id"))
            || number(r, "amount_minor") < 0
            || text(r, "currency").len() != 3
            || !text(r, "currency").bytes().all(|b| b.is_ascii_uppercase())
            || text(r, "reference").is_empty()
            || text(r, "reference").len() > 160
            || !["recorded", "void"].contains(&text(r, "state"))
        {
            return Err(bad());
        }
    }
    for r in &t["member_identities"] {
        let issuer = url::Url::parse(text(r, "issuer")).map_err(|_| bad())?;
        if issuer.scheme() != "https"
            || !users.contains(text(r, "user_id"))
            || text(r, "subject").is_empty()
            || text(r, "subject").len() > 255
        {
            return Err(bad());
        }
    }
    Ok(())
}
