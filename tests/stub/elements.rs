//! Element actions and the assertions on what they changed.

use super::*;

#[test]
fn follow_press_and_click_find_by_text_and_click() {
    let _guard = serial();
    let stub = start_stub(StubState {
        elements: vec![
            element("'New order'", "New order"),
            element("'Pay'", "Pay"),
            element("#cart", ""),
        ],
        ..Default::default()
    });
    let handle = init(&stub, json!({"find_timeout_secs": 0}));
    let dir = artifacts();
    assert_eq!(
        dispatch(handle, FOLLOW, &["New order"], None, dir.path())["status"],
        "passed"
    );
    assert_eq!(
        dispatch(handle, PRESS, &["Pay"], None, dir.path())["status"],
        "passed"
    );
    assert_eq!(
        dispatch(handle, CLICK_ON, &["#cart"], None, dir.path())["status"],
        "passed"
    );
    let calls = calls(&stub);
    assert!(
        calls.iter().any(|c| c.contains("\"using\":\"xpath\"")
            && c.contains("//a[normalize-space(.)='New order'")),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|c| c.contains("\"using\":\"css selector\",\"value\":\"#cart\"")),
        "{calls:?}"
    );
    assert_eq!(calls.iter().filter(|c| c.ends_with("/click {}")).count(), 3);
    drop_instance(handle);
}

#[test]
fn a_missing_element_is_fatal_for_an_action_and_names_the_lookup() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let handle = init(&stub, json!({"find_timeout_secs": 0, "on_failure": "none"}));
    let dir = artifacts();
    let r = dispatch(handle, PRESS, &["Nope"], None, dir.path());
    assert_eq!(r["status"], "fatal");
    assert!(
        r["error"]
            .as_str()
            .expect("error")
            .contains("button \"Nope\""),
        "{r}"
    );
    drop_instance(handle);
}

#[test]
fn a_button_text_that_is_not_valid_css_still_waits_and_fails_as_not_found() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let handle = init(&stub, json!({"find_timeout_secs": 0, "on_failure": "none"}));
    let dir = artifacts();
    // "Pay now!" is ordinary button text but invalid CSS (the trailing `!`)
    // — the bare-text fallback strategy must not turn that into a fatal
    // driver error; it should read the same as "no such button".
    let r = dispatch(handle, PRESS, &["Pay now!"], None, dir.path());
    assert_eq!(r["status"], "fatal");
    let error = r["error"].as_str().expect("error");
    assert!(error.contains("button \"Pay now!\""), "{r}");
    assert!(!error.contains("invalid selector"), "{r}");
    drop_instance(handle);
}

#[test]
fn a_tester_written_css_selector_that_is_invalid_is_fatal_with_the_driver_error() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let handle = init(&stub, json!({"find_timeout_secs": 0, "on_failure": "none"}));
    let dir = artifacts();
    // Unlike a named lookup's fallback, `I click on "<selector>"` takes CSS
    // straight from the tester: an invalid one is their bug to see.
    let r = dispatch(handle, CLICK_ON, &["a:nth-child("], None, dir.path());
    assert_eq!(r["status"], "fatal");
    assert!(
        r["error"]
            .as_str()
            .expect("error")
            .contains("invalid selector"),
        "{r}"
    );
    drop_instance(handle);
}

#[test]
fn fill_in_clears_then_types_and_select_picks_the_option() {
    let _guard = serial();
    let stub = start_stub(StubState {
        elements: vec![
            element("'email'", ""),
            element("'country'", ""),
            element("option[normalize-space(.)='Latvia'", "Latvia"),
        ],
        ..Default::default()
    });
    let handle = init(&stub, json!({"find_timeout_secs": 0}));
    let dir = artifacts();
    assert_eq!(
        dispatch(
            handle,
            FILL_IN,
            &["email", "ann@example.test"],
            None,
            dir.path()
        )["status"],
        "passed"
    );
    assert_eq!(
        stub.state.lock().expect("state").elements[0].value,
        "ann@example.test"
    );
    let calls_before_select = calls(&stub);
    let clear = calls_before_select
        .iter()
        .position(|c| c.ends_with("/e0/clear {}"))
        .expect("clear");
    let typed = calls_before_select
        .iter()
        .position(|c| c.ends_with("/e0/value {\"text\":\"ann@example.test\"}"))
        .expect("value");
    assert!(clear < typed);
    assert_eq!(
        dispatch(handle, SELECT, &["Latvia", "country"], None, dir.path())["status"],
        "passed"
    );
    assert!(
        calls(&stub)
            .iter()
            .any(|c| c.starts_with("POST /session/s1/element/e1/element") && c.contains("option"))
    );
    drop_instance(handle);
}

