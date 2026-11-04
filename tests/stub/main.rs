//! The exports driven in-process against a fake WebDriver server. Nothing
//! here needs a browser; what it pins is the plugin's side of the protocol
//! and its replies to the host.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "helpers here are not #[test] functions, so clippy.toml's allow-*-in-tests misses them"
)]

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char};
use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{Method, StatusCode, Uri};
use axum::{Json, Router};
use serde_json::{Value, json};

pub const ELEMENT_KEY: &str = "element-6066-11e4-a52e-4f735466cecf";
/// A 1×1 transparent PNG, base64.
const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

#[derive(Clone, Default)]
pub struct StubElement {
    /// Found when the strategy's value *contains* this — good enough for a stub.
    pub key: String,
    pub text: String,
    pub value: String,
    pub displayed: bool,
    pub selected: bool,
    pub attrs: HashMap<String, String>,
}

#[derive(Default)]
pub struct StubState {
    /// `METHOD /path body`, in order.
    pub calls: Vec<String>,
    pub url: String,
    pub title: String,
    pub elements: Vec<StubElement>,
    /// `find` answers "no such element" this many times first.
    pub not_found_first: u32,
    pub script_result: Value,
    /// What `GET /source` answers.
    pub source: String,
    /// When true the session reply advertises `webSocketUrl` = the stub's `/bidi`.
    pub bidi: bool,
    /// When true the session reply advertises `webSocketUrl` as an
    /// unreachable `wss://` endpoint instead of the stub's own `ws://`.
    pub bidi_wss: bool,
    /// Sent on the socket right after the subscribe is acknowledged.
    pub events: Vec<Value>,
    /// Filled by `start_stub`.
    pub ws_url: String,
}

type Shared = Arc<Mutex<StubState>>;

pub struct Stub {
    pub url: String,
    pub state: Shared,
    _rt: tokio::runtime::Runtime,
}

pub fn start_stub(state: StubState) -> Stub {
    let shared: Shared = Arc::new(Mutex::new(state));
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let listener = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    shared.lock().expect("state").ws_url = format!("ws://{addr}/bidi");
    let app = Router::new()
        .route("/bidi", axum::routing::get(bidi_socket))
        .fallback(handle)
        .with_state(Arc::clone(&shared));
    rt.spawn(async move { axum::serve(listener, app).await.expect("serve") });
    Stub {
        url: format!("http://{addr}"),
        state: shared,
        _rt: rt,
    }
}

async fn bidi_socket(
    State(state): State<Shared>,
    upgrade: axum::extract::ws::WebSocketUpgrade,
) -> axum::response::Response {
    upgrade.on_upgrade(move |mut socket| async move {
        use axum::extract::ws::Message;
        // The subscribe comes first; acknowledge it by id.
        if let Some(Ok(Message::Text(text))) = socket.recv().await {
            let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            let ack = json!({"id": v["id"], "type": "success", "result": {}}).to_string();
            let _ = socket.send(Message::Text(ack.into())).await;
        }
        let events = state.lock().expect("state").events.clone();
        for event in events {
            let _ = socket.send(Message::Text(event.to_string().into())).await;
        }
        // Stay open until the plugin closes.
        while let Some(Ok(message)) = socket.recv().await {
            if matches!(message, Message::Close(_)) {
                break;
            }
        }
    })
}

fn reply(v: Value) -> (StatusCode, Json<Value>) {
    let mut body = json!({});
    body["value"] = v;
    (StatusCode::OK, Json(body))
}

fn error(status: StatusCode, error: &str, message: &str) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({"value": {"error": error, "message": message, "stacktrace": ""}})),
    )
}

fn element_index(id: &str) -> Option<usize> {
    id.strip_prefix('e')?.parse().ok()
}

/// A real WebDriver rejects most punctuation as CSS. Good enough to make the
/// stub answer `invalid selector` for `!`, `:`, `(` and the like, the way a
/// button/field/link's bare-text fallback runs into them.
fn is_plausible_css_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "_#.-[]= ".contains(c)
}

