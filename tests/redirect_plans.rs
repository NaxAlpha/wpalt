//! Supported local route semantics and concrete adversarial failure boundaries.
use std::collections::BTreeMap;
use wpalt::discovery::redirects::{Exact, Plan, Policy, Rule};

fn rule(id: &str, pattern: &str, target: &str) -> Rule {
    Rule {
        id: id.into(),
        label: id.into(),
        pattern: pattern.into(),
        target: target.into(),
        ..Rule::default()
    }
}
fn plan(rules: Vec<Rule>) -> Plan {
    let mut policy = Policy::default();
    policy.settings.rules = rules;
    Plan::new(policy, BTreeMap::new()).unwrap()
}
#[test]
fn numbered_capture_moves_only_a_whole_safe_local_path() {
    let p = plan(vec![rule("move", r"^/old/([a-z0-9-]+)$", "/new/${1}")]);
    let resolved = p.resolve("/old/public-story", |_| false).unwrap().unwrap();
    assert_eq!(resolved.destination, "/new/public-story");
    assert_eq!(resolved.code, 301);
    assert!(
        p.resolve("/other/old/public-story", |_| false)
            .unwrap()
            .is_none()
    );
    assert!(
        p.resolve("/old/public-story?key=private", |_| false)
            .unwrap()
            .is_none()
    );
    assert!(
        p.resolve("/old/public-story", |path| path == "/old/public-story")
            .unwrap()
            .is_none()
    );
    let p = plan(vec![rule("alternation", r"^/old|/other$", "/new")]);
    assert!(
        p.next("/old/unmatched-suffix").unwrap().is_none(),
        "Alternation cannot bypass whole-path admission"
    );
}
#[test]
fn ambiguous_regex_preview_obeys_stable_priority_and_exact_precedence() {
    let mut policy = Policy::default();
    let mut later = rule("later", r"^/old/(.*)$", "/second/$1");
    later.priority = 10;
    policy.settings.rules = vec![later, rule("first", r"^/old/(.*)$", "/first/$1")];
    let p = Plan::new(policy.clone(), BTreeMap::new()).unwrap();
    let hop = p.next("/old/story").unwrap().unwrap();
    assert_eq!(hop.target, "/first/story");
    assert_eq!(hop.matching_patterns, vec!["first", "later"]);
    let p = Plan::new(
        policy,
        BTreeMap::from([(
            "/old/story".into(),
            Exact {
                target: "/manual".into(),
                code: 302,
            },
        )]),
    )
    .unwrap();
    let hop = p.next("/old/story").unwrap().unwrap();
    assert_eq!(hop.target, "/manual");
    assert_eq!(hop.matching_patterns.len(), 2);
    assert_eq!(
        p.resolve("/old/story", |_| false).unwrap().unwrap().code,
        302
    );
}
#[test]
fn unsupported_patterns_missing_captures_and_dangerous_expansions_are_refused() {
    for r in [
        rule("lookaround", r"^/(?=secret).*$", "/new"),
        rule("backreference", r"^/(a)\1$", "/new"),
        rule("repetition", r"^/[a-z]{99999999}$", "/new"),
        rule("missing", r"^/old/(.*)$", "/new/$2"),
        rule("external", r"^/old/(.*)$", "https://outside.test/$1"),
        rule("credentials", r"^/old/(.*)$", "/account/$1"),
    ] {
        let mut policy = Policy::default();
        policy.settings.rules = vec![r];
        assert!(policy.validate().is_err());
    }
    let p = plan(vec![rule("capture", r"^/old/(.*)$", "/$1")]);
    for path in [
        "/old//outside.test",
        "/old/admin/users",
        "/old/../secret",
        "/old/a%2fsecret",
    ] {
        if wpalt::discovery::redirects::safe_path(path) {
            assert!(p.next(path).is_err(), "{path}");
        } else {
            assert!(p.next(path).unwrap().is_none(), "{path}");
        }
    }
    let mut policy = Policy::default();
    policy.settings.rules = (0..129)
        .map(|i| rule(&format!("rule_{i}"), r"^/old$", "/new"))
        .collect();
    assert!(policy.validate().is_err());
}
#[test]
fn finite_resolution_rejects_cycles_and_keeps_temporary_chain_semantics() {
    let p = plan(vec![
        rule("one", r"^/one$", "/two"),
        rule("two", r"^/two$", "/one"),
    ]);
    assert!(p.resolve("/one", |_| false).is_err());
    assert_eq!(
        p.resolve("/one", |path| path == "/two")
            .unwrap()
            .unwrap()
            .destination,
        "/two",
        "A real served target has authority even if a rule could match it"
    );
    let mut temporary = rule("temporary", r"^/two$", "/three");
    temporary.code = 302;
    let p = plan(vec![rule("one", r"^/one$", "/two"), temporary]);
    let r = p.resolve("/one", |_| false).unwrap().unwrap();
    assert_eq!(r.destination, "/three");
    assert_eq!(r.code, 302);
    assert_eq!(r.hops.len(), 2);
    let exact = (0..16)
        .map(|i| {
            (
                format!("/step-{i}"),
                Exact {
                    target: format!("/step-{}", i + 1),
                    code: 301,
                },
            )
        })
        .collect();
    assert!(
        Plan::new(Policy::default(), exact).is_err(),
        "A sixteenth hop must not pass static admission"
    );
}

