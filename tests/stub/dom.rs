//! DOM dumps and artifact numbering.

use super::*;

#[test]
fn i_dump_the_dom_writes_the_page_source_into_artifacts_dir() {
    let _guard = serial();
    let stub = start_stub(StubState {
        source: "<html><body><h1>Hi</h1></body></html>".into(),
        ..Default::default()
    });
    let handle = init(&stub, json!({}));
    let dir = artifacts();
    let target = dir.path().join("000001");
    assert_eq!(
        dispatch(handle, DUMP_DOM, &[], None, &target)["status"],
        "passed"
    );
    let html = std::fs::read_to_string(target.join("dom-1.html")).expect("dom-1.html is written");
    assert_eq!(html, "<html><body><h1>Hi</h1></body></html>");
    drop_instance(handle);
}

#[test]
fn i_dump_the_dom_of_an_element_writes_its_outer_html() {
    let _guard = serial();
    let stub = start_stub(StubState {
        elements: vec![element("#cart", "")],
        script_result: json!("<div id=\"cart\">2 items</div>"),
        ..Default::default()
    });
    let handle = init(&stub, json!({}));
    let dir = artifacts();
    let target = dir.path().join("000001");
    assert_eq!(
        dispatch(handle, DUMP_DOM_OF, &["#cart"], None, &target)["status"],
        "passed"
    );
    let html = std::fs::read_to_string(target.join("dom-1.html")).expect("dom-1.html is written");
    assert_eq!(html, "<div id=\"cart\">2 items</div>");
    let calls = calls(&stub);
    let script = calls
        .iter()
        .find(|c| c.starts_with("POST /session/s1/execute/sync") && c.contains("outerHTML"))
        .expect("the outerHTML script ran");
    assert!(
        script.contains(ELEMENT_KEY) && script.contains("\"e0\""),
        "{script}"
    );
    drop_instance(handle);
}

#[test]
fn dumping_the_dom_of_a_missing_element_fails_at_once_and_never_waits() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let handle = init(&stub, json!({"find_timeout_secs": 5, "on_failure": "none"}));
    let dir = artifacts();
    let target = dir.path().join("000001");
    let started = std::time::Instant::now();
    let r = dispatch(handle, DUMP_DOM_OF, &["#cart"], None, &target);
    assert_eq!(
        r["status"], "fatal",
        "an action may not answer not_yet: {r}"
    );
    assert!(r["error"].as_str().expect("error").contains("#cart"), "{r}");
    assert!(started.elapsed() < std::time::Duration::from_millis(500));
    assert!(!target.exists());
    drop_instance(handle);
}

#[test]
fn on_failure_dom_adds_the_page_source_file_to_a_failure() {
    let _guard = serial();
    let stub = start_stub(StubState {
        source: "<html>broken</html>".into(),
        ..Default::default()
    });
    let handle = init(&stub, json!({"find_timeout_secs": 0, "on_failure": "dom"}));
    let dir = artifacts();
    let target = dir.path().join("000001");
    let r = dispatch(handle, PRESS, &["Pay"], None, &target);
    assert_eq!(r["status"], "fatal");
    let d = r["diagnostics"].as_array().expect("diagnostics");
    let titles: Vec<&str> = d.iter().map(|x| x["title"].as_str().expect("t")).collect();
    assert_eq!(titles, vec!["Page", "DOM", "WebDriver"], "{r}");
    let path = d[1]["path"]
        .as_str()
        .expect("the DOM diagnostic carries a path");
    assert_eq!(
        std::fs::read_to_string(path).expect("the file exists"),
        "<html>broken</html>"
    );
    assert!(
        d[2]["content"]
            .as_str()
            .expect("http")
            .contains("POST session/s1/element"),
        "the failed find stays the last exchange: {r}"
    );
    drop_instance(handle);
}

#[test]
fn artifact_names_count_up_within_a_scenario_and_start_over_after_a_reset() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let handle = init(&stub, json!({}));
    let dir = artifacts();
    let names = |n: u32, step: u32| {
        let target = dir.path().join(format!("{n:06}"));
        assert_eq!(
            dispatch(handle, step, &[], None, &target)["status"],
            "passed"
        );
        let mut files: Vec<String> = std::fs::read_dir(&target)
            .expect("written")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        files.sort();
        files
    };
    assert_eq!(names(1, SCREENSHOT), ["screenshot-1.png"]);
    assert_eq!(names(2, SCREENSHOT), ["screenshot-2.png"]);
    assert_eq!(
        names(3, DUMP_DOM),
        ["dom-1.html"],
        "each kind counts on its own"
    );
    assert_eq!(reset(handle)["ok"], true);
    assert_eq!(names(4, SCREENSHOT), ["screenshot-1.png"]);
    drop_instance(handle);
}
