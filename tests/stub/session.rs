//! Session lifecycle, navigation and config checks through the exports.

use super::*;

#[test]
fn every_declared_step_is_routed_and_the_next_index_is_not() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let dir = artifacts();
    let handle = init(&stub, json!({"find_timeout_secs": 0}));
    let unknown = |index| {
        let r = dispatch(handle, index, &["a", "b", "c"], Some("doc"), dir.path());
        r["error"]
            .as_str()
            .is_some_and(|e| e.contains("unknown step index"))
    };
    let steps = take(bddkit_browser::bddkit_list_steps());
    let count = u32::try_from(steps.as_array().expect("an array").len()).expect("fits");
    for index in 0..count {
        assert!(!unknown(index), "step {index} reaches no handler");
    }
    assert!(unknown(count), "the index after the last step is unknown");
    drop_instance(handle);
}

#[test]
fn init_opens_a_headless_session_and_sizes_the_window() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let handle = init(&stub, json!({"window": "1024x600"}));
    let calls = calls(&stub);
    let session = calls
        .iter()
        .find(|c| c.starts_with("POST /session "))
        .expect("a session was opened");
    assert!(session.contains("\"webSocketUrl\":true"), "{session}");
    assert!(session.contains("--headless=new"), "{session}");
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("POST /session/s1/window/rect") && c.contains("\"width\":1024")),
        "{calls:?}"
    );
    assert_eq!(drop_instance(handle)["ok"], true);
}

#[test]
fn i_am_on_joins_a_relative_path_to_base_url_and_keeps_an_absolute_one() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let handle = init(&stub, json!({}));
    let dir = artifacts();
    assert_eq!(
        dispatch(handle, AM_ON, &["/login"], None, dir.path())["status"],
        "passed"
    );
    assert!(
        calls(&stub)
            .iter()
            .any(|c| c.contains(r#"{"url":"http://app.test/login"}"#))
    );
    assert_eq!(
        dispatch(handle, AM_ON, &["https://other.test/x"], None, dir.path())["status"],
        "passed"
    );
    assert!(
        calls(&stub)
            .iter()
            .any(|c| c.contains(r#"{"url":"https://other.test/x"}"#))
    );
    drop_instance(handle);
}

#[test]
fn a_relative_path_without_base_url_is_fatal_naming_the_key() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let r = take(bddkit_browser::bddkit_init_instance(
        c(&json!({"group": "browser", "instance": "main", "config": {"browser": "chrome", "url": stub.url}, "options": {}}).to_string()).as_ptr(),
    ));
    let handle = r["handle"].as_u64().expect("handle");
    let dir = artifacts();
    let reply = dispatch(handle, AM_ON, &["/login"], None, dir.path());
    assert_eq!(reply["status"], "fatal");
    assert!(
        reply["error"].as_str().expect("error").contains("base_url"),
        "{reply}"
    );
    drop_instance(handle);
}

#[test]
fn should_be_on_and_title_answer_not_yet_on_a_mismatch() {
    let _guard = serial();
    let stub = start_stub(StubState {
        url: "http://app.test/login?error=1".into(),
        title: "Sign in".into(),
        ..Default::default()
    });
    let handle = init(&stub, json!({"on_failure": "none"}));
    let dir = artifacts();
    assert_eq!(
        dispatch(handle, SHOULD_BE_ON, &["/login?error=1"], None, dir.path())["status"],
        "passed"
    );
    let miss = dispatch(handle, SHOULD_BE_ON, &["/dashboard"], None, dir.path());
    assert_eq!(miss["status"], "not_yet");
    assert!(
        miss["error"]
            .as_str()
            .expect("error")
            .contains("/login?error=1"),
        "{miss}"
    );
    assert_eq!(
        dispatch(handle, TITLE_SHOULD_BE, &["Sign in"], None, dir.path())["status"],
        "passed"
    );
    assert_eq!(
        dispatch(handle, TITLE_SHOULD_BE, &["Dashboard"], None, dir.path())["status"],
        "not_yet"
    );
    drop_instance(handle);
}

#[test]
fn reset_clears_storage_and_cookies_before_leaving_the_page() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let handle = init(&stub, json!({}));
    stub.state.lock().expect("state").calls.clear();
    assert_eq!(reset(handle)["ok"], true);
    let calls = calls(&stub);
    let at = |needle: &str| {
        calls
            .iter()
            .position(|c| c.contains(needle))
            .unwrap_or_else(|| panic!("{needle} missing in {calls:?}"))
    };
    let storage = at("execute/sync");
    let cookies = at("DELETE /session/s1/cookie");
    let blank = at("about:blank");
    assert!(
        storage < cookies && cookies < blank,
        "wrong order: {calls:?}"
    );
    assert!(calls[storage].contains("localStorage.clear()"));
    drop_instance(handle);
}

#[test]
fn drop_deletes_the_session_and_forgets_the_handle() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let handle = init(&stub, json!({}));
    let before = bddkit_browser::live_instances();
    assert_eq!(drop_instance(handle)["ok"], true);
    assert!(calls(&stub).iter().any(|c| c == "DELETE /session/s1"));
    assert_eq!(bddkit_browser::live_instances(), before - 1);
    let dir = artifacts();
    let after = dispatch(handle, AM_ON, &["/"], None, dir.path());
    assert_eq!(after["status"], "fatal");
    assert_eq!(after["error"], "unknown handle");
}

#[test]
fn a_probe_opens_reads_and_closes_without_keeping_a_handle() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let before = bddkit_browser::live_instances();
    let r = take(bddkit_browser::bddkit_probe_config(
        c(&config_for(&stub, json!({})).to_string()).as_ptr(),
    ));
    assert_eq!(r["ok"], true, "{r}");
    assert!(r.get("handle").is_none());
    assert_eq!(bddkit_browser::live_instances(), before);
    let calls = calls(&stub);
    assert!(calls.iter().any(|c| c.starts_with("POST /session ")));
    assert!(calls.iter().any(|c| c == "DELETE /session/s1"));
}

#[test]
fn an_unreachable_endpoint_fails_init_with_the_url_in_the_message() {
    let _guard = serial();
    let r = take(bddkit_browser::bddkit_init_instance(
        c(&json!({"group": "browser", "instance": "main", "config": {"browser": "chrome", "url": "http://127.0.0.1:1"}, "options": {}}).to_string()).as_ptr(),
    ));
    assert_eq!(r["ok"], false);
    assert!(
        r["error"].as_str().expect("error").contains("127.0.0.1:1"),
        "{r}"
    );
}

#[test]
fn validate_config_refuses_a_typo_through_the_export() {
    let _guard = serial();
    let r = take(bddkit_browser::bddkit_validate_config(
        c(&json!({"group": "browser", "instance": "main", "config": {"browser": "chrome", "url": "http://h:4444", "headles": true}, "options": {}}).to_string()).as_ptr(),
    ));
    assert_eq!(r["ok"], false);
    assert!(
        r["error"].as_str().expect("error").contains("headles"),
        "{r}"
    );
}