#[test]
fn check_and_uncheck_click_only_when_the_state_differs() {
    let _guard = serial();
    let stub = start_stub(StubState {
        elements: vec![element("'newsletter'", "")],
        ..Default::default()
    });
    let handle = init(&stub, json!({"find_timeout_secs": 0}));
    let dir = artifacts();
    assert_eq!(
        dispatch(handle, CHECK, &["newsletter"], None, dir.path())["status"],
        "passed"
    );
    assert!(stub.state.lock().expect("state").elements[0].selected);
    assert_eq!(
        dispatch(handle, CHECK, &["newsletter"], None, dir.path())["status"],
        "passed"
    );
    assert!(
        stub.state.lock().expect("state").elements[0].selected,
        "a second check must not toggle it back"
    );
    assert_eq!(
        dispatch(handle, UNCHECK, &["newsletter"], None, dir.path())["status"],
        "passed"
    );
    assert!(!stub.state.lock().expect("state").elements[0].selected);
    assert_eq!(
        calls(&stub)
            .iter()
            .filter(|c| c.ends_with("/click {}"))
            .count(),
        2
    );
    drop_instance(handle);
}

#[test]
fn attach_sends_the_absolute_path_and_refuses_a_missing_file() {
    let _guard = serial();
    let stub = start_stub(StubState {
        elements: vec![element("'avatar'", "")],
        ..Default::default()
    });
    let handle = init(&stub, json!({"find_timeout_secs": 0, "on_failure": "none"}));
    let dir = artifacts();
    std::fs::write(dir.path().join("me.png"), b"png").expect("write");
    assert_eq!(
        dispatch(handle, ATTACH, &["me.png", "avatar"], None, dir.path())["status"],
        "passed"
    );
    let expected = dir.path().join("me.png").display().to_string();
    assert!(
        calls(&stub)
            .iter()
            .any(|c| c.ends_with(&format!("/e0/value {{\"text\":\"{expected}\"}}"))),
        "{:?}",
        calls(&stub)
    );
    let missing = dispatch(handle, ATTACH, &["nope.png", "avatar"], None, dir.path());
    assert_eq!(missing["status"], "fatal");
    assert!(
        missing["error"]
            .as_str()
            .expect("error")
            .contains("nope.png")
    );
    drop_instance(handle);
}

#[test]
fn read_text_and_attribute_publish_variables_and_a_missing_attribute_is_fatal() {
    let _guard = serial();
    let mut e = element("[data-test=order-id]", "  ord-42 \n");
    e.attrs.insert("data-id".into(), "42".into());
    let stub = start_stub(StubState {
        elements: vec![e],
        ..Default::default()
    });
    let handle = init(&stub, json!({"find_timeout_secs": 0, "on_failure": "none"}));
    let dir = artifacts();
    let r = dispatch(
        handle,
        READ_TEXT,
        &["[data-test=order-id]", "orderId"],
        None,
        dir.path(),
    );
    assert_eq!(r["status"], "passed");
    assert_eq!(r["vars"]["orderId"], "ord-42");
    let r = dispatch(
        handle,
        READ_ATTR,
        &["data-id", "[data-test=order-id]", "id"],
        None,
        dir.path(),
    );
    assert_eq!(r["vars"]["id"], "42");
    let r = dispatch(
        handle,
        READ_ATTR,
        &["data-nope", "[data-test=order-id]", "id"],
        None,
        dir.path(),
    );
    assert_eq!(r["status"], "fatal");
    assert!(r["error"].as_str().expect("error").contains("data-nope"));
    drop_instance(handle);
}

