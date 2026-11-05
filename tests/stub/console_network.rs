//! The console and network steps over BiDi.

use super::*;

fn console_event(level: &str, text: &str) -> Value {
    json!({"type": "event", "method": "log.entryAdded", "params": {"type": "console", "level": level, "text": text, "timestamp": 1}})
}

fn request_event(id: &str, method: &str, url: &str) -> Value {
    json!({"type": "event", "method": "network.beforeRequestSent", "params": {"timestamp": 1, "request": {"request": id, "method": method, "url": url, "headers": []}}})
}

fn response_event(id: &str, status: u16) -> Value {
    json!({"type": "event", "method": "network.responseCompleted", "params": {"timestamp": 2, "request": {"request": id}, "response": {"status": status, "headers": []}}})
}

/// Events arrive on another thread; an assertion answers `not_yet` until
/// they have — exactly what the host's polling would do.
fn eventually_passes(handle: u64, index: u32, args: &[&str], dir: &Path) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let r = dispatch(handle, index, args, None, dir);
        if r["status"] == "passed" || std::time::Instant::now() > deadline {
            return r;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
fn without_bidi_the_console_and_network_steps_fail_naming_the_capability() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let handle = init(&stub, json!({"on_failure": "none"}));
    let dir = artifacts();
    for index in [DUMP_CONSOLE, DUMP_NETWORK, NO_CONSOLE_ERRORS] {
        let r = dispatch(handle, index, &[], None, dir.path());
        assert_eq!(r["status"], "fatal", "{index}");
        assert!(
            r["error"].as_str().expect("error").contains("webSocketUrl"),
            "{r}"
        );
    }
    drop_instance(handle);
}

#[test]
fn a_wss_bidi_endpoint_opens_the_session_but_leaves_the_console_and_network_steps_unavailable() {
    let _guard = serial();
    let stub = start_stub(StubState {
        bidi_wss: true,
        ..Default::default()
    });
    // Init must succeed: a wss:// webSocketUrl is a known limit, not a
    // reason to fail the session.
    let handle = init(&stub, json!({"on_failure": "none"}));
    let dir = artifacts();
    for index in [DUMP_CONSOLE, DUMP_NETWORK, NO_CONSOLE_ERRORS] {
        let r = dispatch(handle, index, &[], None, dir.path());
        assert_eq!(r["status"], "fatal", "{index}");
        assert!(
            r["error"].as_str().expect("error").contains("webSocketUrl"),
            "{r}"
        );
    }
    drop_instance(handle);
}

#[test]
fn console_errors_including_uncaught_exceptions_are_fatal_with_the_entries() {
    let _guard = serial();
    let stub = start_stub(StubState {
        bidi: true,
        events: vec![
            console_event("info", "fine"),
            console_event("error", "boom: the widget failed"),
            json!({"type": "event", "method": "log.entryAdded", "params": {"type": "javascript", "level": "error", "text": "Error: uncaught", "timestamp": 3}}),
        ],
        ..Default::default()
    });
    let handle = init(&stub, json!({"on_failure": "console"}));
    let dir = artifacts();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let r = dispatch(handle, NO_CONSOLE_ERRORS, &[], None, dir.path());
    assert_eq!(r["status"], "fatal", "errors never disappear: {r}");
    assert!(
        r["error"].as_str().expect("error").contains("2 error"),
        "{r}"
    );
    let console = r["diagnostics"]
        .as_array()
        .expect("d")
        .iter()
        .find(|d| d["title"] == "Console")
        .expect("console evidence");
    assert!(
        console["content"]
            .as_str()
            .expect("content")
            .contains("boom: the widget failed")
    );
    drop_instance(handle);
}