async fn handle(
    State(state): State<Shared>,
    method: Method,
    uri: Uri,
    body: String,
) -> (StatusCode, Json<Value>) {
    let mut st = state.lock().expect("stub state");
    let path = uri.path().to_string();
    st.calls
        .push(format!("{method} {path} {body}").trim().to_string());
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let req: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    match (method.as_str(), segments.as_slice()) {
        ("GET", ["status"]) => reply(json!({"ready": true, "message": "stub"})),
        ("POST", ["session"]) => {
            let mut caps = json!({"browserName": "chrome", "browserVersion": "131.0"});
            if st.bidi_wss {
                caps["webSocketUrl"] = json!("wss://127.0.0.1:1/bidi");
            } else if st.bidi {
                caps["webSocketUrl"] = json!(st.ws_url);
            }
            reply(json!({"sessionId": "s1", "capabilities": caps}))
        }
        ("DELETE", ["session", "s1"] | ["session", "s1", "cookie"])
        | ("POST", ["session", "s1", "refresh"]) => reply(Value::Null),
        ("POST", ["session", "s1", "url"]) => {
            st.url = req["url"].as_str().unwrap_or("").to_string();
            reply(Value::Null)
        }
        ("GET", ["session", "s1", "url"]) => reply(json!(st.url)),
        ("GET", ["session", "s1", "title"]) => reply(json!(st.title)),
        ("POST", ["session", "s1", "window", "rect"]) => {
            reply(json!({"width": req["width"], "height": req["height"], "x": 0, "y": 0}))
        }
        ("POST", ["session", "s1", "execute", "sync"]) => reply(st.script_result.clone()),
        ("GET", ["session", "s1", "screenshot"]) => reply(json!(PNG_1X1)),
        ("GET", ["session", "s1", "source"]) => reply(json!(st.source)),
        ("POST", ["session", "s1", "element"] | ["session", "s1", "element", _, "element"]) => {
            let using = req["using"].as_str().unwrap_or("");
            let value = req["value"].as_str().unwrap_or("").to_string();
            if using == "css selector" && !value.chars().all(is_plausible_css_char) {
                return error(
                    StatusCode::BAD_REQUEST,
                    "invalid selector",
                    &format!("{value} is not a valid CSS selector"),
                );
            }
            if st.not_found_first > 0 {
                st.not_found_first -= 1;
                return error(StatusCode::NOT_FOUND, "no such element", "stub: not yet");
            }
            match st.elements.iter().position(|e| value.contains(&e.key)) {
                Some(i) => reply(json!({ELEMENT_KEY: format!("e{i}")})),
                None => error(
                    StatusCode::NOT_FOUND,
                    "no such element",
                    &format!("nothing matches {value}"),
                ),
            }
        }
        ("POST" | "GET", ["session", "s1", "element", id, rest @ ..]) => {
            let Some(i) = element_index(id) else {
                return error(StatusCode::NOT_FOUND, "stale element reference", id);
            };
            let e = &mut st.elements[i];
            match (method.as_str(), rest) {
                ("POST", ["click"]) => {
                    e.selected = !e.selected;
                    reply(Value::Null)
                }
                ("POST", ["clear"]) => {
                    e.value.clear();
                    reply(Value::Null)
                }
                ("POST", ["value"]) => {
                    e.value.push_str(req["text"].as_str().unwrap_or(""));
                    reply(Value::Null)
                }
                ("GET", ["text"]) => reply(json!(e.text)),
                ("GET", ["attribute", name]) => {
                    reply(e.attrs.get(*name).map_or(Value::Null, |v| json!(v)))
                }
                ("GET", ["property", "value"]) => reply(json!(e.value)),
                ("GET", ["displayed"]) => reply(json!(e.displayed)),
                ("GET", ["selected"]) => reply(json!(e.selected)),
                _ => error(StatusCode::NOT_FOUND, "unknown command", &path),
            }
        }
        _ => error(StatusCode::NOT_FOUND, "unknown command", &path),
    }
}

// ---- driving the exports ----

fn take(raw: *mut c_char) -> Value {
    // SAFETY: `raw` is what an export just returned: `out` makes it with
    // `CString::into_raw`, so it is not null, is NUL-terminated and lives until
    // it is freed below.
    let s = unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: the same pointer, not freed yet; `s` holds its own copy.
    unsafe { bddkit_browser::bddkit_free_string(raw) };
    serde_json::from_str(&s)
        .unwrap_or_else(|e| panic!("the plugin replied with non-JSON {s:?}: {e}"))
}

