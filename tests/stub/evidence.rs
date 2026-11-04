//! Scripts, screenshots, waiting and what a failed step attaches.

use super::*;

#[test]
fn scripts_publish_their_result_as_text() {
    let _guard = serial();
    let stub = start_stub(StubState {
        script_result: json!({"a": 1}),
        ..Default::default()
    });
    let handle = init(&stub, json!({}));
    let dir = artifacts();
    let r = dispatch(handle, SCRIPT, &["return document.title"], None, dir.path());
    assert_eq!(r["vars"]["script_result"], "{\"a\":1}");
    stub.state.lock().expect("state").script_result = Value::Null;
    let r = dispatch(handle, SCRIPT_DOC, &[], Some("return null"), dir.path());
    assert_eq!(r["status"], "passed");
    assert_eq!(r["vars"]["script_result"], "");
    assert!(
        calls(&stub)
            .iter()
            .any(|c| c.contains("\"script\":\"return null\""))
    );
    let r = dispatch(handle, SCRIPT_DOC, &[], None, dir.path());
    assert_eq!(
        r["status"], "fatal",
        "the doc string form without a doc string"
    );
    drop_instance(handle);
}

#[test]
fn i_take_a_screenshot_writes_a_png_into_artifacts_dir() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let handle = init(&stub, json!({}));
    let dir = artifacts();
    let target = dir.path().join("000001");
    assert_eq!(
        dispatch(handle, SCREENSHOT, &[], None, &target)["status"],
        "passed"
    );
    let png = std::fs::read(target.join("screenshot-1.png"))
        .expect("the plugin creates artifacts_dir and writes the PNG");
    assert_eq!(&png[..4], b"\x89PNG");
    drop_instance(handle);
}

#[test]
fn an_action_waits_for_its_element_and_gives_up_after_find_timeout() {
    let _guard = serial();
    let stub = start_stub(StubState {
        elements: vec![element("'Pay'", "Pay")],
        not_found_first: 3,
        ..Default::default()
    });
    let handle = init(&stub, json!({"find_timeout_secs": 2, "on_failure": "none"}));
    let dir = artifacts();
    let started = std::time::Instant::now();
    assert_eq!(
        dispatch(handle, PRESS, &["Pay"], None, dir.path())["status"],
        "passed"
    );
    assert!(
        started.elapsed() >= std::time::Duration::from_millis(100),
        "three misses mean at least one sleep"
    );
    stub.state.lock().expect("state").not_found_first = 1000;
    let started = std::time::Instant::now();
    let r = dispatch(handle, PRESS, &["Pay"], None, dir.path());
    assert_eq!(r["status"], "fatal");
    assert!(
        r["error"].as_str().expect("error").contains("within 2s"),
        "{r}"
    );
    assert!(started.elapsed() >= std::time::Duration::from_secs(2));
    drop_instance(handle);
}

#[test]
fn an_assertion_looks_once_and_never_waits() {
    let _guard = serial();
    let stub = start_stub(StubState {
        elements: vec![element("h1", "x")],
        not_found_first: 1,
        ..Default::default()
    });
    let handle = init(&stub, json!({"find_timeout_secs": 5, "on_failure": "none"}));
    let dir = artifacts();
    let started = std::time::Instant::now();
    assert_eq!(
        dispatch(handle, VISIBLE, &["h1"], None, dir.path())["status"],
        "not_yet"
    );
    assert!(started.elapsed() < std::time::Duration::from_millis(500));
    drop_instance(handle);
}

#[test]
fn a_failure_carries_the_page_a_screenshot_and_the_last_exchange() {
    let _guard = serial();
    let stub = start_stub(StubState {
        url: "http://app.test/orders/new".into(),
        title: "New order".into(),
        ..Default::default()
    });
    let handle = init(&stub, json!({"find_timeout_secs": 0}));
    let dir = artifacts();
    let target = dir.path().join("000007");
    let r = dispatch(handle, PRESS, &["Pay"], None, &target);
    assert_eq!(r["status"], "fatal");
    let d = r["diagnostics"].as_array().expect("diagnostics");
    let titles: Vec<&str> = d
        .iter()
        .map(|x| x["title"].as_str().expect("title"))
        .collect();
    assert_eq!(titles, vec!["Page", "Screenshot", "WebDriver"], "{r}");
    assert!(
        d[0]["content"]
            .as_str()
            .expect("page")
            .contains("http://app.test/orders/new\nNew order")
    );
    assert!(Path::new(d[1]["path"].as_str().expect("path")).is_file());
    let http = d[2]["content"].as_str().expect("http");
    assert!(
        http.contains("POST session/s1/element") && http.contains("no such element"),
        "the last exchange is the failed find, not the evidence calls: {http}"
    );
    drop_instance(handle);
}

#[test]
fn on_failure_none_writes_nothing_to_disk() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let handle = init(&stub, json!({"find_timeout_secs": 0, "on_failure": "none"}));
    let dir = artifacts();
    let target = dir.path().join("000008");
    let r = dispatch(handle, PRESS, &["Pay"], None, &target);
    assert_eq!(r["status"], "fatal");
    assert!(!target.exists(), "artifacts_dir must not even be created");
    let titles: Vec<&str> = r["diagnostics"]
        .as_array()
        .expect("d")
        .iter()
        .map(|x| x["title"].as_str().expect("t"))
        .collect();
    assert_eq!(titles, vec!["Page", "WebDriver"]);
    drop_instance(handle);
}

#[test]
fn debug_true_then_false_traces_without_failing_the_dispatch() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let handle = init(&stub, json!({}));
    let dir = artifacts();
    let req = |debug: bool| {
        json!({
            "args": ["/login"],
            "docstring": null,
            "table": null,
            "artifacts_dir": dir.path().display().to_string(),
            "workspace_dir": dir.path().display().to_string(),
            "debug": debug,
            "options": {"polling": {"timeout_secs": 1, "interval_ms": 100}},
        })
    };
    let r = take(bddkit_browser::bddkit_dispatch(
        handle,
        AM_ON,
        c(&req(true).to_string()).as_ptr(),
    ));
    assert_eq!(r["status"], "passed");
    let r = take(bddkit_browser::bddkit_dispatch(
        handle,
        AM_ON,
        c(&req(false).to_string()).as_ptr(),
    ));
    assert_eq!(r["status"], "passed");
    drop_instance(handle);
}