#[test]
fn network_assertions_match_method_and_path_prefix_and_read_the_status() {
    let _guard = serial();
    let stub = start_stub(StubState {
        bidi: true,
        events: vec![
            request_event("r1", "GET", "http://app.test/orders/new"),
            request_event("r2", "POST", "http://app.test/api/orders?x=1"),
            response_event("r2", 201),
        ],
        ..Default::default()
    });
    let handle = init(&stub, json!({"on_failure": "none"}));
    let dir = artifacts();
    assert_eq!(
        eventually_passes(handle, SENT_REQUEST, &["POST", "/api/orders"], dir.path())["status"],
        "passed"
    );
    assert_eq!(
        dispatch(
            handle,
            SENT_REQUEST,
            &["DELETE", "/api/orders"],
            None,
            dir.path()
        )["status"],
        "not_yet"
    );
    assert_eq!(
        dispatch(handle, SENT_REQUEST, &["GET", "/api"], None, dir.path())["status"],
        "not_yet",
        "prefix on the path, method exact"
    );
    assert_eq!(
        dispatch(
            handle,
            LAST_STATUS,
            &["/api/orders", "201"],
            None,
            dir.path()
        )["status"],
        "passed"
    );
    assert_eq!(
        dispatch(
            handle,
            LAST_STATUS,
            &["/api/orders", "200"],
            None,
            dir.path()
        )["status"],
        "not_yet"
    );
    assert_eq!(
        dispatch(
            handle,
            LAST_STATUS,
            &["/orders/new", "200"],
            None,
            dir.path()
        )["status"],
        "not_yet",
        "no response yet"
    );
    let r = dispatch(
        handle,
        READ_STATUS,
        &["/api/orders", "code"],
        None,
        dir.path(),
    );
    assert_eq!(r["vars"]["code"], "201");
    let none = dispatch(handle, READ_STATUS, &["/nope", "code"], None, dir.path());
    assert_eq!(none["status"], "fatal");
    assert_eq!(
        dispatch(handle, NO_CONSOLE_ERRORS, &[], None, dir.path())["status"],
        "passed"
    );
    drop_instance(handle);
}

#[test]
fn dumps_write_json_files_and_a_reset_empties_the_buffers() {
    let _guard = serial();
    let stub = start_stub(StubState {
        bidi: true,
        events: vec![
            console_event("warn", "w"),
            request_event("r1", "GET", "http://app.test/x"),
        ],
        ..Default::default()
    });
    let handle = init(&stub, json!({"on_failure": "none"}));
    let dir = artifacts();
    assert_eq!(
        eventually_passes(handle, SENT_REQUEST, &["GET", "/x"], dir.path())["status"],
        "passed"
    );
    let target = dir.path().join("000009");
    assert_eq!(
        dispatch(handle, DUMP_CONSOLE, &[], None, &target)["status"],
        "passed"
    );
    assert_eq!(
        dispatch(handle, DUMP_NETWORK, &[], None, &target)["status"],
        "passed"
    );
    let console: Value =
        serde_json::from_slice(&std::fs::read(target.join("console.json")).expect("console.json"))
            .expect("json");
    assert_eq!(console[0]["text"], "w");
    let network: Value =
        serde_json::from_slice(&std::fs::read(target.join("network.json")).expect("network.json"))
            .expect("json");
    assert_eq!(network[0]["url"], "http://app.test/x");
    assert_eq!(reset(handle)["ok"], true);
    assert_eq!(
        dispatch(handle, SENT_REQUEST, &["GET", "/x"], None, dir.path())["status"],
        "not_yet",
        "the reset emptied the network log"
    );
    drop_instance(handle);
}

#[test]
fn a_failure_attaches_console_and_network_when_on_failure_asks() {
    let _guard = serial();
    let stub = start_stub(StubState {
        bidi: true,
        events: vec![
            console_event("error", "e"),
            request_event("r1", "GET", "http://app.test/x"),
        ],
        ..Default::default()
    });
    let handle = init(
        &stub,
        json!({"find_timeout_secs": 0, "on_failure": "console,network"}),
    );
    let dir = artifacts();
    assert_eq!(
        eventually_passes(handle, SENT_REQUEST, &["GET", "/x"], dir.path())["status"],
        "passed"
    );
    let r = dispatch(handle, PRESS, &["Pay"], None, dir.path());
    let titles: Vec<&str> = r["diagnostics"]
        .as_array()
        .expect("d")
        .iter()
        .map(|x| x["title"].as_str().expect("t"))
        .collect();
    assert_eq!(titles, vec!["Page", "Console", "Network", "WebDriver"]);
    drop_instance(handle);
}