fn c(s: &str) -> CString {
    CString::new(s).expect("no NUL")
}

pub fn config_for(stub: &Stub, extra: Value) -> Value {
    let mut cfg = json!({"browser": "chrome", "url": stub.url, "base_url": "http://app.test"});
    if let (Some(base), Value::Object(over)) = (cfg.as_object_mut(), extra) {
        base.extend(over);
    }
    json!({"group": "browser", "instance": "main", "config": cfg, "options": {}})
}

pub fn init(stub: &Stub, extra: Value) -> u64 {
    let r = take(bddkit_browser::bddkit_init_instance(
        c(&config_for(stub, extra).to_string()).as_ptr(),
    ));
    assert_eq!(r["ok"], true, "init failed: {r}");
    r["handle"].as_u64().expect("handle")
}

pub fn dispatch(
    handle: u64,
    index: u32,
    args: &[&str],
    docstring: Option<&str>,
    artifacts: &Path,
) -> Value {
    let req = json!({
        "args": args,
        "docstring": docstring,
        "table": null,
        "artifacts_dir": artifacts.display().to_string(),
        "workspace_dir": artifacts.display().to_string(),
        "debug": false,
        "options": {"polling": {"timeout_secs": 1, "interval_ms": 100}},
    });
    take(bddkit_browser::bddkit_dispatch(
        handle,
        index,
        c(&req.to_string()).as_ptr(),
    ))
}

pub fn reset(handle: u64) -> Value {
    take(bddkit_browser::bddkit_reset_scenario(handle))
}

pub fn drop_instance(handle: u64) -> Value {
    take(bddkit_browser::bddkit_drop_instance(handle))
}

pub fn calls(stub: &Stub) -> Vec<String> {
    stub.state.lock().expect("state").calls.clone()
}

pub fn artifacts() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

mod console_network;
mod dom;
mod elements;
mod evidence;
mod session;

// ---- step indexes ----

const AM_ON: u32 = 0;
const SHOULD_BE_ON: u32 = 15;
const TITLE_SHOULD_BE: u32 = 16;
const FOLLOW: u32 = 2;
const PRESS: u32 = 3;
const CLICK_ON: u32 = 4;
const FILL_IN: u32 = 5;
const SELECT: u32 = 6;
const CHECK: u32 = 7;
const UNCHECK: u32 = 8;
const ATTACH: u32 = 9;
const READ_TEXT: u32 = 12;
const READ_ATTR: u32 = 13;
const PAGE_CONTAINS: u32 = 17;
const PAGE_NOT_CONTAINS: u32 = 18;
const ELEMENT_CONTAINS: u32 = 19;
const ELEMENT_NOT_CONTAINS: u32 = 20;
const VISIBLE: u32 = 21;
const NOT_VISIBLE: u32 = 22;
const FIELD_CONTAINS: u32 = 23;
const CHECKED: u32 = 24;
const UNCHECKED: u32 = 25;
const SCRIPT: u32 = 10;
const SCRIPT_DOC: u32 = 11;
const SCREENSHOT: u32 = 14;
const DUMP_CONSOLE: u32 = 26;
const DUMP_NETWORK: u32 = 27;
const READ_STATUS: u32 = 28;
const NO_CONSOLE_ERRORS: u32 = 29;
const SENT_REQUEST: u32 = 30;
const LAST_STATUS: u32 = 31;
const DUMP_DOM: u32 = 32;
const DUMP_DOM_OF: u32 = 33;

/// `INSTANCES` (in `src/lib.rs`) is one process-wide table, and cargo runs
/// this binary's `#[test]` functions on several threads by default. Every
/// test serializes on this lock so a sibling test's `init`/`drop`
/// cannot land inside another test's `live_instances()` before/after window
/// — the delta the module doc promises only holds with that guarantee.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn element(key: &str, text: &str) -> StubElement {
    StubElement {
        key: key.into(),
        text: text.into(),
        displayed: true,
        ..Default::default()
    }
}
