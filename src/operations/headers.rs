//! Compiled owner controls without arbitrary header strings or weakened script CSP.
use axum::http::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Referrer {
    #[default]
    StrictOriginWhenCrossOrigin,
    NoReferrer,
    SameOrigin,
}
impl Referrer {
    fn value(&self) -> &'static str {
        match self {
            Self::StrictOriginWhenCrossOrigin => "strict-origin-when-cross-origin",
            Self::NoReferrer => "no-referrer",
            Self::SameOrigin => "same-origin",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub referrer: Referrer,
    pub hsts_seconds: u64,
    pub hsts_include_subdomains: bool,
    pub isolate_opener: bool,
    pub same_origin_resources: bool,
    pub upgrade_insecure_requests: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            referrer: Referrer::default(),
            hsts_seconds: 31536000,
            hsts_include_subdomains: false,
            isolate_opener: true,
            same_origin_resources: false,
            upgrade_insecure_requests: false,
        }
    }
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.hsts_seconds <= 63072000,
            "HSTS duration must be 0..63,072,000 seconds"
        );
        Ok(())
    }
}
#[derive(Clone)]
pub struct Policy {
    csp: HeaderValue,
    form_csp: HeaderValue,
    preview_csp: HeaderValue,
    hsts: Option<HeaderValue>,
    referrer: HeaderValue,
    isolate_opener: bool,
    same_origin_resources: bool,
}
impl Policy {
    pub fn compile(config: &crate::config::Config) -> Self {
        let secure = config.secure_cookie();
        let policy = &config.headers;
        let csp = |profile: &str| {
            let suffix = if secure && policy.upgrade_insecure_requests {
                "; upgrade-insecure-requests"
            } else {
                ""
            };
            format!("{profile}{suffix}")
                .parse()
                .expect("static CSP and validated controls")
        };
        let hsts = secure.then(|| {
            format!(
                "max-age={}{}",
                policy.hsts_seconds,
                if policy.hsts_include_subdomains {
                    "; includeSubDomains"
                } else {
                    ""
                }
            )
            .parse()
            .expect("integer HSTS controls")
        });
        Self {
            csp: csp(
                "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
            ),
            form_csp: csp(
                "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; object-src 'none'; base-uri 'none'; frame-ancestors 'self'; form-action 'self'",
            ),
            preview_csp: csp(
                "default-src 'self'; script-src 'none'; style-src 'self'; img-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'self'; form-action 'none'",
            ),
            hsts,
            referrer: HeaderValue::from_static(policy.referrer.value()),
            isolate_opener: policy.isolate_opener,
            same_origin_resources: policy.same_origin_resources,
        }
    }
    pub fn apply(&self, route: &str, headers: &mut HeaderMap) {
        headers.insert(
            "x-content-type-options",
            HeaderValue::from_static("nosniff"),
        );
        headers.insert("referrer-policy", self.referrer.clone());
        if [
            "/audience/",
            "/registration/",
            "/members/gifts",
            "/members/identity",
        ]
        .iter()
        .any(|prefix| route.starts_with(prefix))
        {
            headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
        }
        let csp = if ["/admin/design/{id}/preview", "/admin/preview/{id}"].contains(&route) {
            &self.preview_csp
        } else if route == "/forms/{id}" {
            &self.form_csp
        } else {
            &self.csp
        };
        headers.insert("content-security-policy", csp.clone());
        headers.insert(
            "permissions-policy",
            HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
        );
        if let Some(hsts) = &self.hsts {
            headers.insert("strict-transport-security", hsts.clone());
        }
        if self.isolate_opener {
            headers.insert(
                "cross-origin-opener-policy",
                HeaderValue::from_static("same-origin"),
            );
        }
        let private_surface = [
            "/admin",
            "/api/admin",
            "/account",
            "/api/members",
            "/members",
            "/commerce",
        ]
        .iter()
        .any(|prefix| route.starts_with(prefix));
        headers.insert(
            "cross-origin-resource-policy",
            HeaderValue::from_static(if self.same_origin_resources || private_surface {
                "same-origin"
            } else {
                "cross-origin"
            }),
        );
        headers.remove("server");
        headers.remove("x-powered-by");
    }
}
