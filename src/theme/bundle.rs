//! Portable native theme + exactly referenced local fonts. No executable files.
use super::{Package, assets, font};
use crate::{
    App,
    error::{Error, Result},
    model::Session,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sqlx::{Any, QueryBuilder, Row};
use std::collections::BTreeSet;
pub const MAX_BYTES: usize = 24 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub format: u32,
    pub package: Package,
    pub fonts: Vec<Font>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Font {
    pub definition: assets::Definition,
    pub data: String,
}
pub(super) struct Admitted {
    pub definition: assets::Definition,
    pub data: Vec<u8>,
}
impl Bundle {
    fn inspect(self) -> Result<(Package, Vec<Admitted>)> {
        if self.format != 1 || self.fonts.len() > 8 {
            return Err(Error::invalid(
                "Unsupported native theme bundle or font count.",
            ));
        }
        let expected: BTreeSet<_> = self
            .package
            .fonts
            .values()
            .map(|face| face.asset.clone())
            .collect();
        let mut seen = BTreeSet::new();
        let mut total = 0usize;
        let mut fonts = Vec::new();
        for value in self.fonts {
            value.definition.validate()?;
            let id = &value.definition.inspection.sha256;
            if !expected.contains(id)
                || !seen.insert(id.clone())
                || value.data.len() > (font::MAX_FONT_BYTES.div_ceil(3) * 4)
            {
                return Err(Error::invalid(
                    "Bundle must contain exactly its referenced immutable font assets.",
                ));
            }
            let data = STANDARD
                .decode(value.data)
                .map_err(|_| Error::invalid("Invalid font encoding."))?;
            total += data.len();
            if total > assets::MAX_TOTAL_BYTES as usize
                || font::inspect(&data)? != value.definition.inspection
            {
                return Err(Error::invalid(
                    "Font bytes do not match the inspected bundle metadata.",
                ));
            }
            fonts.push(Admitted {
                definition: value.definition,
                data,
            });
        }
        if expected != seen {
            return Err(Error::invalid("Bundle is missing a declared local font."));
        }
        Ok((self.package, fonts))
    }
}
pub async fn export(app: &App, id: &str, draft: bool) -> Result<Bundle> {
    let package = super::load(app, id, draft).await?.package;
    let ids: BTreeSet<_> = package
        .fonts
        .values()
        .map(|face| face.asset.as_str())
        .collect();
    let mut fonts = Vec::new();
    if !ids.is_empty() {
        let _permit = app
            .media_reads
            .acquire()
            .await
            .map_err(|_| Error::invalid("Font reads unavailable."))?;
        let mut q = QueryBuilder::<Any>::new(
            "SELECT id,definition,size,data FROM theme_assets WHERE id IN (",
        );
        let mut list = q.separated(",");
        for id in &ids {
            list.push_bind(*id);
        }
        list.push_unseparated(") ORDER BY id");
        for row in app.db.fetch_builder(&mut q).await? {
            let definition = assets::Definition::parse(&row.get::<String, _>("definition"))?;
            let data: Vec<u8> = row.get("data");
            if data.len() > font::MAX_FONT_BYTES
                || data.len() as i64 != row.get::<i64, _>("size")
                || crate::auth::digest(&data) != row.get::<String, _>("id")
            {
                return Err(Error::invalid("Stored font integrity failed."));
            }
            fonts.push(Font {
                definition,
                data: STANDARD.encode(data),
            });
        }
        if fonts.len() != ids.len() {
            return Err(Error::invalid("Theme is missing a local font."));
        }
    }
    Ok(Bundle {
        format: 1,
        package,
        fonts,
    })
}
pub async fn import(
    app: &App,
    actor: Option<&Session>,
    id: &str,
    bytes: Vec<u8>,
    version: i64,
    publish: bool,
) -> Result<i64> {
    super::current_owner(app, actor).await?;
    if bytes.len() > MAX_BYTES {
        return Err(Error::invalid("Theme bundles are limited to 24 MiB."));
    }
    let _permit = app
        .media_work
        .acquire()
        .await
        .map_err(|_| Error::invalid("Font inspection unavailable."))?;
    let (package, fonts) = tokio::task::spawn_blocking(move || {
        let bundle: Bundle = serde_json::from_slice(&bytes)
            .map_err(|_| Error::invalid("Invalid native theme bundle."))?;
        bundle.inspect()
    })
    .await
    .map_err(|_| Error::invalid("Theme bundle inspection failed."))??;
    drop(_permit);
    super::save_checked(app, id, package, version, publish, actor, &fonts).await
}
