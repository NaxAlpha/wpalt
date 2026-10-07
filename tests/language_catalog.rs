//! A single reviewable catalog boundary cluster: fallback, grammar, direction and text safety.
use wpalt::platform::i18n::Catalog;
#[test]
fn interface_catalog_preserves_safe_fallback_and_language_grammar() {
    let english = Catalog::english();
    let french = Catalog::select("fr").unwrap();
    let japanese = Catalog::select("ja").unwrap();
    let arabic = Catalog::select("ar").unwrap();
    assert!(Catalog::select("ar\" onload=\"alert(1)").is_err());
    assert_eq!(arabic.direction(), "rtl");
    assert_eq!(japanese.direction(), "ltr");
    assert_eq!(
        french.text("language.policy_help"),
        english.text("language.policy_help")
    );
    assert!(french.missing().contains(&"language.policy_help"));
    assert_eq!(japanese.text("unknown.message"), "Translation unavailable");
    let escaped = french
        .value("language.saved_revision", "<script>{value}</script>")
        .into_string();
    assert!(!escaped.contains("<script>"));
    assert!(escaped.contains("&lt;script&gt;{value}&lt;/script&gt;"));
    assert_eq!(english.content_count(1), "1 content item");
    assert_eq!(english.content_count(2), "2 content items");
    assert_eq!(french.content_count(0), "0 contenu");
    assert_eq!(french.content_count(1200), "1\u{202f}200 contenus");
    assert_eq!(japanese.content_count(1200), "1,200 件のコンテンツ");
    assert_eq!(arabic.content_count(0), "لا توجد عناصر");
    assert_eq!(arabic.content_count(2), "عنصران");
    assert_eq!(arabic.content_count(3), "٣ عناصر");
    assert_eq!(arabic.content_count(11), "١١ عنصرًا");
    assert_eq!(arabic.content_count(100), "١٠٠ عنصر");
    assert!(english.scope().contains("other screens"));
}