#[test]
fn page_and_element_text_assertions_collapse_whitespace_and_answer_not_yet() {
    let _guard = serial();
    let stub = start_stub(StubState {
        elements: vec![
            element("body", "Welcome,\n   ann@example.test  \n Sign out"),
            element("h1", "Welcome,   ann"),
        ],
        ..Default::default()
    });
    let handle = init(&stub, json!({"on_failure": "none"}));
    let dir = artifacts();
    assert_eq!(
        dispatch(
            handle,
            PAGE_CONTAINS,
            &["Welcome, ann@example.test"],
            None,
            dir.path()
        )["status"],
        "passed"
    );
    assert_eq!(
        dispatch(handle, PAGE_CONTAINS, &["Goodbye"], None, dir.path())["status"],
        "not_yet"
    );
    assert_eq!(
        dispatch(handle, PAGE_NOT_CONTAINS, &["Goodbye"], None, dir.path())["status"],
        "passed"
    );
    assert_eq!(
        dispatch(handle, PAGE_NOT_CONTAINS, &["Sign out"], None, dir.path())["status"],
        "not_yet"
    );
    assert_eq!(
        dispatch(
            handle,
            ELEMENT_CONTAINS,
            &["h1", "Welcome, ann"],
            None,
            dir.path()
        )["status"],
        "passed"
    );
    assert_eq!(
        dispatch(
            handle,
            ELEMENT_NOT_CONTAINS,
            &["h1", "bob"],
            None,
            dir.path()
        )["status"],
        "passed"
    );
    let missing = dispatch(
        handle,
        ELEMENT_NOT_CONTAINS,
        &["h2", "bob"],
        None,
        dir.path(),
    );
    assert_eq!(
        missing["status"], "not_yet",
        "an absent element is expected to appear, for both directions"
    );
    drop_instance(handle);
}

#[test]
fn visibility_field_and_checkbox_assertions() {
    let _guard = serial();
    let mut hidden = element("#hidden", "");
    hidden.displayed = false;
    let mut email = element("'email'", "");
    email.value = "ann@example.test".into();
    let mut news = element("'newsletter'", "");
    news.selected = true;
    let stub = start_stub(StubState {
        elements: vec![element("#shown", ""), hidden, email, news],
        ..Default::default()
    });
    let handle = init(&stub, json!({"on_failure": "none"}));
    let dir = artifacts();
    assert_eq!(
        dispatch(handle, VISIBLE, &["#shown"], None, dir.path())["status"],
        "passed"
    );
    assert_eq!(
        dispatch(handle, VISIBLE, &["#hidden"], None, dir.path())["status"],
        "not_yet"
    );
    assert_eq!(
        dispatch(handle, NOT_VISIBLE, &["#hidden"], None, dir.path())["status"],
        "passed"
    );
    assert_eq!(
        dispatch(handle, NOT_VISIBLE, &["#absent"], None, dir.path())["status"],
        "passed",
        "not found counts as not visible"
    );
    assert_eq!(
        dispatch(handle, VISIBLE, &["#absent"], None, dir.path())["status"],
        "not_yet"
    );
    assert_eq!(
        dispatch(
            handle,
            FIELD_CONTAINS,
            &["email", "ann@example.test"],
            None,
            dir.path()
        )["status"],
        "passed"
    );
    assert_eq!(
        dispatch(handle, FIELD_CONTAINS, &["email", "bob"], None, dir.path())["status"],
        "not_yet"
    );
    assert_eq!(
        dispatch(handle, CHECKED, &["newsletter"], None, dir.path())["status"],
        "passed"
    );
    assert_eq!(
        dispatch(handle, UNCHECKED, &["newsletter"], None, dir.path())["status"],
        "not_yet"
    );
    drop_instance(handle);
}

#[test]
fn a_null_argument_is_refused_before_anything_is_sent() {
    let _guard = serial();
    let stub = start_stub(StubState::default());
    let handle = init(&stub, json!({}));
    stub.state.lock().expect("state").calls.clear();
    let dir = artifacts();
    let r = dispatch(
        handle,
        FILL_IN,
        &["email", "\u{0}__bddkit_null__\u{0}"],
        None,
        dir.path(),
    );
    assert_eq!(r["status"], "fatal");
    assert!(r["error"].as_str().expect("error").contains("<<null>>"));
    assert!(calls(&stub).is_empty(), "nothing must reach the driver");
    drop_instance(handle);
}