#[test]
fn rendered_html_parser_finds_real_edges_and_refuses_partial_work() {
    use wpalt::discovery::rendered_links::parse;
    let parsed=parse(r##"<!doctype html><head><script id="script-target">const a='<a href="/false">'</script></head><body><nav><a href="/navigation">N</a></nav><a name="legacy"></a><main id="main"><a href="#main">Here</a><img src="/media/image" alt="A"><template><a href="/client-only">Later</a></template><footer><a href="https://outside.test/">Outside</a></footer></main></body>"##).unwrap();
    assert!(
        parsed.ids.contains("main")
            && parsed.ids.contains("legacy")
            && parsed.ids.contains("script-target")
    );
    let fragments =
        parse("<div id='percent%nothex'></div><div id='�'></div><a name='legacy anchor'></a>")
            .unwrap();
    for raw in ["%70ercent%nothex", "%FF", "legacy%20anchor", "%54OP"] {
        assert!(
            fragments.has_fragment(raw),
            "Browser-resolvable fragment: {raw}"
        );
    }
    assert!(!fragments.has_fragment("missing%nothex"));
    let long_id = "x".repeat(600);
    assert!(
        parse(&format!("<div id='{long_id}'></div>"))
            .unwrap()
            .ids
            .contains(&long_id)
    );
    assert!(
        parse(&"<section>".repeat(100000)).is_err(),
        "Deep markup is refused before DOM allocation"
    );
    assert!(
        parse(&"<div/>".repeat(100000)).is_err(),
        "HTML self-closing non-void tags cannot bypass preflight depth"
    );
    assert_eq!(parsed.links.len(), 4);
    assert_eq!(
        parsed.links[0],
        ("/navigation".into(), "a".into(), "navigation".into())
    );
    assert!(
        parsed
            .links
            .iter()
            .any(|(target, element, _)| target == "/media/image" && element == "img")
    );
    assert!(
        !parsed
            .links
            .iter()
            .any(|(target, _, _)| target == "/false" || target == "/client-only")
    );
    let too_many = "<a href='/x'>x</a>".repeat(20001);
    assert!(
        parse(&too_many).is_err(),
        "Overflow produces no successful partial edge graph"
    );
    assert!(parse(&"x".repeat(2 * 1024 * 1024 + 1)).is_err());
    assert!(
        parse(&format!(
            "{}text{}",
            "<section>".repeat(70),
            "</section>".repeat(70)
        ))
        .is_err()
    );
}
