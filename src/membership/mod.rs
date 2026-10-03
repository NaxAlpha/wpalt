//! Owner-controlled access and versioned learning; policy is evaluated from durable state.
pub mod backup;
pub mod identity;
pub mod referrals;
pub mod web;
use crate::{
    App,
    error::{Error, Result},
    model::Session,
    now,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashSet;

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS member_policies(id TEXT PRIMARY KEY,title TEXT NOT NULL,entitlement TEXT NOT NULL DEFAULT '',group_id TEXT NOT NULL DEFAULT '',enabled BIGINT NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),version BIGINT NOT NULL DEFAULT 1);
CREATE TABLE IF NOT EXISTS member_groups(id TEXT PRIMARY KEY,title TEXT NOT NULL,manager_id TEXT NOT NULL DEFAULT '',seat_limit BIGINT NOT NULL DEFAULT 0 CHECK(seat_limit>=0),version BIGINT NOT NULL DEFAULT 1);
CREATE TABLE IF NOT EXISTS member_group_users(group_id TEXT NOT NULL REFERENCES member_groups(id),user_id TEXT NOT NULL REFERENCES users(id),created_at BIGINT NOT NULL,PRIMARY KEY(group_id,user_id));
CREATE INDEX IF NOT EXISTS member_group_user_lookup ON member_group_users(user_id,group_id);
CREATE TABLE IF NOT EXISTS member_grants(id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),entitlement TEXT NOT NULL,starts_at BIGINT NOT NULL,expires_at BIGINT NOT NULL DEFAULT 0,revoked BIGINT NOT NULL DEFAULT 0 CHECK(revoked IN (0,1)),origin TEXT NOT NULL DEFAULT '',version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL, CHECK(expires_at=0 OR expires_at>starts_at));
CREATE INDEX IF NOT EXISTS member_grant_access ON member_grants(user_id,entitlement,revoked,starts_at,expires_at);
CREATE TABLE IF NOT EXISTS member_resources(kind TEXT NOT NULL CHECK(kind IN ('post','media','course')),resource_id TEXT NOT NULL,policy_id TEXT NOT NULL REFERENCES member_policies(id),opens_at BIGINT NOT NULL DEFAULT 0,delay_seconds BIGINT NOT NULL DEFAULT 0 CHECK(delay_seconds>=0),course_id TEXT NOT NULL DEFAULT '',lesson_id TEXT NOT NULL DEFAULT '',PRIMARY KEY(kind,resource_id));
CREATE INDEX IF NOT EXISTS member_resource_policy ON member_resources(policy_id,kind,resource_id);
CREATE TABLE IF NOT EXISTS member_profiles(user_id TEXT PRIMARY KEY REFERENCES users(id),biography TEXT NOT NULL DEFAULT '',version BIGINT NOT NULL DEFAULT 1);
CREATE TABLE IF NOT EXISTS member_courses(id TEXT PRIMARY KEY,title TEXT NOT NULL,published_title TEXT NOT NULL DEFAULT '',policy_id TEXT NOT NULL REFERENCES member_policies(id),draft TEXT NOT NULL,live TEXT NOT NULL DEFAULT '',version BIGINT NOT NULL DEFAULT 1,published_version BIGINT NOT NULL DEFAULT 0,created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS member_course_catalog ON member_courses(created_at DESC,id);
CREATE TABLE IF NOT EXISTS member_course_versions(course_id TEXT NOT NULL REFERENCES member_courses(id),version BIGINT NOT NULL,definition TEXT NOT NULL,created_at BIGINT NOT NULL,PRIMARY KEY(course_id,version));
CREATE TABLE IF NOT EXISTS member_progress(course_id TEXT NOT NULL REFERENCES member_courses(id),course_version BIGINT NOT NULL,lesson_id TEXT NOT NULL,user_id TEXT NOT NULL REFERENCES users(id),attempts BIGINT NOT NULL DEFAULT 0,best_score BIGINT NOT NULL DEFAULT 0,completed_at BIGINT NOT NULL DEFAULT 0,PRIMARY KEY(course_id,course_version,lesson_id,user_id));
CREATE INDEX IF NOT EXISTS member_progress_user ON member_progress(user_id,course_id,course_version);
CREATE TABLE IF NOT EXISTS member_attempts(id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),course_id TEXT NOT NULL REFERENCES member_courses(id),course_version BIGINT NOT NULL,lesson_id TEXT NOT NULL,request_key TEXT NOT NULL,score BIGINT NOT NULL,passed BIGINT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(user_id,course_id,course_version,lesson_id,request_key));
CREATE TABLE IF NOT EXISTS member_assignments(id TEXT PRIMARY KEY,course_id TEXT NOT NULL REFERENCES member_courses(id),course_version BIGINT NOT NULL,lesson_id TEXT NOT NULL,user_id TEXT NOT NULL REFERENCES users(id),body TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN ('submitted','approved','changes')),feedback TEXT NOT NULL DEFAULT '',version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,UNIQUE(course_id,course_version,lesson_id,user_id));
CREATE INDEX IF NOT EXISTS member_assignment_review ON member_assignments(state,created_at,id);
CREATE TABLE IF NOT EXISTS member_certificates(id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),course_id TEXT NOT NULL REFERENCES member_courses(id),course_version BIGINT NOT NULL,issued_at BIGINT NOT NULL,revoked BIGINT NOT NULL DEFAULT 0,UNIQUE(user_id,course_id,course_version));
CREATE TABLE IF NOT EXISTS member_discussions(id TEXT PRIMARY KEY,group_id TEXT NOT NULL REFERENCES member_groups(id),user_id TEXT NOT NULL REFERENCES users(id),body TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN ('pending','approved','rejected')),created_at BIGINT NOT NULL);
CREATE INDEX IF NOT EXISTS member_discussion_group ON member_discussions(group_id,state,created_at,id);
CREATE TABLE IF NOT EXISTS member_gifts(id TEXT PRIMARY KEY,token_hash TEXT NOT NULL UNIQUE,entitlement TEXT NOT NULL,expires_at BIGINT NOT NULL,duration_seconds BIGINT NOT NULL,claimed_by TEXT NOT NULL DEFAULT '',created_at BIGINT NOT NULL);
CREATE TABLE IF NOT EXISTS member_referrals(id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),title TEXT NOT NULL,visits BIGINT NOT NULL DEFAULT 0,created_at BIGINT NOT NULL);
CREATE TABLE IF NOT EXISTS member_commissions(id TEXT PRIMARY KEY,referral_id TEXT NOT NULL REFERENCES member_referrals(id),reference TEXT NOT NULL UNIQUE,amount_minor BIGINT NOT NULL CHECK(amount_minor>=0),currency TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN ('recorded','void')),created_at BIGINT NOT NULL);
"#;

