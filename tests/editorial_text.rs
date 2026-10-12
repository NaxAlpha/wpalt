//! Real rendered-text observations, with declared boundaries rather than scores.
use wpalt::discovery::insights::text;

#[test]
fn phrase_locations_follow_actual_main_text_and_unicode_boundaries() {
    let page = text::parse("<head><title>Café guide</title><meta name='description' content='Café notes'></head><body><nav>Café navigation</nav><main><h1>Café</h1><p>A cafe\u{301} guide. A cat, cathedral and education.</p><a href='/notes'>Café resources</a><img src='/image' alt='Café garden'><p hidden>Café hidden</p><script>Café script</script><template>Café client template</template></main><footer>Café footer</footer></body>").unwrap();
    assert!(page.main_element);
    assert!(
        !page.body.contains("navigation")
            && !page.body.contains("hidden")
            && !page.body.contains("script")
            && !page.body.contains("template")
            && !page.body.contains("footer")
    );
    let observed = page.observe("CAFÉ").unwrap();
    assert_eq!(observed.unicode_version, "17.0.0");
    let count = |label| {
        observed
            .locations
            .iter()
            .find(|l| l.label == label)
            .unwrap()
            .occurrences
    };
    assert_eq!(count("title"), 1);
    assert_eq!(count("description"), 1);
    assert_eq!(count("main text"), 3);
    assert_eq!(count("headings"), 1);
    assert_eq!(count("image alternatives"), 1);
    assert_eq!(count("link labels"), 1);
    let cats = page.observe("cat").unwrap();
    assert_eq!(
        cats.locations
            .iter()
            .find(|l| l.label == "main text")
            .unwrap()
            .occurrences,
        1,
        "Do not match cathedral/education substrings"
    );
    let multilingual =
        text::parse("<main><p>Don't split 👩🏽‍💻 into letters. 中文。العربية! कक्ष.</p></main>")
            .unwrap();
    assert!(
        multilingual
            .observe("don't")
            .unwrap()
            .locations
            .iter()
            .any(|l| l.label == "main text" && l.occurrences == 1)
    );
    assert!(multilingual.observe("").unwrap().word_segments > 0);
    assert!(
        multilingual
            .observe("")
            .unwrap()
            .scope
            .contains("not dictionary")
    );
}

#[test]
fn malformed_or_oversized_advice_inputs_produce_no_partial_observation() {
    for phrase in [
        "one\ntwo",
        "++",
        "👩🏽‍💻",
        "one two three four five six seven eight nine",
    ] {
        assert!(text::focus(phrase).is_err(), "Refuse {phrase:?}");
    }
    assert!(text::focus(&"x".repeat(101)).is_err());
    assert!(
        text::parse(&format!(
            "<main><p>{}</p></main>",
            "x".repeat(512 * 1024 + 1)
        ))
        .is_err()
    );
    assert!(text::parse(&"<section>".repeat(100000)).is_err());
    let hidden = text::parse("<div hidden><main>Invisible chosen root</main></div><main><p>Visible actual root</p></main>").unwrap();
    assert!(!hidden.body.contains("Invisible"));
    assert!(hidden.body.contains("Visible actual"));
    assert!(
        text::focus(&"İ".repeat(50)).is_err(),
        "Lowercase expansion also observes its byte budget"
    );
    let fallback = text::parse("<body><p>Body fallback.</p></body>").unwrap();
    assert!(!fallback.main_element);
    assert_eq!(fallback.observe("").unwrap().word_segments, 2);
}
