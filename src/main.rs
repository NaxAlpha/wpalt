use clap::{Parser, Subcommand};
use fs2::FileExt;
use std::{io::Read, path::PathBuf};
use wpalt::{
    App, auth, backup,
    config::Config,
    content,
    model::{PostInput, Session, User},
};

#[derive(Parser)]
#[command(
    version,
    about = "Owner-controlled publishing, from one Rust application"
)]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    database_url: Option<String>,
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    listen: Option<std::net::SocketAddr>,
    #[arg(long, global = true)]
    base_url: Option<String>,
    #[arg(long, global = true)]
    debug: bool,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Initialize a site. Read the initial password from stdin, never an argv flag.
    Init {
        #[arg(long)]
        admin_email: String,
        #[arg(long, default_value = "Site owner")]
        admin_name: String,
    },
    /// Run the web server and durable publication scheduler.
    Serve,
    /// Show validated effective configuration with credentials redacted.
    Config,
    /// Create an offline database-and-media snapshot in a new private file.
    Backup { output: PathBuf },
    /// Restore a snapshot into an EMPTY database/data directory, with the server stopped.
    Restore { input: PathBuf },
    /// Add an account while the server is stopped. Password is read from stdin.
    UserAdd {
        #[arg(long)]
        email: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "editor")]
        role: String,
    },
    /// Offline portable theme operations, using the same validation as the studio.
    Theme {
        #[command(subcommand)]
        command: ThemeCommand,
    },
    /// Populate an initialized empty site with reviewable example content.
    SeedDemo {
        #[arg(long, default_value_t = 8)]
        posts: u32,
    },
}
#[derive(Subcommand)]
enum ThemeCommand {
    Import {
        id: String,
        input: PathBuf,
        #[arg(long)]
        publish: bool,
    },
    Export {
        id: String,
        output: PathBuf,
        #[arg(long)]
        draft: bool,
    },
    Publish {
        id: String,
    },
    Activate {
        id: String,
    },
}
fn password() -> anyhow::Result<String> {
    let mut value = String::new();
    std::io::stdin().take(258).read_to_string(&mut value)?;
    let password = value.trim_end_matches(['\r', '\n']).to_owned();
    anyhow::ensure!(
        (12..=256).contains(&password.len()),
        "password from stdin must be 12..256 bytes"
    );
    Ok(password)
}
fn lock(config: &Config) -> anyhow::Result<std::fs::File> {
    config.prepare_directories()?;
    let mut options = std::fs::OpenOptions::new();
    options.create(true).read(true).write(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(config.data_dir.join(".wpalt.lock"))?;
    file.try_lock_exclusive().map_err(|_| {
        anyhow::anyhow!(
            "this data directory is already in use; stop the server before offline operations"
        )
    })?;
    Ok(file)
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut config = Config::load(cli.config.as_deref())?;
    if let Some(v) = cli.database_url {
        config.database_url = v;
    }
    if let Some(v) = cli.data_dir {
        config.data_dir = v;
    }
    if let Some(v) = cli.listen {
        config.listen = v;
    }
    if let Some(v) = cli.base_url {
        config.base_url = v;
    }
    if cli.debug {
        config.debug = true;
    }
    config.validate()?;
    if matches!(cli.command, Command::Config) {
        println!("{}", serde_json::to_string_pretty(&config.redacted())?);
        return Ok(());
    }
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(if config.debug {
            "wpalt=debug,sqlx=debug"
        } else {
            "wpalt=info,sqlx=warn"
        })
        .with_writer(std::io::stderr)
        .init();
    let _lock = lock(&config)?;
    let app = App::open(config).await?;
    match cli.command {
        Command::Init {
            admin_email,
            admin_name,
        } => {
            auth::initialize(&app, &admin_email, &admin_name, &password()?).await?;
            println!(
                "Site initialized. Run wpalt serve, then visit {}/login",
                app.config.origin()
            );
        }
        Command::Config => unreachable!(),
        Command::Backup { output } => {
            let bytes = backup::capture(&app)
                .await
                .map_err(|e| anyhow::anyhow!(e.1))?;
            backup::write_private(&output, &bytes)?;
            println!(
                "Created private snapshot ({} bytes). Keep an independent secure copy.",
                bytes.len()
            );
        }
        Command::Restore { input } => {
            let file = std::fs::File::open(&input)?;
            anyhow::ensure!(
                file.metadata()?.len() <= app.config.max_backup_bytes as u64,
                "backup exceeds configured limit"
            );
            let mut bytes = Vec::new();
            file.take(app.config.max_backup_bytes as u64 + 1)
                .read_to_end(&mut bytes)?;
            backup::restore(&app, &bytes)
                .await
                .map_err(|e| anyhow::anyhow!(e.1))?;
            optimize_bulk(&app).await;
            println!("Restored site. Existing sessions are not restored; sign in again.");
        }
        Command::UserAdd { email, name, role } => {
            auth::add_user(&app, &email, &name, &role, &password()?).await?;
            println!("Account created.");
        }
        Command::Theme { command } => {
            use wpalt::{schema, theme};
            let result: wpalt::error::Result<()> = async {
                match command {
                    ThemeCommand::Import { id, input, publish } => {
                        let file = std::fs::File::open(input)?;
                        if file.metadata()?.len() > 256 * 1024 {
                            return Err(wpalt::error::Error::invalid(
                                "Theme package exceeds 256 KiB.",
                            ));
                        }
                        let mut raw = String::new();
                        file.take(256 * 1024 + 1).read_to_string(&mut raw)?;
                        let package =
                            theme::Package::parse(&raw, &schema::Registry::load(&app).await?)?;
                        let version: Option<i64> =
                            sqlx::query_scalar("SELECT version FROM themes WHERE id=$1")
                                .bind(&id)
                                .fetch_optional(&app.db.pool)
                                .await?;
                        theme::save(&app, &id, package, version.unwrap_or(0), publish).await?;
                    }
                    ThemeCommand::Export { id, output, draft } => {
                        let stored = theme::load(&app, &id, draft).await?;
                        let bytes = serde_json::to_vec_pretty(&stored.package)
                            .map_err(|_| wpalt::error::Error::invalid("Invalid theme."))?;
                        backup::write_private(&output, &bytes).map_err(|_| {
                            wpalt::error::Error::invalid("Cannot create a new private export file.")
                        })?;
                    }
                    ThemeCommand::Publish { id } => {
                        let stored = theme::load(&app, &id, true).await?;
                        theme::save(&app, &id, stored.package, stored.version, true).await?;
                    }
                    ThemeCommand::Activate { id } => theme::activate(&app, &id).await?,
                }
                Ok(())
            }
            .await;
            result.map_err(|e| anyhow::anyhow!(e.1))?;
            println!("Theme operation completed.");
        }
        Command::SeedDemo { posts } => seed(&app, posts).await?,
        Command::Serve => {
            app.db.settings().await.map_err(|_| {
                anyhow::anyhow!("site is not initialized; run wpalt init or restore first")
            })?;
            let scheduled = app.clone();
            let job = tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(
                    scheduled.config.scheduler_seconds,
                ));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    if content::publish_due(&scheduled).await.is_err() {
                        tracing::error!(event = "scheduler_tick_failed");
                    }
                    if let Err(_e) = cleanup(&scheduled).await {
                        tracing::error!(event = "maintenance_tick_failed");
                    }
                }
            });
            let listener = tokio::net::TcpListener::bind(app.config.listen).await?;
            tracing::info!(event="server_started",listen=%app.config.listen);
            axum::serve(
                listener,
                wpalt::web::router(app)
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown())
            .await?;
            job.abort();
        }
    }
    Ok(())
}
async fn shutdown() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("signal handler");
        tokio::select! {_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
async fn cleanup(app: &App) -> wpalt::error::Result<()> {
    let _guard = app.mutations.lock().await;
    sqlx::query("DELETE FROM sessions WHERE expires_at<$1")
        .bind(wpalt::now())
        .execute(&app.db.pool)
        .await?;
    sqlx::query("DELETE FROM comment_limits WHERE last_at<$1")
        .bind(wpalt::now() - 86400)
        .execute(&app.db.pool)
        .await?;
    Ok(())
}
async fn seed(app: &App, count: u32) -> anyhow::Result<()> {
    use sqlx::Row;
    anyhow::ensure!((1..=10000).contains(&count), "demo posts must be 1..10000");
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM posts")
        .fetch_one(&app.db.pool)
        .await?;
    anyhow::ensure!(n == 0, "demo seed requires an empty content library");
    let r = sqlx::query(
        "SELECT id,email,name,role FROM users WHERE role='admin' ORDER BY created_at LIMIT 1",
    )
    .fetch_one(&app.db.pool)
    .await?;
    let s = Session {
        user: User {
            id: r.get("id"),
            email: r.get("email"),
            name: r.get("name"),
            role: r.get("role"),
        },
        csrf: String::new(),
        hash: String::new(),
    };
    let default_language = wpalt::discovery::load(app)
        .await
        .map_err(|e| anyhow::anyhow!(e.1))?
        .0
        .default_language;
    let stories = [
        (
            "A quieter place on the web",
            "Our digital spaces should feel like places we own. A little slower, more considered, and built around the stories we want to tell.",
        ),
        (
            "Notes from the garden",
            "Good things take root with a little patience. These are the things we have been making, reading and learning.",
        ),
        (
            "Building with intention",
            "A small website can do a lot. Start with clear words, thoughtful structure and tools that stay out of the way.",
        ),
        (
            "A field guide to independent publishing",
            "Own your words, your audience and your archives. Publishing should not require a collection of accounts to keep your site running.",
        ),
    ];
    sqlx::query("UPDATE settings SET title='The Local Journal',description='Ideas, field notes and a quieter corner of the independent web.',navigation=$1 WHERE id=1").bind(r#"[{"label":"About","url":"/about"}]"#).execute(&app.db.pool).await?;
    for i in 0..count {
        let (title, lead) = stories[i as usize % stories.len()];
        content::save(app,&s,None,PostInput{locale:default_language.clone(),translation_group:String::new(),seo:"{}".into(),title:if i<4{title.into()}else{format!("{title} · {}",i+1)},slug:format!("journal-{}",i+1),kind:"post".into(),body:format!("{lead}\n\n## Room to think\n\nThis is a working wpalt example: structured content, a shared theme and a publishing workflow you can run on your own server.\n\n- Draft and preview before publishing.\n- Keep unfinished changes away from your live pages.\n- Back up your content and take it with you.\n\n> Useful tools should make good work easier.\n\n### What comes next\n\nExplore the administration panel, edit this story, and switch between the Paper and Ink themes."),fields:serde_json::json!({"subtitle":lead,"reading_minutes":3,"featured":i==0}).to_string(),blocks:r#"[{"kind":"callout","text":"Your content. Your server. No vendor account."}]"#.into(),categories:"Field notes".into(),tags:"Independent web, Publishing".into(),taxonomies:"{}".into(),version:0,action:"publish".into(),publish_at:0,csrf:String::new()}).await.map_err(|e|anyhow::anyhow!(e.1))?;
    }
    content::save(app,&s,None,PostInput{locale:default_language.clone(),translation_group:String::new(),seo:"{}".into(),title:"About the journal".into(),slug:"about".into(),kind:"page".into(),body:"# A home for our ideas\n\nThis journal is an example website running entirely on wpalt. It is small by design, with room to grow.\n\nUse the admin panel to change the navigation, theme and content. Everything here stays on your server.".into(),fields:"{}".into(),blocks:"[]".into(),categories:String::new(),tags:String::new(),taxonomies:"{}".into(),version:0,action:"publish".into(),publish_at:0,csrf:String::new()}).await.map_err(|e|anyhow::anyhow!(e.1))?;
    optimize_bulk(app).await;
    println!("Created {count} stories and an about page.");
    Ok(())
}

async fn optimize_bulk(app: &App) {
    if let Err(error) = app.db.optimize_after_bulk_write().await {
        tracing::warn!(event = "bulk_maintenance_failed", error = %error,
            "Content was committed; database maintenance should be retried by the operator");
    }
}