pub fn uuid(id: &str) -> Result<()> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| Error::invalid("Use a valid record identifier."))
}
fn label(value: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > max {
        Err(Error::invalid("Check the title or text length."))
    } else {
        Ok(())
    }
}
pub async fn staff(app: &App, s: &Session) -> Result<()> {
    let role: Option<String> = sqlx::query_scalar("SELECT role FROM users WHERE id=$1")
        .bind(&s.user.id)
        .fetch_optional(&app.db.pool)
        .await?;
    if role.as_deref() != Some("admin") {
        return Err(Error::forbidden());
    }
    Ok(())
}
/// Public projections always exclude protected resources, independently of module enablement.
pub const PUBLIC_POST: &str = " AND NOT EXISTS(SELECT 1 FROM member_resources mr WHERE mr.kind='post' AND mr.resource_id=posts.id)";

pub async fn allowed(app: &App, kind: &str, id: &str, user: Option<&str>, at: i64) -> Result<bool> {
    let row=sqlx::query("SELECT r.policy_id,r.opens_at,r.delay_seconds,r.course_id,r.lesson_id,p.entitlement,p.group_id,p.enabled FROM member_resources r JOIN member_policies p ON p.id=r.policy_id WHERE r.kind=$1 AND r.resource_id=$2").bind(kind).bind(id).fetch_optional(&app.db.pool).await?;
    let Some(row) = row else { return Ok(true) };
    let Some(user) = user else { return Ok(false) };
    let role: Option<String> = sqlx::query_scalar("SELECT role FROM users WHERE id=$1")
        .bind(user)
        .fetch_optional(&app.db.pool)
        .await?;
    if matches!(role.as_deref(), Some("admin" | "editor")) {
        return Ok(true);
    }
    if !app.config.membership_enabled
        || role.is_none()
        || role.as_deref() == Some("disabled")
        || row.get::<i64, _>("enabled") != 1
        || at < row.get::<i64, _>("opens_at")
    {
        return Ok(false);
    }
    let entitlement: String = row.get("entitlement");
    let group: String = row.get("group_id");
    let starts: Option<i64> = if entitlement.is_empty() {
        Some(0)
    } else {
        sqlx::query_scalar("SELECT MIN(starts_at) FROM member_grants WHERE user_id=$1 AND entitlement=$2 AND revoked=0 AND starts_at<=$3 AND (expires_at=0 OR expires_at>$3)").bind(user).bind(entitlement).bind(at).fetch_one(&app.db.pool).await?
    };
    let Some(mut starts) = starts else {
        return Ok(false);
    };
    if !group.is_empty() {
        let joined: Option<i64> = sqlx::query_scalar(
            "SELECT created_at FROM member_group_users WHERE group_id=$1 AND user_id=$2",
        )
        .bind(group)
        .bind(user)
        .fetch_optional(&app.db.pool)
        .await?;
        let Some(joined) = joined else {
            return Ok(false);
        };
        starts = starts.max(joined);
    }
    if at < starts.saturating_add(row.get::<i64, _>("delay_seconds")) {
        return Ok(false);
    }
    let course: String = row.get("course_id");
    if !course.is_empty() {
        let (definition, version) = live(app, &course).await?;
        let lesson: String = row.get("lesson_id");
        let Some(position) = definition.lessons.iter().position(|l| l.id == lesson) else {
            return Ok(false);
        };
        if definition.sequential && position > 0 {
            let completed:Vec<String>=sqlx::query_scalar("SELECT lesson_id FROM member_progress WHERE user_id=$1 AND course_id=$2 AND course_version=$3 AND completed_at>0").bind(user).bind(course).bind(version).fetch_all(&app.db.pool).await?;
            if !definition.lessons[..position]
                .iter()
                .all(|l| completed.contains(&l.id))
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
pub async fn require(app: &App, kind: &str, id: &str, user: Option<&str>) -> Result<()> {
    if allowed(app, kind, id, user, now()).await? {
        Ok(())
    } else {
        Err(Error::forbidden())
    }
}

pub async fn policy(app: &App, title: &str, entitlement: &str, group: &str) -> Result<String> {
    label(title, 160)?;
    if entitlement.len() > 80
        || !entitlement
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(Error::invalid("Use a short entitlement key."));
    }
    if !group.is_empty() {
        uuid(group)?;
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM member_groups WHERE id=$1")
            .bind(group)
            .fetch_one(&app.db.pool)
            .await?;
        if n != 1 {
            return Err(Error::not_found());
        }
    }
    if entitlement.is_empty() && group.is_empty() {
        return Err(Error::invalid("A policy needs an entitlement or group."));
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO member_policies(id,title,entitlement,group_id) VALUES($1,$2,$3,$4)")
        .bind(&id)
        .bind(title.trim())
        .bind(entitlement)
        .bind(group)
        .execute(&app.db.pool)
        .await?;
    Ok(id)
}
pub async fn grant(
    app: &App,
    user: &str,
    key: &str,
    starts: i64,
    expires: i64,
    origin: &str,
) -> Result<String> {
    uuid(user)?;
    label(key, 80)?;
    if !key
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || starts < 0
        || (expires != 0 && expires <= starts)
        || origin.len() > 160
    {
        return Err(Error::invalid("Check entitlement dates and origin."));
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO member_grants(id,user_id,entitlement,starts_at,expires_at,origin,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(&id).bind(user).bind(key).bind(starts).bind(expires).bind(origin).bind(now()).execute(&app.db.pool).await?;
    tracing::info!(event="membership_granted",grant_id=%id);
    Ok(id)
}
pub async fn protect(
    app: &App,
    kind: &str,
    id: &str,
    policy: &str,
    opens: i64,
    delay: i64,
) -> Result<()> {
    uuid(id)?;
    uuid(policy)?;
    if !["post", "media"].contains(&kind) || opens < 0 || !(0..=31536000).contains(&delay) {
        return Err(Error::invalid("Check resource type and unlock dates."));
    }
    let _guard = app.mutations.lock().await;
    let table = if kind == "post" { "posts" } else { "media" };
    let exists: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE id=$1"))
        .bind(id)
        .fetch_one(&app.db.pool)
        .await?;
    if exists != 1 {
        return Err(Error::not_found());
    }
    let course: Option<String> = sqlx::query_scalar(
        "SELECT course_id FROM member_resources WHERE kind=$1 AND resource_id=$2",
    )
    .bind(kind)
    .bind(id)
    .fetch_optional(&app.db.pool)
    .await?;
    if course.is_some_and(|v| !v.is_empty()) {
        return Err(Error::invalid(
            "Edit the owning course to change a lesson policy.",
        ));
    }
    sqlx::query("INSERT INTO member_resources(kind,resource_id,policy_id,opens_at,delay_seconds) VALUES($1,$2,$3,$4,$5) ON CONFLICT(kind,resource_id) DO UPDATE SET policy_id=excluded.policy_id,opens_at=excluded.opens_at,delay_seconds=excluded.delay_seconds").bind(kind).bind(id).bind(policy).bind(opens).bind(delay).execute(&app.db.pool).await?;
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Question {
    pub prompt: String,
    pub choices: Vec<String>,
    pub correct: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lesson {
    pub id: String,
    pub title: String,
    pub post_id: String,
    #[serde(default)]
    pub delay_seconds: i64,
    #[serde(default)]
    pub opens_at: i64,
    #[serde(default)]
    pub questions: Vec<Question>,
    #[serde(default)]
    pub assignment: String,
    #[serde(default = "pass_default")]
    pub pass_percent: i64,
    #[serde(default = "attempt_default")]
    pub max_attempts: i64,
}
fn pass_default() -> i64 {
    70
}
fn attempt_default() -> i64 {
    3
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Course {
    pub title: String,
    pub policy_id: String,
    #[serde(default = "yes")]
    pub sequential: bool,
    pub lessons: Vec<Lesson>,
}
fn yes() -> bool {
    true
}
impl Course {
    pub fn validate(&self) -> Result<()> {
        label(&self.title, 160)?;
        uuid(&self.policy_id)?;
        if self.lessons.is_empty() || self.lessons.len() > 100 {
            return Err(Error::invalid("Use one to 100 lessons."));
        }
        let mut ids = HashSet::new();
        let mut posts = HashSet::new();
        for l in &self.lessons {
            uuid(&l.id)?;
            uuid(&l.post_id)?;
            label(&l.title, 160)?;
            if !ids.insert(&l.id)
                || !posts.insert(&l.post_id)
                || !(0..=31536000).contains(&l.delay_seconds)
                || l.opens_at < 0
                || l.questions.len() > 20
                || l.assignment.len() > 4000
                || !(1..=100).contains(&l.pass_percent)
                || !(1..=10).contains(&l.max_attempts)
            {
                return Err(Error::invalid(
                    "Check lesson identities, unlocks and assessment limits.",
                ));
            }
            for q in &l.questions {
                label(&q.prompt, 1000)?;
                if !(2..=8).contains(&q.choices.len()) || q.correct >= q.choices.len() {
                    return Err(Error::invalid(
                        "Use two to eight choices and a valid answer.",
                    ));
                }
                for choice in &q.choices {
                    label(choice, 500)?
                }
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| Error::invalid("Invalid course."))?
            .len()
            > 256 * 1024
        {
            return Err(Error::invalid("Course exceeds the authoring budget."));
        }
        Ok(())
    }
}
pub async fn create_course(app: &App, c: &Course) -> Result<String> {
    c.validate()?;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO member_courses(id,title,policy_id,draft,created_at) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(&id)
    .bind(&c.title)
    .bind(&c.policy_id)
    .bind(serde_json::to_string(c).map_err(|_| Error::invalid("Invalid course."))?)
    .bind(now())
    .execute(&app.db.pool)
    .await?;
    Ok(id)
}
pub async fn save_course(
    app: &App,
    id: &str,
    version: i64,
    c: &Course,
    publish: bool,
) -> Result<i64> {
    c.validate()?;
    let _guard = app.mutations.lock().await;
    let mut tx = app.db.pool.begin().await?;
    let stored=sqlx::query("UPDATE member_courses SET title=$1,policy_id=$2,draft=$3,version=version+1 WHERE id=$4 AND version=$5 RETURNING version").bind(&c.title).bind(&c.policy_id).bind(serde_json::to_string(c).map_err(|_|Error::invalid("Invalid course."))?).bind(id).bind(version).fetch_optional(&mut *tx).await?.ok_or_else(Error::conflict)?;
    let next: i64 = stored.get("version");
    if publish {
        for l in &c.lessons {
            let status: Option<String> = sqlx::query_scalar("SELECT status FROM posts WHERE id=$1")
                .bind(&l.post_id)
                .fetch_optional(&mut *tx)
                .await?;
            if status.as_deref() != Some("published") {
                return Err(Error::invalid(
                    "Publish every lesson's shared content first.",
                ));
            }
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT course_id FROM member_resources WHERE kind='post' AND resource_id=$1",
            )
            .bind(&l.post_id)
            .fetch_optional(&mut *tx)
            .await?;
            if existing.is_some_and(|owner| owner != id) {
                return Err(Error::invalid(
                    "A lesson publication belongs to another access rule or course.",
                ));
            }
            sqlx::query("INSERT INTO member_resources(kind,resource_id,policy_id,opens_at,delay_seconds,course_id,lesson_id) VALUES('post',$1,$2,$3,$4,$5,$6) ON CONFLICT(kind,resource_id) DO UPDATE SET policy_id=excluded.policy_id,opens_at=excluded.opens_at,delay_seconds=excluded.delay_seconds,course_id=excluded.course_id,lesson_id=excluded.lesson_id").bind(&l.post_id).bind(&c.policy_id).bind(l.opens_at).bind(l.delay_seconds).bind(id).bind(&l.id).execute(&mut *tx).await?;
        }
        // Removed lessons stay protected: release is a deliberate owner action, not a publication side effect.
        sqlx::query("INSERT INTO member_resources(kind,resource_id,policy_id) VALUES('course',$1,$2) ON CONFLICT(kind,resource_id) DO UPDATE SET policy_id=excluded.policy_id").bind(id).bind(&c.policy_id).execute(&mut *tx).await?;
        sqlx::query(
            "UPDATE member_courses SET live=$1,published_version=$2,published_title=$4 WHERE id=$3",
        )
        .bind(serde_json::to_string(c).map_err(|_| Error::invalid("Invalid course."))?)
        .bind(next)
        .bind(id)
        .bind(&c.title)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO member_course_versions(course_id,version,definition,created_at) VALUES($1,$2,$3,$4)").bind(id).bind(next).bind(serde_json::to_string(c).map_err(|_|Error::invalid("Invalid course."))?).bind(now()).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    tracing::info!(event="course_saved",course_id=%id,version=next,published=publish);
    Ok(next)
}
pub async fn live(app: &App, id: &str) -> Result<(Course, i64)> {
    let r = sqlx::query(
        "SELECT live,published_version FROM member_courses WHERE id=$1 AND published_version>0",
    )
    .bind(id)
    .fetch_optional(&app.db.pool)
    .await?
    .ok_or_else(Error::not_found)?;
    Ok((
        serde_json::from_str(&r.get::<String, _>("live"))
            .map_err(|_| Error::invalid("Invalid stored course."))?,
        r.get("published_version"),
    ))
}

#[derive(Clone, Debug, Serialize)]
pub struct Assessment {
    pub score: i64,
    pub passed: bool,
    pub completed: bool,
}
/// Concurrent retries of the same key never consume another attempt.
pub async fn assess(
    app: &App,
    s: &Session,
    course: &str,
    lesson: &str,
    version: i64,
    key: &str,
    answers: &[usize],
    assignment: &str,
) -> Result<Assessment> {
    uuid(key)?;
    if assignment.len() > 16000 {
        return Err(Error::invalid("Assignment is limited to 16,000 bytes."));
    }
    let _guard = app.mutations.lock().await;
    require(app, "course", course, Some(&s.user.id)).await?;
    let (c, current) = live(app, course).await?;
    if current != version {
        return Err(Error::conflict());
    }
    let l = c
        .lessons
        .iter()
        .find(|l| l.id == lesson)
        .ok_or_else(Error::not_found)?;
    require(app, "post", &l.post_id, Some(&s.user.id)).await?;
    let mut tx = app.db.pool.begin().await?;
    if let Some(r)=sqlx::query("SELECT score,passed FROM member_attempts WHERE user_id=$1 AND course_id=$2 AND course_version=$3 AND lesson_id=$4 AND request_key=$5").bind(&s.user.id).bind(course).bind(version).bind(lesson).bind(key).fetch_optional(&mut *tx).await?{let complete:Option<i64>=sqlx::query_scalar("SELECT completed_at FROM member_progress WHERE user_id=$1 AND course_id=$2 AND course_version=$3 AND lesson_id=$4").bind(&s.user.id).bind(course).bind(version).bind(lesson).fetch_optional(&mut *tx).await?;return Ok(Assessment{score:r.get("score"),passed:r.get::<i64,_>("passed")==1,completed:complete.is_some_and(|t|t>0)})}
    if answers.len() != l.questions.len()
        || answers
            .iter()
            .zip(&l.questions)
            .any(|(a, q)| *a >= q.choices.len())
    {
        return Err(Error::invalid("Answer every quiz question."));
    }
    if !l.assignment.is_empty() {
        label(assignment, 16000)?
    }
    let score = if l.questions.is_empty() {
        100
    } else {
        100 * answers
            .iter()
            .zip(&l.questions)
            .filter(|(a, q)| **a == q.correct)
            .count() as i64
            / l.questions.len() as i64
    };
    let passed = score >= l.pass_percent;
    let completed = passed && l.assignment.is_empty();
    let time = now();
    let updated=sqlx::query("INSERT INTO member_progress(course_id,course_version,lesson_id,user_id,attempts,best_score,completed_at) VALUES($1,$2,$3,$4,1,$5,$6) ON CONFLICT(course_id,course_version,lesson_id,user_id) DO UPDATE SET attempts=member_progress.attempts+1,best_score=CASE WHEN excluded.best_score>member_progress.best_score THEN excluded.best_score ELSE member_progress.best_score END,completed_at=CASE WHEN member_progress.completed_at>0 THEN member_progress.completed_at ELSE excluded.completed_at END WHERE member_progress.attempts<$7 RETURNING completed_at").bind(course).bind(version).bind(lesson).bind(&s.user.id).bind(score).bind(if completed{time}else{0}).bind(l.max_attempts).fetch_optional(&mut *tx).await?.ok_or(Error::invalid("Attempt limit reached. Ask the operator to review your progress."))?;
    if !l.assignment.is_empty() {
        sqlx::query("INSERT INTO member_assignments(id,course_id,course_version,lesson_id,user_id,body,state,created_at) VALUES($1,$2,$3,$4,$5,$6,'submitted',$7) ON CONFLICT(course_id,course_version,lesson_id,user_id) DO UPDATE SET body=excluded.body,state='submitted',feedback='',version=member_assignments.version+1").bind(uuid::Uuid::new_v4().to_string()).bind(course).bind(version).bind(lesson).bind(&s.user.id).bind(assignment).bind(time).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO member_attempts(id,user_id,course_id,course_version,lesson_id,request_key,score,passed,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(uuid::Uuid::new_v4().to_string()).bind(&s.user.id).bind(course).bind(version).bind(lesson).bind(key).bind(score).bind(i64::from(passed)).bind(time).execute(&mut *tx).await?;
    let result = Assessment {
        score,
        passed,
        completed: updated.get::<i64, _>("completed_at") > 0,
    };
    tx.commit().await?;
    tracing::info!(event="learning_attempt_recorded",course_id=%course,course_version=version,completed=result.completed);
    Ok(result)
}
pub async fn grade(
    app: &App,
    id: &str,
    version: i64,
    approved: bool,
    feedback: &str,
) -> Result<()> {
    if feedback.len() > 4000 {
        return Err(Error::invalid("Feedback is limited to 4,000 bytes."));
    }
    let _guard = app.mutations.lock().await;
    let mut tx = app.db.pool.begin().await?;
    let r=sqlx::query("UPDATE member_assignments SET state=$1,feedback=$2,version=version+1 WHERE id=$3 AND version=$4 RETURNING course_id,course_version,lesson_id,user_id").bind(if approved{"approved"}else{"changes"}).bind(feedback).bind(id).bind(version).fetch_optional(&mut *tx).await?.ok_or_else(Error::conflict)?;
    if approved {
        let raw: String = sqlx::query_scalar(
            "SELECT definition FROM member_course_versions WHERE course_id=$1 AND version=$2",
        )
        .bind(r.get::<String, _>("course_id"))
        .bind(r.get::<i64, _>("course_version"))
        .fetch_one(&mut *tx)
        .await?;
        let c: Course =
            serde_json::from_str(&raw).map_err(|_| Error::invalid("Invalid course history."))?;
        let l = c
            .lessons
            .iter()
            .find(|l| l.id == r.get::<String, _>("lesson_id"))
            .ok_or_else(Error::not_found)?;
        sqlx::query("UPDATE member_progress SET completed_at=$1 WHERE course_id=$2 AND course_version=$3 AND lesson_id=$4 AND user_id=$5 AND best_score>=$6").bind(now()).bind(r.get::<String,_>("course_id")).bind(r.get::<i64,_>("course_version")).bind(r.get::<String,_>("lesson_id")).bind(r.get::<String,_>("user_id")).bind(l.pass_percent).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
pub async fn certificate(app: &App, s: &Session, course: &str) -> Result<String> {
    let _guard = app.mutations.lock().await;
    require(app, "course", course, Some(&s.user.id)).await?;
    let (c, v) = live(app, course).await?;
    let completed:Vec<String>=sqlx::query_scalar("SELECT lesson_id FROM member_progress WHERE course_id=$1 AND course_version=$2 AND user_id=$3 AND completed_at>0").bind(course).bind(v).bind(&s.user.id).fetch_all(&app.db.pool).await?;
    if !c.lessons.iter().all(|l| completed.contains(&l.id)) {
        return Err(Error::invalid(
            "Complete every current lesson before requesting a certificate.",
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO member_certificates(id,user_id,course_id,course_version,issued_at) VALUES($1,$2,$3,$4,$5) ON CONFLICT(user_id,course_id,course_version) DO NOTHING").bind(id).bind(&s.user.id).bind(course).bind(v).bind(now()).execute(&app.db.pool).await?;
    sqlx::query_scalar("SELECT id FROM member_certificates WHERE user_id=$1 AND course_id=$2 AND course_version=$3 AND revoked=0").bind(&s.user.id).bind(course).bind(v).fetch_optional(&app.db.pool).await?.ok_or_else(Error::forbidden)
}

pub async fn viewer(app: &App, h: &axum::http::HeaderMap) -> Result<Option<Session>> {
    match crate::auth::session(app, h).await {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.0 == axum::http::StatusCode::UNAUTHORIZED => Ok(None),
        Err(e) => Err(e),
    }
}
pub async fn group(app: &App, title: &str, manager: &str, seats: i64) -> Result<String> {
    label(title, 160)?;
    if !manager.is_empty() {
        uuid(manager)?
    }
    if !(0..=1000).contains(&seats) {
        return Err(Error::invalid("Use at most 1,000 organization seats."));
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO member_groups(id,title,manager_id,seat_limit) VALUES($1,$2,$3,$4)")
        .bind(&id)
        .bind(title)
        .bind(manager)
        .bind(seats)
        .execute(&app.db.pool)
        .await?;
    Ok(id)
}
pub async fn seat(app: &App, s: &Session, group: &str, email: &str, remove: bool) -> Result<()> {
    let _guard = app.mutations.lock().await;
    let mut tx = app.db.pool.begin().await?;
    let g = sqlx::query("SELECT manager_id,seat_limit FROM member_groups WHERE id=$1")
        .bind(group)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(Error::not_found)?;
    let role: Option<String> = sqlx::query_scalar("SELECT role FROM users WHERE id=$1")
        .bind(&s.user.id)
        .fetch_optional(&mut *tx)
        .await?;
    if role.as_deref() != Some("admin")
        && (role.as_deref() == Some("disabled") || g.get::<String, _>("manager_id") != s.user.id)
    {
        return Err(Error::forbidden());
    }
    let user: Option<String> =
        sqlx::query_scalar("SELECT id FROM users WHERE email=$1 AND role<>'disabled'")
            .bind(email.trim().to_ascii_lowercase())
            .fetch_optional(&mut *tx)
            .await?;
    let user = user.ok_or(Error::invalid("No active account matches that email."))?;
    if remove {
        sqlx::query("DELETE FROM member_group_users WHERE group_id=$1 AND user_id=$2")
            .bind(group)
            .bind(user)
            .execute(&mut *tx)
            .await?;
    } else {
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM member_group_users WHERE group_id=$1 AND user_id=$2",
        )
        .bind(group)
        .bind(&user)
        .fetch_one(&mut *tx)
        .await?;
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM member_group_users WHERE group_id=$1")
                .bind(group)
                .fetch_one(&mut *tx)
                .await?;
        let limit: i64 = g.get("seat_limit");
        if exists == 0 && count >= if limit == 0 { 1000 } else { limit } {
            return Err(Error::invalid("Organization seat limit reached."));
        }
        sqlx::query("INSERT INTO member_group_users(group_id,user_id,created_at) VALUES($1,$2,$3) ON CONFLICT(group_id,user_id) DO NOTHING").bind(group).bind(user).bind(now()).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
pub async fn gift(app: &App, key: &str, expires: i64, duration: i64) -> Result<String> {
    label(key, 80)?;
    if !key
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || expires <= now()
        || !(1..=31536000).contains(&duration)
    {
        return Err(Error::invalid("Check gift entitlement and duration."));
    }
    let token = crate::auth::random_token();
    sqlx::query("INSERT INTO member_gifts(id,token_hash,entitlement,expires_at,duration_seconds,created_at) VALUES($1,$2,$3,$4,$5,$6)").bind(uuid::Uuid::new_v4().to_string()).bind(crate::auth::digest(token.as_bytes())).bind(key).bind(expires).bind(duration).bind(now()).execute(&app.db.pool).await?;
    Ok(token)
}
pub async fn claim_gift(app: &App, s: &Session, token: &str) -> Result<()> {
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::not_found());
    }
    let _guard = app.mutations.lock().await;
    let mut tx = app.db.pool.begin().await?;
    let time = now();
    let r=sqlx::query("UPDATE member_gifts SET claimed_by=$1 WHERE token_hash=$2 AND claimed_by='' AND expires_at>$3 RETURNING id,entitlement,duration_seconds").bind(&s.user.id).bind(crate::auth::digest(token.as_bytes())).bind(time).fetch_optional(&mut *tx).await?.ok_or(Error::invalid("Gift is expired or already claimed."))?;
    sqlx::query("INSERT INTO member_grants(id,user_id,entitlement,starts_at,expires_at,origin,created_at) VALUES($1,$2,$3,$4,$5,$6,$4)").bind(uuid::Uuid::new_v4().to_string()).bind(&s.user.id).bind(r.get::<String,_>("entitlement")).bind(time).bind(time+r.get::<i64,_>("duration_seconds")).bind(format!("gift:{}",r.get::<String,_>("id"))).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn discuss(app: &App, s: &Session, group: &str, body: &str) -> Result<()> {
    label(body, 4000)?;
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM member_group_users WHERE group_id=$1 AND user_id=$2",
    )
    .bind(group)
    .bind(&s.user.id)
    .fetch_one(&app.db.pool)
    .await?;
    if n != 1 {
        return Err(Error::forbidden());
    }
    sqlx::query("INSERT INTO member_discussions(id,group_id,user_id,body,state,created_at) VALUES($1,$2,$3,$4,'pending',$5)").bind(uuid::Uuid::new_v4().to_string()).bind(group).bind(&s.user.id).bind(body).bind(now()).execute(&app.db.pool).await?;
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub struct LearnerLesson {
    pub id: String,
    pub title: String,
    pub unlocked: bool,
    pub completed: bool,
    pub best_score: i64,
    pub attempts: i64,
}
/// A whole-course projection uses one progress read, not a query per lesson.
pub async fn learner_state(
    app: &App,
    s: &Session,
    id: &str,
) -> Result<(Course, i64, Vec<LearnerLesson>)> {
    require(app, "course", id, Some(&s.user.id)).await?;
    let (c, v) = live(app, id).await?;
    let p = sqlx::query("SELECT entitlement,group_id FROM member_policies WHERE id=$1")
        .bind(&c.policy_id)
        .fetch_one(&app.db.pool)
        .await?;
    let key: String = p.get("entitlement");
    let group: String = p.get("group_id");
    let mut since = 0;
    if !key.is_empty() {
        since=sqlx::query_scalar::<_,Option<i64>>("SELECT MIN(starts_at) FROM member_grants WHERE user_id=$1 AND entitlement=$2 AND revoked=0 AND starts_at<=$3 AND (expires_at=0 OR expires_at>$3)").bind(&s.user.id).bind(key).bind(now()).fetch_one(&app.db.pool).await?.unwrap_or(now());
    }
    if !group.is_empty() {
        let joined: Option<i64> = sqlx::query_scalar(
            "SELECT created_at FROM member_group_users WHERE group_id=$1 AND user_id=$2",
        )
        .bind(group)
        .bind(&s.user.id)
        .fetch_optional(&app.db.pool)
        .await?;
        since = since.max(joined.unwrap_or(now()));
    }
    let progress=sqlx::query("SELECT lesson_id,completed_at,best_score,attempts FROM member_progress WHERE course_id=$1 AND course_version=$2 AND user_id=$3").bind(id).bind(v).bind(&s.user.id).fetch_all(&app.db.pool).await?;
    let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id=$1")
        .bind(&s.user.id)
        .fetch_one(&app.db.pool)
        .await?;
    let staff = matches!(role.as_str(), "admin" | "editor");
    let mut prior = true;
    let time = now();
    let mut out = Vec::new();
    for l in &c.lessons {
        let p = progress
            .iter()
            .find(|p| p.get::<String, _>("lesson_id") == l.id);
        let completed = p.is_some_and(|p| p.get::<i64, _>("completed_at") > 0);
        let unlocked = staff
            || ((!c.sequential || prior)
                && time >= l.opens_at
                && time >= since.saturating_add(l.delay_seconds));
        out.push(LearnerLesson {
            id: l.id.clone(),
            title: if unlocked {
                l.title.clone()
            } else {
                "Locked lesson".into()
            },
            unlocked,
            completed,
            best_score: p.map(|p| p.get("best_score")).unwrap_or(0),
            attempts: p.map(|p| p.get("attempts")).unwrap_or(0),
        });
        prior &= completed;
    }
    Ok((c, v, out))
}
