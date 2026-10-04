//! Local loading controls; disabling optimization remains supported for diagnosis.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub scoped_theme_css: bool,
    pub preload_theme_css: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            scoped_theme_css: true,
            preload_theme_css: true,
        }
    }
}
