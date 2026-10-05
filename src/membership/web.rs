//! Shared administration and private learner workflows.
use super::*;
use crate::{auth, view};
use axum::{
    Router,
    extract::{Form, Path, Query, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use maud::{Markup, html};

pub fn routes(app: &App) -> Router<App> {
    if !app.config.membership_enabled {
        return Router::new();
    };
    Router::new()
        .merge(super::identity::routes(app))
        .merge(super::referrals::routes())
        .route(
            "/admin/members/referrals",
            get(referral_admin).post(referral_action),
        )
        .route(
            "/admin/members/identity",
            get(identity_admin).post(identity_binding),
        )
        .route("/admin/members", get(admin).post(admin_action))
        .route("/admin/courses", get(courses).post(new_course))
        .route("/admin/courses/{id}", get(course_editor).post(edit_course))
        .route("/members", get(dashboard))
        .route("/members/profile", get(profile).post(save_profile))
        .route("/members/courses/{id}", get(course_page))
        .route(
            "/members/courses/{id}/lessons/{lesson}",
            get(lesson_page).post(attempt),
        )
        .route(
            "/members/courses/{id}/certificate",
            axum::routing::post(issue_certificate),
        )
        .route("/members/certificates/{id}", get(certificate_page))
        .route("/members/groups/{id}", get(group_page).post(group_action))
        .route("/members/gifts/{token}", get(gift_page).post(redeem_gift))
        .route("/api/members/courses/{id}", get(course_api))
}
async fn owner(app: &App, h: &HeaderMap) -> Result<Session> {
    let s = auth::session(app, h).await?;
    staff(app, &s).await?;
    Ok(s)
}
async fn admin_page(app: &App, s: &Session, title: &str, body: Markup) -> Result<Html<String>> {
    Ok(Html(view::layout(
        title,
        &app.db.settings().await?,
        Some(s),
        body,
    )))
}
async fn member_page(app: &App, title: &str, body: Markup) -> Result<Html<String>> {
    Ok(Html(view::member_layout(
        title,
        &app.db.settings().await?,
        html! {nav aria-label="Member area" {a href="/members" {"My learning"} " · " a href="/members/profile" {"Profile"} " · " a href="/account" {"Account"}}(body)},
    )))
}
fn utc_input(value: i64) -> String {
    if value == 0 {
        return String::new();
    }
    chrono::DateTime::from_timestamp(value, 0)
        .map(|d| d.format("%Y-%m-%dT%H:%M").to_string())
        .unwrap_or_default()
}
fn utc_date(raw: &str) -> Result<i64> {
    if raw.is_empty() {
        return Ok(0);
    }
    chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M")
        .map(|d| d.and_utc().timestamp())
        .map_err(|_| Error::invalid("Enter a valid date and time in UTC."))
}
fn hidden(name: &str, value: &str) -> Markup {
    html! {input type="hidden" name=(name) value=(value);}
}
async fn admin(State(app): State<App>, h: HeaderMap) -> Result<Html<String>> {
    let s = owner(&app, &h).await?;
    let policies=sqlx::query("SELECT id,title,entitlement,group_id,enabled,version FROM member_policies ORDER BY title LIMIT 100").fetch_all(&app.db.pool).await?;
    let groups = sqlx::query("SELECT id,title FROM member_groups ORDER BY title LIMIT 100")
        .fetch_all(&app.db.pool)
        .await?;
    let users = sqlx::query(
        "SELECT id,name,email FROM users WHERE role<>'disabled' ORDER BY name LIMIT 100",
    )
    .fetch_all(&app.db.pool)
    .await?;
    let grants=sqlx::query("SELECT g.id,g.entitlement,g.starts_at,g.expires_at,g.revoked,g.version,u.name FROM member_grants g JOIN users u ON u.id=g.user_id ORDER BY g.created_at DESC,g.id DESC LIMIT 40").fetch_all(&app.db.pool).await?;
    let resources=sqlx::query("SELECT r.kind,r.resource_id,p.title FROM member_resources r JOIN member_policies p ON p.id=r.policy_id ORDER BY r.kind,r.resource_id LIMIT 100").fetch_all(&app.db.pool).await?;
    let posts = sqlx::query("SELECT id,title FROM posts ORDER BY updated_at DESC LIMIT 100")
        .fetch_all(&app.db.pool)
        .await?;
    let media = sqlx::query("SELECT id,alt,filename FROM media ORDER BY created_at DESC LIMIT 100")
        .fetch_all(&app.db.pool)
        .await?;
    let assignments=sqlx::query("SELECT a.id,a.version,a.body,a.state,a.lesson_title,a.course_version,u.name,c.title FROM member_assignments a JOIN users u ON u.id=a.user_id JOIN member_courses c ON c.id=a.course_id WHERE a.state='submitted' ORDER BY a.created_at ASC,a.id ASC LIMIT 40").fetch_all(&app.db.pool).await?;
    let pending=sqlx::query("SELECT d.id,d.body,u.name,g.title FROM member_discussions d JOIN users u ON u.id=d.user_id JOIN member_groups g ON g.id=d.group_id WHERE d.state='pending' ORDER BY d.created_at LIMIT 40").fetch_all(&app.db.pool).await?;
    admin_page(&app,&s,"Members",html!{
 (view::heading("Community & learning","Members","Assign access, manage communities and review learning in one place."))
 p {a href="/admin/courses" {"Courses"} " · " a href="/admin/members/referrals" {"Referrals"} " · " a href="/admin/members/identity" {"Identity bindings"} " · " a href="/admin/registrations" {"Account requests"} " · " a href="/admin/users" {"User roles"}}
 section class="panel" {h2 {"Create an access policy"}p class="muted" {"When both are set, the member needs the entitlement and group."}form method="post" {(view::csrf(&s))(hidden("operation","policy"))label {"Policy title" input name="title" required maxlength="160";}label {"Entitlement key" input name="key" maxlength="80" placeholder="e.g. academy";}label {"Required group" select name="group" {option value="" {"No group requirement"}@for g in &groups {option value=(g.get::<String,_>("id")){(g.get::<String,_>("title"))}}}}button {"Create policy"}}}
 section class="panel" {h2 {"Assign a membership"}form method="post" {(view::csrf(&s))(hidden("operation","grant"))label {"Member" select name="user" required {@for u in &users {option value=(u.get::<String,_>("id")){(u.get::<String,_>("name")) " · " (u.get::<String,_>("email"))}}}}label {"Entitlement key" input name="key" required maxlength="80";}label {"Starts at (UTC)" input type="datetime-local" name="starts" value=(utc_input(now())) required;}label {"Expires at (UTC, leave blank for no expiry)" input type="datetime-local" name="expires";}button {"Assign access"}}
 @for g in &grants {div class="toolbar" {strong {(g.get::<String,_>("name"))}span {(g.get::<String,_>("entitlement"))}span class="status" {(if g.get::<i64,_>("revoked")==1{"Revoked"}else if g.get::<i64,_>("expires_at")>0&&g.get::<i64,_>("expires_at")<=now(){"Expired"}else{"Assigned"})}form method="post" {(view::csrf(&s))(hidden("operation","revoke"))(hidden("id",&g.get::<String,_>("id")))(hidden("version",&g.get::<i64,_>("version").to_string()))button class="quiet" {"Revoke"}}}}}
 section class="panel" {h2 {"Protect content or a download"}p {"Public files remain public until a media rule is assigned. Course lesson rules are managed by their course."}form method="post" {(view::csrf(&s))label {"Resource" select name="resource" {@for p in &posts {option value=(format!("post:{}",p.get::<String,_>("id"))){"Content · " (p.get::<String,_>("title"))}}@for m in &media {option value=(format!("media:{}",m.get::<String,_>("id"))){"Media · " (m.get::<String,_>("alt")) " " (m.get::<String,_>("filename"))}}}}label {"Access policy" select name="policy" required {@for p in &policies {option value=(p.get::<String,_>("id")){(p.get::<String,_>("title"))}}}}label {"Opens at (UTC, leave blank for immediately)" input type="datetime-local" name="opens";}label {"Delay after membership starts (seconds)" input type="number" name="delay" value="0" min="0" max="31536000";}button name="operation" value="protect" {"Protect resource"}button name="operation" value="release" class="quiet" {"Release access rule"}p class="muted" {"Release makes published content discoverable again; unprotected private files still require staff access. Active course resources cannot be released."}}
 @for r in resources {p {(r.get::<String,_>("kind")) " · " (r.get::<String,_>("resource_id")) " → " (r.get::<String,_>("title"))}}}
 section class="panel" {h2 {"Policies"}@for p in &policies {form method="post" class="toolbar" {(view::csrf(&s))(hidden("operation","policy-toggle"))(hidden("id",&p.get::<String,_>("id")))(hidden("version",&p.get::<i64,_>("version").to_string()))strong {(p.get::<String,_>("title"))}span {(p.get::<String,_>("entitlement"))}button class="quiet" {(if p.get::<i64,_>("enabled")==1{"Suspend policy"}else{"Enable policy"})}}}}
 section class="panel" {h2 {"Create a group or organization"}form method="post" {(view::csrf(&s))(hidden("operation","group"))label {"Group title" input name="title" required maxlength="160";}label {"Seat manager" select name="user" {option value="" {"Site owner only"}@for u in &users {option value=(u.get::<String,_>("id")){(u.get::<String,_>("name"))}}}}label {"Seat limit (0 means ordinary group)" input type="number" name="seats" value="0" min="0" max="1000";}button {"Create group"}}@for g in &groups {p {a href=(format!("/members/groups/{}",g.get::<String,_>("id"))){(g.get::<String,_>("title"))}}}}
 section class="panel" {h2 {"Create a single-use gift"}form method="post" {(view::csrf(&s))(hidden("operation","gift"))label {"Entitlement key" input name="key" required maxlength="80";}label {"Claim before (UTC)" input type="datetime-local" name="expires" value=(utc_input(now()+86400)) required;}label {"Access duration (seconds)" input type="number" name="duration" min="1" max="31536000" value="2592000" required;}button {"Create gift link"}}}
 section class="panel" {h2 {"Assignments & gradebook"}p {a href="/admin/courses" {"Open a course to inspect member progress."}}@for a in assignments {article {h3 {(a.get::<String,_>("name")) " · " (a.get::<String,_>("title"))}p class="muted" {"Edition " (a.get::<i64,_>("course_version")) " · " (a.get::<String,_>("lesson_title"))}p class="status" {(a.get::<String,_>("state"))}p {(a.get::<String,_>("body"))}form method="post" {(view::csrf(&s))(hidden("operation","grade"))(hidden("id",&a.get::<String,_>("id")))(hidden("version",&a.get::<i64,_>("version").to_string()))label {"Feedback" textarea name="feedback" maxlength="4000" {}}button name="decision" value="approve" {"Approve work"}button name="decision" value="changes" class="quiet" {"Request changes"}}}}}
 section class="panel" {h2 {"Discussion moderation"}@for d in pending {article {h3 {(d.get::<String,_>("title")) " · " (d.get::<String,_>("name"))}p {(d.get::<String,_>("body"))}form method="post" {(view::csrf(&s))(hidden("operation","moderate"))(hidden("id",&d.get::<String,_>("id")))button name="decision" value="approve" {"Approve"}button name="decision" value="reject" class="quiet" {"Reject"}}}}}
 }).await
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct Action {
    csrf: String,
    operation: String,
    title: String,
    key: String,
    group: String,
    user: String,
    id: String,
    version: i64,
    starts: String,
    expires: String,
    resource: String,
    policy: String,
    opens: String,
    delay: i64,
    seats: i64,
    duration: i64,
    feedback: String,
    decision: String,
}
async fn admin_action(
    State(app): State<App>,
    h: HeaderMap,
    Form(i): Form<Action>,
) -> Result<Response> {
    let s = owner(&app, &h).await?;
    auth::csrf(&s, &i.csrf)?;
    match i.operation.as_str() {
        "policy" => {
            policy(&app, &i.title, &i.key, &i.group).await?;
        }
        "grant" => {
            grant(
                &app,
                &i.user,
                &i.key,
                utc_date(&i.starts)?,
                utc_date(&i.expires)?,
                "local-owner",
            )
            .await?;
        }
        "revoke" => {
            let _guard = app.mutation().await?;
            staff(&app, &s).await?;
            if sqlx::query(
                "UPDATE member_grants SET revoked=1,version=version+1 WHERE id=$1 AND version=$2",
            )
            .bind(i.id)
            .bind(i.version)
            .execute(&app.db.pool)
            .await?
            .rows_affected()
                != 1
            {
                return Err(Error::conflict());
            }
        }
        "policy-toggle" => {
            let _guard = app.mutation().await?;
            staff(&app, &s).await?;
            if sqlx::query("UPDATE member_policies SET enabled=1-enabled,version=version+1 WHERE id=$1 AND version=$2").bind(i.id).bind(i.version).execute(&app.db.pool).await?.rows_affected()!=1{return Err(Error::conflict())}
        }
        "protect" => {
            let (k, id) = i
                .resource
                .split_once(':')
                .ok_or(Error::invalid("Select a resource."))?;
            protect(&app, k, id, &i.policy, utc_date(&i.opens)?, i.delay).await?
        }
        "release" => {
            let (kind, id) = i
                .resource
                .split_once(':')
                .ok_or(Error::invalid("Select a resource."))?;
            release(&app, kind, id).await?;
        }
        "group" => {
            group(&app, &i.title, &i.user, i.seats).await?;
        }
        "gift" => {
            let token = gift(&app, &i.key, utc_date(&i.expires)?, i.duration).await?;
            return Ok(admin_page(&app,&s,"Gift link",html!{h1 {"Gift link ready"}p {"Copy this link now. It is shown once and can be claimed by one signed-in member."}p {a href=(format!("/members/gifts/{token}")){(format!("{}/members/gifts/{token}",app.config.origin()))}}a href="/admin/members" {"Back to members"}}).await?.into_response());
        }
        "grade" => grade(&app, &i.id, i.version, i.decision == "approve", &i.feedback).await?,
        "identity-remove" => {
            let _guard = app.mutation().await?;
            staff(&app, &s).await?;
            let mut tx = app.db.pool.begin().await?;
            let user: Option<String> = sqlx::query_scalar(
                "DELETE FROM member_identities WHERE issuer=$1 AND subject=$2 RETURNING user_id",
            )
            .bind(&i.title)
            .bind(&i.key)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(user) = user {
                // Revoke existing sessions too: removing a provider binding ends that account's active sign-ins.
                sqlx::query("DELETE FROM sessions WHERE user_id=$1")
                    .bind(user)
                    .execute(&mut *tx)
                    .await?;
            }
            tx.commit().await?;
        }
        "moderate" => {
            sqlx::query("UPDATE member_discussions SET state=$1 WHERE id=$2 AND state='pending'")
                .bind(if i.decision == "approve" {
                    "approved"
                } else {
                    "rejected"
                })
                .bind(i.id)
                .execute(&app.db.pool)
                .await?;
        }
        _ => return Err(Error::invalid("Unknown member operation.")),
    }
    Ok(Redirect::to("/admin/members").into_response())
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct CatalogCursor {
    before: String,
}
impl CatalogCursor {
    fn values(&self) -> Result<(i64, String)> {
        if self.before.is_empty() {
            return Ok((i64::MAX, "ffffffff-ffff-ffff-ffff-ffffffffffff".into()));
        }
        let (time, id) = self
            .before
            .split_once(':')
            .ok_or(Error::invalid("Invalid course cursor."))?;
        uuid(id)?;
        let time: i64 = time
            .parse()
            .map_err(|_| Error::invalid("Invalid course cursor."))?;
        if time < 0 {
            return Err(Error::invalid("Invalid course cursor."));
        }
        Ok((time, id.into()))
    }
}
fn course_pager(path: &str, first: bool, next: Option<&String>) -> Markup {
    if first && next.is_none() {
        return html! {};
    }
    html! {nav aria-label="Course pages" class="toolbar" {
        @if !first {a class="button secondary" href=(path) {"First course page"}}
        @if let Some(cursor)=next {a class="button secondary" href=(format!("{path}?before={cursor}")) {"More courses"}}
    }}
}
fn next_course_page(rows: &mut Vec<sqlx::any::AnyRow>) -> Option<String> {
    if rows.len() <= 40 {
        return None;
    }
    rows.pop();
    rows.last().map(|r| {
        format!(
            "{}:{}",
            r.get::<i64, _>("created_at"),
            r.get::<String, _>("id")
        )
    })
}
async fn courses(
    State(app): State<App>,
    h: HeaderMap,
    Query(cursor): Query<CatalogCursor>,
) -> Result<Html<String>> {
    let s = owner(&app, &h).await?;
    let (time, id) = cursor.values()?;
    let mut rows=sqlx::query("SELECT id,title,published_version,created_at FROM member_courses WHERE (created_at,id)<($1,$2) ORDER BY created_at DESC,id DESC LIMIT 41").bind(time).bind(id).fetch_all(&app.db.pool).await?;
    let next = next_course_page(&mut rows);
    let policies = sqlx::query("SELECT id,title FROM member_policies ORDER BY title LIMIT 100")
        .fetch_all(&app.db.pool)
        .await?;
    let posts = sqlx::query(
        "SELECT id,title FROM posts WHERE status='published' ORDER BY updated_at DESC LIMIT 100",
    )
    .fetch_all(&app.db.pool)
    .await?;
    admin_page(&app,&s,"Courses",html!{(view::heading("Learning","Courses","Compose lessons from shared published content, then add assessments and publish."))p {a href="/admin/members" {"Memberships & review"}}section class="panel" {h2 {"Create a course"}form method="post" {(view::csrf(&s))label {"Course title" input name="title" maxlength="160" required;}label {"Access policy" select name="policy" required {@for p in policies {option value=(p.get::<String,_>("id")){(p.get::<String,_>("title"))}}}}label {"First lesson's content" select name="post" required {@for p in posts {option value=(p.get::<String,_>("id")){(p.get::<String,_>("title"))}}}}button {"Create course"}}}section class="panel" {h2 {"Your courses"}@for r in rows {p {a href=(format!("/admin/courses/{}",r.get::<String,_>("id"))){(r.get::<String,_>("title"))} " · " (if r.get::<i64,_>("published_version")>0{"Published"}else{"Draft"})}}(course_pager("/admin/courses",cursor.before.is_empty(),next.as_ref()))}}).await
}
#[derive(Deserialize)]
struct NewCourse {
    csrf: String,
    title: String,
    policy: String,
    post: String,
}
async fn new_course(
    State(app): State<App>,
    h: HeaderMap,
    Form(i): Form<NewCourse>,
) -> Result<Redirect> {
    let s = owner(&app, &h).await?;
    auth::csrf(&s, &i.csrf)?;
    let lesson = Lesson {
        id: uuid::Uuid::new_v4().to_string(),
        title: "First lesson".into(),
        post_id: i.post,
        downloads: vec![],
        delay_seconds: 0,
        opens_at: 0,
        questions: vec![],
        assignment: String::new(),
        pass_percent: 70,
        max_attempts: 3,
    };
    let id = create_course(
        &app,
        &Course {
            title: i.title,
            policy_id: i.policy,
            sequential: true,
            lessons: vec![lesson],
        },
    )
    .await?;
    Ok(Redirect::to(&format!("/admin/courses/{id}")))
}
async fn course_editor(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = owner(&app, &h).await?;
    let r = sqlx::query("SELECT draft,version,published_version FROM member_courses WHERE id=$1")
        .bind(&id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    let c: Course = serde_json::from_str(&r.get::<String, _>("draft"))
        .map_err(|_| Error::invalid("Invalid course."))?;
    let version = r.get::<i64, _>("version").to_string();
    let posts = sqlx::query(
        "SELECT id,title FROM posts WHERE status='published' ORDER BY updated_at DESC LIMIT 100",
    )
    .fetch_all(&app.db.pool)
    .await?;
    let media = sqlx::query("SELECT id,alt,filename FROM media ORDER BY created_at DESC LIMIT 100")
        .fetch_all(&app.db.pool)
        .await?;
    let progress=sqlx::query("SELECT u.name,p.user_id,p.lesson_id,p.course_version,p.attempts,p.best_score,p.completed_at FROM member_progress p JOIN users u ON u.id=p.user_id WHERE p.course_id=$1 ORDER BY p.course_version DESC,u.name,p.lesson_id LIMIT 100").bind(&id).fetch_all(&app.db.pool).await?;
    admin_page(&app,&s,"Edit course",html!{(view::heading("Learning",&c.title,"Draft edits stay private. Publish deliberately; a new edition has its own progress."))p {a href="/admin/courses" {"All courses"} " · " a href=(format!("/members/courses/{id}")){"Member view"}}section class="panel" {form method="post" {(view::csrf(&s))(hidden("version",&version))(hidden("operation","publish"))p {"Published edition: " (r.get::<i64,_>("published_version"))}button {"Publish course edition"}}}
 @for (index,l) in c.lessons.iter().enumerate(){section class="panel" {h2 {(index+1) ". " (&l.title)}p {a href=(format!("/admin/posts/{}",l.post_id)){"Edit shared lesson content"}}form method="post" {(view::csrf(&s))(hidden("version",&version))(hidden("lesson",&l.id))(hidden("operation","lesson"))label {"Lesson title" input name="title" value=(&l.title) required maxlength="160";}label {"Opens at (UTC, leave blank for now)" input name="opens" type="datetime-local" value=(utc_input(l.opens_at));}label {"Delay after access starts (seconds)" input name="delay" type="number" value=(l.delay_seconds) min="0" max="31536000";}label {"Assignment instructions (optional)" textarea name="assignment" maxlength="4000" {(&l.assignment)}}label {"Quiz prompt (leave blank for no quiz)" textarea name="prompt" maxlength="1000" {(l.questions.first().map(|q|q.prompt.as_str()).unwrap_or(""))}}label {"Choices, one per line" textarea name="choices" maxlength="4000" {(l.questions.first().map(|q|q.choices.join("\n")).unwrap_or_default())}}label {"Correct choice number" input name="correct" type="number" min="1" max="8" value=(l.questions.first().map(|q|q.correct+1).unwrap_or(1));}label {"Required score (%)" input name="pass" type="number" min="1" max="100" value=(l.pass_percent);}label {"Attempt limit" input name="attempts" type="number" min="1" max="10" value=(l.max_attempts);}button {"Save lesson draft"}}}}
 section class="panel" {h2 {"Course settings & order"}form method="post" {(view::csrf(&s))(hidden("version",&version))(hidden("operation","settings"))label {"Course title" input name="title" value=(&c.title) required maxlength="160";}label {input type="checkbox" name="sequential" value="yes" checked[c.sequential]; " Complete lessons in order"}button {"Save course settings"}}
 @for (index,l) in c.lessons.iter().enumerate(){form method="post" class="toolbar" {(view::csrf(&s))(hidden("version",&version))(hidden("lesson",&l.id))span {(&l.title)}button name="operation" value="up" disabled[index==0] class="quiet" {"Move earlier"}button name="operation" value="down" disabled[index+1==c.lessons.len()] class="quiet" {"Move later"}button name="operation" value="remove" disabled[c.lessons.len()==1] class="quiet" {"Remove lesson"}}}}
 @for l in &c.lessons {section class="panel" {h2 {"Quiz questions · " (&l.title)}@for(index,q) in l.questions.iter().enumerate(){form method="post" class="toolbar" {(view::csrf(&s))(hidden("version",&version))(hidden("lesson",&l.id))(hidden("question",&index.to_string()))span {(&q.prompt)}button name="operation" value="quiz-remove" class="quiet" {"Remove question"}}}details {summary {"Add a quiz question"}form method="post" {(view::csrf(&s))(hidden("version",&version))(hidden("lesson",&l.id))(hidden("operation","quiz-add"))label {"Question prompt" textarea name="prompt" required maxlength="1000" {}}label {"Answer choices, one per line" textarea name="choices" required maxlength="4000" {}}label {"Correct answer number" input type="number" name="correct" min="1" max="8" value="1" required;}button {"Add question"}}}}}
 @for l in &c.lessons {section class="panel" {h2 {"Protected downloads · " (&l.title)}p {"Assign files here to enforce this lesson's access and prerequisites."}@for id in &l.downloads{form method="post" class="toolbar" {(view::csrf(&s))(hidden("version",&version))(hidden("lesson",&l.id))(hidden("media",id))span {(id)}button name="operation" value="media-remove" class="quiet" {"Remove download"}}}form method="post" {(view::csrf(&s))(hidden("version",&version))(hidden("lesson",&l.id))(hidden("operation","media-add"))label {"Uploaded file" select name="media" required {@for m in &media{option value=(m.get::<String,_>("id")){(m.get::<String,_>("alt")) " " (m.get::<String,_>("filename"))}}}}button {"Assign protected download"}}}}
 section class="panel" {h2 {"Add a lesson"}form method="post" {(view::csrf(&s))(hidden("version",&version))(hidden("operation","add"))label {"Lesson title" input name="title" required maxlength="160";}label {"Shared content" select name="post" required {@for p in posts {option value=(p.get::<String,_>("id")){(p.get::<String,_>("title"))}}}}button {"Add lesson"}}}
 section class="panel" {h2 {"Gradebook"}@for p in progress {p {(p.get::<String,_>("name")) " · edition " (p.get::<i64,_>("course_version")) " · " (p.get::<String,_>("lesson_id")) " · score " (p.get::<i64,_>("best_score")) "% · " (p.get::<i64,_>("attempts")) " attempts · " (if p.get::<i64,_>("completed_at")>0{"Complete"}else{"In progress"})}form method="post" {(view::csrf(&s))(hidden("version",&version))(hidden("lesson",&p.get::<String,_>("lesson_id")))(hidden("user",&p.get::<String,_>("user_id")))(hidden("edition",&p.get::<i64,_>("course_version").to_string()))button name="operation" value="reset-progress" class="quiet" {"Reset attempts and completion"}}}}
 }).await
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct Edit {
    csrf: String,
    version: i64,
    operation: String,
    lesson: String,
    title: String,
    post: String,
    opens: String,
    delay: i64,
    assignment: String,
    prompt: String,
    choices: String,
    correct: usize,
    pass: i64,
    attempts: i64,
    sequential: String,
    question: usize,
    media: String,
    user: String,
    edition: i64,
}
async fn edit_course(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(i): Form<Edit>,
) -> Result<Redirect> {
    let s = owner(&app, &h).await?;
    auth::csrf(&s, &i.csrf)?;
    let raw: String = sqlx::query_scalar("SELECT draft FROM member_courses WHERE id=$1")
        .bind(&id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    let mut c: Course =
        serde_json::from_str(&raw).map_err(|_| Error::invalid("Invalid course."))?;
    match i.operation.as_str() {
        "publish" => {}
        "add" => c.lessons.push(Lesson {
            id: uuid::Uuid::new_v4().to_string(),
            title: i.title,
            post_id: i.post,
            downloads: vec![],
            delay_seconds: 0,
            opens_at: 0,
            questions: vec![],
            assignment: String::new(),
            pass_percent: 70,
            max_attempts: 3,
        }),
        "media-add" | "media-remove" => {
            let l = c
                .lessons
                .iter_mut()
                .find(|l| l.id == i.lesson)
                .ok_or_else(Error::not_found)?;
            uuid(&i.media)?;
            if i.operation == "media-add" {
                l.downloads.push(i.media)
            } else {
                l.downloads.retain(|id| id != &i.media)
            }
        }
        "reset-progress" => {
            reset_progress(&app, &id, i.edition, &i.lesson, &i.user).await?;
            return Ok(Redirect::to(&format!("/admin/courses/{id}")));
        }
        "settings" => {
            c.title = i.title;
            c.sequential = i.sequential == "yes";
        }
        "up" | "down" | "remove" => {
            let pos = c
                .lessons
                .iter()
                .position(|l| l.id == i.lesson)
                .ok_or_else(Error::not_found)?;
            match i.operation.as_str() {
                "up" if pos > 0 => c.lessons.swap(pos, pos - 1),
                "down" if pos + 1 < c.lessons.len() => c.lessons.swap(pos, pos + 1),
                "remove" if c.lessons.len() > 1 => {
                    c.lessons.remove(pos);
                }
                _ => return Err(Error::invalid("Cannot move or remove this lesson.")),
            }
        }
        "quiz-add" | "quiz-remove" => {
            let l = c
                .lessons
                .iter_mut()
                .find(|l| l.id == i.lesson)
                .ok_or_else(Error::not_found)?;
            if i.operation == "quiz-remove" {
                if i.question >= l.questions.len() {
                    return Err(Error::conflict());
                }
                l.questions.remove(i.question);
            } else {
                l.questions.push(Question {
                    prompt: i.prompt,
                    choices: i
                        .choices
                        .lines()
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(str::to_owned)
                        .collect(),
                    correct: i
                        .correct
                        .checked_sub(1)
                        .ok_or(Error::invalid("Answer numbers begin at one."))?,
                });
            }
        }
        "lesson" => {
            let l = c
                .lessons
                .iter_mut()
                .find(|l| l.id == i.lesson)
                .ok_or_else(Error::not_found)?;
            l.title = i.title;
            l.opens_at = utc_date(&i.opens)?;
            l.delay_seconds = i.delay;
            l.assignment = i.assignment;
            l.pass_percent = i.pass;
            l.max_attempts = i.attempts;
            if i.prompt.trim().is_empty() {
                if !l.questions.is_empty() {
                    l.questions.remove(0);
                }
            } else {
                let q = Question {
                    prompt: i.prompt,
                    choices: i
                        .choices
                        .lines()
                        .map(str::trim)
                        .filter(|c| !c.is_empty())
                        .map(str::to_owned)
                        .collect(),
                    correct: i
                        .correct
                        .checked_sub(1)
                        .ok_or(Error::invalid("Correct choice numbers begin at one."))?,
                };
                if l.questions.is_empty() {
                    l.questions.push(q)
                } else {
                    l.questions[0] = q
                }
            }
        }
        _ => return Err(Error::invalid("Unknown course operation.")),
    }
    save_course(&app, &id, i.version, &c, i.operation == "publish").await?;
    Ok(Redirect::to(&format!("/admin/courses/{id}")))
}

async fn dashboard(
    State(app): State<App>,
    h: HeaderMap,
    Query(cursor): Query<CatalogCursor>,
) -> Result<Html<String>> {
    let s = auth::session(&app, &h).await?;
    let (time, id) = cursor.values()?;
    let mut rows=sqlx::query("SELECT c.id,c.created_at,c.published_title AS title,c.published_version,(SELECT COUNT(*) FROM member_progress pr WHERE pr.course_id=c.id AND pr.course_version=c.published_version AND pr.user_id=$1 AND pr.completed_at>0) AS completed FROM member_courses c JOIN member_resources cr ON cr.kind='course' AND cr.resource_id=c.id JOIN member_policies p ON p.id=cr.policy_id JOIN users u ON u.id=$1 WHERE c.published_version>0 AND (c.created_at,c.id)<($3,$4) AND (u.role IN ('admin','editor') OR (u.role<>'disabled' AND p.enabled=1 AND (p.entitlement='' OR EXISTS(SELECT 1 FROM member_grants g WHERE g.user_id=$1 AND g.entitlement=p.entitlement AND g.revoked=0 AND g.starts_at<=$2 AND (g.expires_at=0 OR g.expires_at>$2))) AND (p.group_id='' OR EXISTS(SELECT 1 FROM member_group_users gu WHERE gu.group_id=p.group_id AND gu.user_id=$1)))) ORDER BY c.created_at DESC,c.id DESC LIMIT 41").bind(&s.user.id).bind(now()).bind(time).bind(id).fetch_all(&app.db.pool).await?;
    let next = next_course_page(&mut rows);
    let groups=sqlx::query("SELECT g.id,g.title FROM member_groups g WHERE g.manager_id=$1 OR EXISTS(SELECT 1 FROM member_group_users gu WHERE gu.group_id=g.id AND gu.user_id=$1) ORDER BY g.title LIMIT 100").bind(&s.user.id).fetch_all(&app.db.pool).await?;
    member_page(&app,"My learning",html!{(view::heading("Member area","My learning",&format!("Welcome, {}. Your access and progress are managed on this site.",s.user.name)))section class="panel" {h2 {"Your courses"}@if rows.is_empty(){p {"No courses are available with your current membership. Ask the site operator about access."}}@for c in rows {p {a href=(format!("/members/courses/{}",c.get::<String,_>("id"))){(c.get::<String,_>("title"))} " · " (c.get::<i64,_>("completed")) " lessons complete"}}(course_pager("/members",cursor.before.is_empty(),next.as_ref()))}section class="panel" {h2 {"Your communities"}@for g in groups {p {a href=(format!("/members/groups/{}",g.get::<String,_>("id"))){(g.get::<String,_>("title"))}}}}}).await
}
async fn course_page(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = auth::session(&app, &h).await?;
    let (c, v, state) = learner_state(&app, &s, &id).await?;
    member_page(&app,&c.title,html!{(view::heading("Learning",&c.title,"Work through the lessons. Assessments and reviewed assignments update your progress."))p {"Edition " (v)}ol {@for l in &state{li {@if l.unlocked{a href=(format!("/members/courses/{id}/lessons/{}",l.id)){(&l.title)}}@else{span {"Locked lesson"}} " · " (if l.completed{"Complete"}else if l.unlocked{"Ready"}else{"Complete earlier lessons or wait for the scheduled unlock."})}}}form method="post" action=(format!("/members/courses/{id}/certificate")){(view::csrf(&s))button disabled[!state.iter().all(|l|l.completed)] {"Get completion certificate"}}}).await
}
async fn course_api(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<axum::Json<serde_json::Value>> {
    let s = auth::session(&app, &h).await?;
    let (c, v, state) = learner_state(&app, &s, &id).await?;
    Ok(axum::Json(
        serde_json::json!({"id":id,"title":c.title,"version":v,"lessons":state}),
    ))
}
async fn lesson_page(
    State(app): State<App>,
    h: HeaderMap,
    Path((id, lesson)): Path<(String, String)>,
) -> Result<Html<String>> {
    let s = auth::session(&app, &h).await?;
    let (c, v, state) = learner_state(&app, &s, &id).await?;
    let l = c
        .lessons
        .iter()
        .find(|l| l.id == lesson)
        .ok_or_else(Error::not_found)?;
    if !state.iter().any(|l| l.id == lesson && l.unlocked) {
        return Err(Error::forbidden());
    }
    require(&app, "post", &l.post_id, Some(&s.user.id)).await?;
    let p = sqlx::query(
        "SELECT published_document,published_title FROM posts WHERE id=$1 AND status='published'",
    )
    .bind(&l.post_id)
    .fetch_optional(&app.db.pool)
    .await?
    .ok_or_else(Error::not_found)?;
    let document = crate::document::Document::parse(&p.get::<String, _>("published_document"))?;
    let progress = state
        .iter()
        .find(|item| item.id == lesson)
        .ok_or_else(Error::not_found)?;
    let completed = progress.completed;
    let exhausted = progress.attempts >= l.max_attempts;
    let assignment=sqlx::query("SELECT state,feedback,body FROM member_assignments WHERE course_id=$1 AND course_version=$2 AND lesson_id=$3 AND user_id=$4").bind(&id).bind(v).bind(&lesson).bind(&s.user.id).fetch_optional(&app.db.pool).await?;
    member_page(&app,&l.title,html!{p {a href=(format!("/members/courses/{id}")){(&c.title)}}(view::heading("Lesson",&l.title,"Read the lesson, then record your learning."))section class="prose" {(maud::PreEscaped(document.html()))}@if !l.downloads.is_empty(){section class="panel" {h2 {"Lesson downloads"}@for download in &l.downloads{p {a href=(format!("/media/{download}")){"Open protected lesson download"}}}}}
 @if let Some(a)=&assignment {section class="panel" {h2 {"Assignment review"}p class="muted" {"Edition " (a.get::<i64,_>("course_version")) " · " (a.get::<String,_>("lesson_title"))}p class="status" {(a.get::<String,_>("state"))}p {(a.get::<String,_>("feedback"))}}}
 section class="panel" {h2 {"Your assessment"}@if completed {p role="status" {"Lesson complete. Ask the operator to reset it before submitting replacement work."}}@else if exhausted {p role="status" {"Attempt limit reached. Ask the operator to review your work or reset your attempts."}}@else {form method="post" {(view::csrf(&s))(hidden("version",&v.to_string()))(hidden("key",&uuid::Uuid::new_v4().to_string()))
 @for (index,q) in l.questions.iter().enumerate(){fieldset {legend {(&q.prompt)}@for (choice,text) in q.choices.iter().enumerate(){label {input type="radio" name=(format!("answer_{index}")) value=(choice) required; (text)}}}}
 @if !l.assignment.is_empty(){label {(&l.assignment)textarea name="assignment" required maxlength="16000" {(assignment.as_ref().map(|a|a.get::<String,_>("body")).unwrap_or_default())}}}
 p class="muted" {"Up to " (l.max_attempts) " attempts. Required quiz score: " (l.pass_percent) "%. Assignments require operator approval."}button {(if l.questions.is_empty()&&l.assignment.is_empty(){"Mark lesson complete"}else{"Submit assessment"})}}}}
 }).await
}
async fn attempt(
    State(app): State<App>,
    h: HeaderMap,
    Path((id, lesson)): Path<(String, String)>,
    Form(fields): Form<std::collections::HashMap<String, String>>,
) -> Result<Response> {
    let s = auth::session(&app, &h).await?;
    auth::csrf(&s, fields.get("csrf").map(String::as_str).unwrap_or(""))?;
    let version = fields
        .get("version")
        .and_then(|v| v.parse().ok())
        .ok_or(Error::invalid("Reload the course edition."))?;
    let (c, _) = live(&app, &id).await?;
    let l = c
        .lessons
        .iter()
        .find(|l| l.id == lesson)
        .ok_or_else(Error::not_found)?;
    let mut answers = Vec::new();
    for i in 0..l.questions.len() {
        answers.push(
            fields
                .get(&format!("answer_{i}"))
                .and_then(|v| v.parse().ok())
                .ok_or(Error::invalid("Answer every question."))?,
        )
    }
    let result = assess(
        &app,
        &s,
        &id,
        &lesson,
        AttemptInput {
            version,
            key: fields.get("key").map(String::as_str).unwrap_or(""),
            answers: &answers,
            assignment: fields.get("assignment").map(String::as_str).unwrap_or(""),
        },
    )
    .await?;
    Ok(member_page(&app,"Assessment saved",html!{h1 {"Assessment saved"}p role="status" {"Score: " (result.score) "% · " (if result.completed{"Lesson complete"}else if result.passed{"Awaiting assignment review"}else{"Review the lesson and try again within your attempt limit."})}p {a href=(format!("/members/courses/{id}")){"Continue learning"}}p {a href=(format!("/members/courses/{id}/lessons/{lesson}")){"Return to lesson"}}}).await?.into_response())
}
#[derive(Deserialize)]
struct Csrf {
    csrf: String,
}
async fn issue_certificate(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(i): Form<Csrf>,
) -> Result<Redirect> {
    let s = auth::session(&app, &h).await?;
    auth::csrf(&s, &i.csrf)?;
    let certificate = certificate(&app, &s, &id).await?;
    Ok(Redirect::to(&format!(
        "/members/certificates/{certificate}"
    )))
}
async fn certificate_page(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = auth::session(&app, &h).await?;
    let r=sqlx::query("SELECT cert.user_id,cert.course_id,cert.course_version,cert.issued_at,u.name,v.definition FROM member_certificates cert JOIN users u ON u.id=cert.user_id JOIN member_course_versions v ON v.course_id=cert.course_id AND v.version=cert.course_version WHERE cert.id=$1 AND cert.revoked=0").bind(&id).fetch_optional(&app.db.pool).await?.ok_or_else(Error::not_found)?;
    if r.get::<String, _>("user_id") != s.user.id && !s.is_admin() {
        return Err(Error::forbidden());
    }
    require(
        &app,
        "course",
        &r.get::<String, _>("course_id"),
        Some(&s.user.id),
    )
    .await?;
    let c: Course = serde_json::from_str(&r.get::<String, _>("definition"))
        .map_err(|_| Error::invalid("Invalid certificate edition."))?;
    member_page(&app,"Completion certificate",html!{(view::heading("Learning","Completion certificate","A local record of successful completion, verified against this site's course edition."))h2 {(&c.title)}p {(r.get::<String,_>("name"))}p {"Course edition " (r.get::<i64,_>("course_version"))}p {"Certificate " (&id)}p {"Issued at UTC Unix time " (r.get::<i64,_>("issued_at"))}}).await
}
async fn profile(State(app): State<App>, h: HeaderMap) -> Result<Html<String>> {
    let s = auth::session(&app, &h).await?;
    let biography: Option<String> =
        sqlx::query_scalar("SELECT biography FROM member_profiles WHERE user_id=$1")
            .bind(&s.user.id)
            .fetch_optional(&app.db.pool)
            .await?;
    member_page(&app,"Your profile",html!{(view::heading("Member area","Your profile","Your profile is visible to you and the operator. Community posts show your chosen name."))form method="post" {(view::csrf(&s))label {"Display name" input name="name" value=(&s.user.name) required maxlength="100";}label {"Biography" textarea name="biography" maxlength="2000" {(biography.unwrap_or_default())}}button {"Save profile"}}}).await
}
#[derive(Deserialize)]
struct Profile {
    csrf: String,
    name: String,
    biography: String,
}
async fn save_profile(
    State(app): State<App>,
    h: HeaderMap,
    Form(i): Form<Profile>,
) -> Result<Redirect> {
    let s = auth::session(&app, &h).await?;
    auth::csrf(&s, &i.csrf)?;
    auth::valid_user(&s.user.email, &i.name, &s.user.role)?;
    if i.biography.len() > 2000 {
        return Err(Error::invalid("Biography is limited to 2,000 bytes."));
    }
    let _guard = app.mutation().await?;
    let mut tx = app.db.pool.begin().await?;
    sqlx::query("UPDATE users SET name=$1 WHERE id=$2")
        .bind(i.name.trim())
        .bind(&s.user.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO member_profiles(user_id,biography) VALUES($1,$2) ON CONFLICT(user_id) DO UPDATE SET biography=excluded.biography,version=member_profiles.version+1").bind(&s.user.id).bind(i.biography).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Redirect::to("/members/profile"))
}
async fn group_page(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let s = auth::session(&app, &h).await?;
    let g = sqlx::query("SELECT title,manager_id,seat_limit FROM member_groups WHERE id=$1")
        .bind(&id)
        .fetch_optional(&app.db.pool)
        .await?
        .ok_or_else(Error::not_found)?;
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM member_group_users WHERE group_id=$1 AND user_id=$2",
    )
    .bind(&id)
    .bind(&s.user.id)
    .fetch_one(&app.db.pool)
    .await?;
    let manager = s.is_admin() || g.get::<String, _>("manager_id") == s.user.id;
    if n == 0 && !manager {
        return Err(Error::forbidden());
    }
    let posts=sqlx::query("SELECT d.body,d.state,u.name FROM member_discussions d JOIN users u ON u.id=d.user_id WHERE d.group_id=$1 AND (d.state='approved' OR d.user_id=$2) ORDER BY d.created_at DESC,d.id DESC LIMIT 40").bind(&id).bind(&s.user.id).fetch_all(&app.db.pool).await?;
    member_page(&app,&g.get::<String,_>("title"),html!{(view::heading("Community",&g.get::<String,_>("title"),"Share with your group. New contributions are reviewed before other members see them."))
 @if manager {section class="panel" {h2 {"Organization seats"}p {"Seat limit: " (g.get::<i64,_>("seat_limit"))}form method="post" {(view::csrf(&s))(hidden("operation","seat"))label {"Existing member email" input type="email" name="email" required maxlength="254";}button name="decision" value="add" {"Add seat"}button name="decision" value="remove" class="quiet" {"Remove seat"}}}}
 @if n>0 {section class="panel" {h2 {"Contribute"}form method="post" {(view::csrf(&s))(hidden("operation","discuss"))label {"Message" textarea name="body" required maxlength="4000" {}}button {"Submit for review"}}}}
 section class="panel" {h2 {"Group discussion"}@for p in posts{article {h3 {(p.get::<String,_>("name"))}p {(p.get::<String,_>("body"))}span class="status" {(p.get::<String,_>("state"))}}}}
 }).await
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct GroupAction {
    csrf: String,
    operation: String,
    email: String,
    decision: String,
    body: String,
}
async fn group_action(
    State(app): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(i): Form<GroupAction>,
) -> Result<Redirect> {
    let s = auth::session(&app, &h).await?;
    auth::csrf(&s, &i.csrf)?;
    match i.operation.as_str() {
        "seat" => seat(&app, &s, &id, &i.email, i.decision == "remove").await?,
        "discuss" => discuss(&app, &s, &id, &i.body).await?,
        _ => return Err(Error::invalid("Unknown group operation.")),
    }
    Ok(Redirect::to(&format!("/members/groups/{id}")))
}
async fn gift_page(
    State(app): State<App>,
    h: HeaderMap,
    Path(token): Path<String>,
) -> Result<Html<String>> {
    let s = auth::session(&app, &h).await?;
    let key:Option<String>=sqlx::query_scalar("SELECT entitlement FROM member_gifts WHERE token_hash=$1 AND claimed_by='' AND expires_at>$2").bind(auth::digest(token.as_bytes())).bind(now()).fetch_optional(&app.db.pool).await?;
    let key = key.ok_or_else(Error::not_found)?;
    member_page(&app,"Claim gift",html!{h1 {"Claim your gift"}p {"This grants " (&key) " access to your account."}form method="post" {(view::csrf(&s))button {"Claim gift"}}}).await
}
async fn redeem_gift(
    State(app): State<App>,
    h: HeaderMap,
    Path(token): Path<String>,
    Form(i): Form<Csrf>,
) -> Result<Redirect> {
    let s = auth::session(&app, &h).await?;
    auth::csrf(&s, &i.csrf)?;
    claim_gift(&app, &s, &token).await?;
    Ok(Redirect::to("/members"))
}

async fn referral_admin(State(app): State<App>, h: HeaderMap) -> Result<Html<String>> {
    let s = owner(&app, &h).await?;
    let users =
        sqlx::query("SELECT id,name FROM users WHERE role<>'disabled' ORDER BY name LIMIT 100")
            .fetch_all(&app.db.pool)
            .await?;
    let referrals=sqlx::query("SELECT r.id,r.title,r.visits,u.name FROM member_referrals r JOIN users u ON u.id=r.user_id ORDER BY r.created_at DESC LIMIT 100").fetch_all(&app.db.pool).await?;
    let commissions=sqlx::query("SELECT id,reference,amount_minor,currency,state FROM member_commissions ORDER BY created_at DESC LIMIT 40").fetch_all(&app.db.pool).await?;
    admin_page(&app,&s,"Referrals",html!{(view::heading("Members","Referrals & commission records","Local referral links and manually recorded obligations. Commerce will connect purchases and settlement."))p {a href="/admin/members" {"Members"}}section class="panel" {h2 {"Create a referral"}form method="post" {(view::csrf(&s))(hidden("operation","referral"))label {"Title" input name="title" required maxlength="160";}label {"Member" select name="user" {@for u in users{option value=(u.get::<String,_>("id")){(u.get::<String,_>("name"))}}}}button {"Create referral link"}}
 @for r in &referrals {p {a href=(format!("/r/{}",r.get::<String,_>("id"))){(r.get::<String,_>("title"))} " · " (r.get::<String,_>("name")) " · " (r.get::<i64,_>("visits")) " visits (not unique visitors)"}}}
 section class="panel" {h2 {"Record a commission"}form method="post" {(view::csrf(&s))(hidden("operation","commission"))label {"Referral" select name="id" {@for r in &referrals{option value=(r.get::<String,_>("id")){(r.get::<String,_>("title"))}}}}label {"Unique reference" input name="title" required maxlength="160";}label {"Amount in minor units" input type="number" name="amount" min="0" max="1000000000000" required;}label {"Currency code" input name="currency" required minlength="3" maxlength="3" placeholder="USD";}button {"Record commission"}}
 @for c in commissions {form method="post" class="toolbar" {(view::csrf(&s))(hidden("operation","void"))(hidden("id",&c.get::<String,_>("id")))span {(c.get::<String,_>("reference")) " · " (c.get::<i64,_>("amount_minor")) " " (c.get::<String,_>("currency")) " · " (c.get::<String,_>("state"))}button class="quiet" {"Void record"}}}}
 }).await
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct ReferralAction {
    csrf: String,
    operation: String,
    title: String,
    user: String,
    id: String,
    amount: i64,
    currency: String,
}
async fn referral_action(
    State(app): State<App>,
    h: HeaderMap,
    Form(i): Form<ReferralAction>,
) -> Result<Redirect> {
    let s = owner(&app, &h).await?;
    auth::csrf(&s, &i.csrf)?;
    match i.operation.as_str() {
        "referral" => {
            super::referrals::create(&app, &i.user, &i.title).await?;
        }
        "commission" => {
            super::referrals::commission(&app, &i.id, &i.title, i.amount, &i.currency).await?;
        }
        "void" => {
            sqlx::query("UPDATE member_commissions SET state='void' WHERE id=$1")
                .bind(i.id)
                .execute(&app.db.pool)
                .await?;
        }
        _ => return Err(Error::invalid("Unknown referral operation.")),
    }
    Ok(Redirect::to("/admin/members/referrals"))
}
async fn identity_admin(State(app): State<App>, h: HeaderMap) -> Result<Html<String>> {
    let s = owner(&app, &h).await?;
    let users =
        sqlx::query("SELECT id,name FROM users WHERE role<>'disabled' ORDER BY name LIMIT 100")
            .fetch_all(&app.db.pool)
            .await?;
    let bindings=sqlx::query("SELECT i.issuer,i.subject,u.name FROM member_identities i JOIN users u ON u.id=i.user_id ORDER BY u.name LIMIT 100").fetch_all(&app.db.pool).await?;
    admin_page(&app,&s,"Identity bindings",html!{(view::heading("Members","Identity bindings","Bind an approved provider's stable subject to an existing local account. Email matching never grants access."))p {a href="/admin/members" {"Members"}}@if !app.config.identity.enabled{p {"Optional provider sign-in is disabled. Local sign-in and learning continue independently. Configure the provider through the site's configuration file."}}@else{section class="panel" {h2 {"Add binding"}p {"Issuer: " (&app.config.identity.issuer)}form method="post" {(view::csrf(&s))label {"Member" select name="user" {@for u in users{option value=(u.get::<String,_>("id")){(u.get::<String,_>("name"))}}}}label {"Provider subject" input name="subject" required maxlength="255";}button {"Bind identity"}}}}section class="panel" {h2 {"Current bindings"}@for i in bindings{form action="/admin/members" method="post" {(view::csrf(&s))(hidden("operation","identity-remove"))(hidden("title",&i.get::<String,_>("issuer")))(hidden("key",&i.get::<String,_>("subject")))p {(i.get::<String,_>("name")) " · " (i.get::<String,_>("issuer")) " · " (i.get::<String,_>("subject"))}button class="quiet" {"Remove binding and revoke account sessions"}}}}}).await
}
#[derive(Deserialize)]
struct Binding {
    csrf: String,
    user: String,
    subject: String,
}
async fn identity_binding(
    State(app): State<App>,
    h: HeaderMap,
    Form(i): Form<Binding>,
) -> Result<Redirect> {
    let s = owner(&app, &h).await?;
    auth::csrf(&s, &i.csrf)?;
    if !app.config.identity.enabled || i.subject.is_empty() || i.subject.len() > 255 {
        return Err(Error::invalid(
            "Configure the provider and check the subject.",
        ));
    }
    uuid(&i.user)?;
    sqlx::query("INSERT INTO member_identities(issuer,subject,user_id) VALUES($1,$2,$3)")
        .bind(&app.config.identity.issuer)
        .bind(i.subject)
        .bind(i.user)
        .execute(&app.db.pool)
        .await?;
    Ok(Redirect::to("/admin/members/identity"))
}
