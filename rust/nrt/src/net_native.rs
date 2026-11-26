//! Native dispatch for `http`/`ws` in the no-VM runtime.
//!
//! The VM keeps an `Rc<RefCell<...>>` in `Payload::HttpClient` / `WebSocket`.
//! NRT used to keep leaked boxes behind tag 11/12 handles; after A-09 the
//! per-object state lives in handle-indexed tables of `Arc<Mutex<..>>`, so a
//! worker thread can never obtain a mutable alias to another worker's state.
//! Program-visible behaviour (validation messages, returned strings, error
//! tags) is unchanged from the VM.

use std::sync::{Arc, Mutex, MutexGuard};

use crate::net;
use crate::obj::{alloc, err, obj, unrt_obj_bool, unrt_obj_int, unrt_obj_none, NObj};

const TAG_HTTP_CLIENT: i64 = 11;
const TAG_WEB_SOCKET: i64 = 12;

// A-09 (W-2026-11-26-P03): a tag-11/12 handle stores a table index in `NObj.a`,
// not a raw state pointer. The table lock is held only long enough to clone the
// per-object `Arc`; the payload lock is then scoped to one dispatch call, so
// concurrent requests on *different* objects never contend on one global lock
// and no mutable reference escapes the call. The C ABI only sees the `u64`
// handle, so no symbol or signature changes.
static HTTP_STATES: Mutex<Vec<Arc<Mutex<net::HttpState>>>> = Mutex::new(Vec::new());
static WS_STATES: Mutex<Vec<Arc<Mutex<net::WsState>>>> = Mutex::new(Vec::new());

fn http_handle(state: net::HttpState) -> u64 {
    let idx = {
        let mut table = HTTP_STATES.lock().unwrap();
        let idx = table.len();
        table.push(Arc::new(Mutex::new(state)));
        idx
    };
    unsafe { alloc(NObj { tag: TAG_HTTP_CLIENT, a: idx as i64, b: 0 }) }
}

fn ws_handle(state: net::WsState) -> u64 {
    let idx = {
        let mut table = WS_STATES.lock().unwrap();
        let idx = table.len();
        table.push(Arc::new(Mutex::new(state)));
        idx
    };
    unsafe { alloc(NObj { tag: TAG_WEB_SOCKET, a: idx as i64, b: 0 }) }
}

/// Look up the per-object cell after validating the receiver tag. Returning an
/// owned `Arc` means the registry lock is released before the payload is used.
fn state_cell<T>(
    p: u64,
    tag: i64,
    table: &Mutex<Vec<Arc<Mutex<T>>>>,
    message: &str,
) -> Result<Arc<Mutex<T>>, u64> {
    if unsafe { obj(p).tag } != tag {
        return Err(err(message));
    }
    let idx = unsafe { obj(p).a } as usize;
    // An out-of-range index cannot be produced by either `*_handle`
    // constructor. Report it with the existing tag diagnostic rather than
    // inventing a new user-visible error string (I5).
    table.lock().unwrap().get(idx).cloned().ok_or_else(|| err(message))
}

fn client_cell(p: u64) -> Result<Arc<Mutex<net::HttpState>>, u64> {
    state_cell(p, TAG_HTTP_CLIENT, &HTTP_STATES, "expected 'http' object")
}

fn socket_cell(p: u64) -> Result<Arc<Mutex<net::WsState>>, u64> {
    state_cell(p, TAG_WEB_SOCKET, &WS_STATES, "expected 'socket' object")
}

/// Recover from poisoning rather than panicking: a worker that panicked while
/// holding a state lock must not change a plain build's observable output.
fn lock_state<T>(cell: &Mutex<T>) -> MutexGuard<'_, T> {
    cell.lock().unwrap_or_else(|e| e.into_inner())
}

fn str_arg(p: u64, message: &str) -> Result<String, u64> {
    if unsafe { obj(p).tag } == 6 {
        Ok(unsafe { crate::strings::str_of(p) }.clone())
    } else {
        Err(err(message))
    }
}

fn url_of(args: &[u64]) -> Result<net::Uri, u64> {
    let text = str_arg(args[0], "expected url is string")?;
    net::parse_uri(&text).map_err(|e| err(&e))
}

fn http_request(
    method: &str,
    args: &[u64],
    with_body: bool,
) -> Result<net::Response, u64> {
    // Validate the receiver before the URL/body, exactly as before, so the
    // first error reported for a malformed call stays byte-identical.
    let cell = client_cell(args[0])?;
    let uri = url_of(&args[1..])?;
    let body = if with_body {
        Some(str_arg(args[2], "expected body is string")?)
    } else {
        None
    };
    let state = lock_state(&cell);
    net::request_body(&state, method, &uri, body.as_deref()).map_err(|e| err(&e))
}

/// Returns `None` when this module/member pair is not one of ours, so the
/// caller can fall through to its generic unsupported-native diagnostic.
pub fn dispatch(m: &str, name: &str, args: &[u64]) -> Option<u64> {
    match (m, name) {
        ("http", "connect") => Some(http_handle(net::HttpState { headers: Vec::new() })),
        ("http_client", "set_header") => {
            let key = match str_arg(args[1], "expected key is string") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let value = match str_arg(args[2], "expected value is string") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let cell = match client_cell(args[0]) {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            if key.is_empty()
                || key.contains(['\r', '\n', ':'])
                || value.contains(['\r', '\n'])
            {
                return Some(err("invalid header: names must be non-empty and free of ':', CR and LF; values must be free of CR and LF"));
            }
            let mut state = lock_state(&cell);
            state
                .headers
                .retain(|(k, _)| !k.eq_ignore_ascii_case(&key));
            state.headers.push((key, value));
            Some(unrt_obj_none())
        }
        ("http_client", "clear_headers") => match client_cell(args[0]) {
            Ok(cell) => {
                lock_state(&cell).headers.clear();
                Some(unrt_obj_none())
            }
            Err(e) => Some(e),
        },
        ("http_client", "get") => Some(match http_request("GET", args, false) {
            Ok(response) => crate::strings::str_new(response.body.clone()),
            Err(e) => e,
        }),
        ("http_client", "delete") => Some(match http_request("DELETE", args, false) {
            Ok(response) => crate::strings::str_new(response.body.clone()),
            Err(e) => e,
        }),
        ("http_client", "post") => Some(match http_request("POST", args, true) {
            Ok(response) => crate::strings::str_new(response.body.clone()),
            Err(e) => e,
        }),
        ("http_client", "put") => Some(match http_request("PUT", args, true) {
            Ok(response) => crate::strings::str_new(response.body.clone()),
            Err(e) => e,
        }),
        ("http_client", "patch") => Some(match http_request("PATCH", args, true) {
            Ok(response) => crate::strings::str_new(response.body.clone()),
            Err(e) => e,
        }),
        ("http_client", "head") => Some(match http_request("HEAD", args, false) {
            Ok(response) => unrt_obj_int(response.code as i64),
            Err(e) => e,
        }),
        ("http_client", "options") => Some(match http_request("OPTIONS", args, false) {
            Ok(response) => {
                let mut methods = Vec::new();
                for (name, value) in &response.headers {
                    if !name.eq_ignore_ascii_case("Allow") {
                        continue;
                    }
                    for method in value.split(',') {
                        let method = method.trim();
                        if !method.is_empty() {
                            methods.push(crate::strings::str_new(method.to_string()));
                        }
                    }
                }
                crate::list::list_new(methods)
            }
            Err(e) => e,
        }),
        ("ws", "connect") => {
            let text = match str_arg(args[0], "expected value is string") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let uri = match net::parse_uri(&text) {
                Ok(v) => v,
                Err(e) => return Some(err(&e)),
            };
            match net::ws_connect(&uri) {
                Ok(state) => Some(ws_handle(state)),
                Err(e) => Some(err(&e)),
            }
        }
        ("web_socket", "send") => {
            let text = match str_arg(args[1], "expected text is string") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let cell = match socket_cell(args[0]) {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let mut state = lock_state(&cell);
            if state.state != "Open" {
                return Some(err("cannot send, the socket is not open"));
            }
            let stream = match state.stream.as_mut() {
                Some(v) => v,
                None => return Some(err("cannot send, the socket is not open")),
            };
            Some(match net::write_frame(stream, 1, text.as_bytes()) {
                Ok(()) => unrt_obj_none(),
                Err(e) => err(&e),
            })
        }
        ("web_socket", "receive") => {
            let cell = match socket_cell(args[0]) {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let mut state = lock_state(&cell);
            if state.state != "Open" {
                return Some(unrt_obj_none());
            }
            let stream = match state.stream.as_mut() {
                Some(v) => v,
                None => return Some(unrt_obj_none()),
            };
            Some(loop {
                match net::read_frame(stream) {
                    Ok((opcode, payload, _fin)) => {
                        if opcode == 8 {
                            state.state = "Closed".to_owned();
                            break unrt_obj_none();
                        }
                        if opcode == 1 {
                            break crate::strings::str_new(String::from_utf8_lossy(&payload).into_owned());
                        }
                    }
                    Err(e) => break err(&e),
                }
            })
        }
        ("web_socket", "close") => {
            let cell = match socket_cell(args[0]) {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let mut state = lock_state(&cell);
            state.state = "Closed".to_owned();
            state.stream.take();
            Some(unrt_obj_none())
        }
        ("web_socket", "is_open") => match socket_cell(args[0]) {
            Ok(cell) => Some(unrt_obj_bool((lock_state(&cell).state == "Open") as i64)),
            Err(e) => Some(e),
        },
        _ => None,
    }
}

#[cfg(test)]
mod a09_tests {
    use super::*;
    use crate::obj::display;

    fn client() -> u64 {
        http_handle(net::HttpState { headers: Vec::new() })
    }

    fn socket() -> u64 {
        ws_handle(net::WsState { stream: None, state: "Open".into() })
    }

    /// A-09: two handles must not share header state, and `NObj.a` is now an
    /// index into the guard table (the ABI-visible handle is still the same
    /// `u64`, so no generated code changes).
    #[test]
    fn http_handles_have_independent_locked_state() {
        let a = client();
        let b = client();
        assert_eq!(unsafe { obj(a).tag }, TAG_HTTP_CLIENT);
        let key = crate::strings::str_new("X-A".into());
        let value = crate::strings::str_new("1".into());
        let r = dispatch("http_client", "set_header", &[a, key, value]).unwrap();
        assert_eq!(unsafe { obj(r).tag }, 4, "set_header returns none");
        assert_eq!(lock_state(&client_cell(a).unwrap()).headers.len(), 1);
        assert_eq!(lock_state(&client_cell(b).unwrap()).headers.len(), 0);

        let r = dispatch("http_client", "clear_headers", &[a]).unwrap();
        assert_eq!(unsafe { obj(r).tag }, 4);
        assert_eq!(lock_state(&client_cell(a).unwrap()).headers.len(), 0);
    }

    /// A-09: eight workers writing distinct headers to the *same* client are
    /// serialized by its per-object lock; no write is lost and no shared
    /// mutable reference is returned to a worker.
    #[test]
    fn concurrent_header_writes_are_serialized_per_object() {
        let c = client();
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let key = crate::strings::str_new(format!("X-{i}"));
                let value = crate::strings::str_new(i.to_string());
                std::thread::spawn(move || {
                    let r = dispatch("http_client", "set_header", &[c, key, value]).unwrap();
                    unsafe { obj(r).tag }
                })
            })
            .collect();
        for t in threads {
            assert_eq!(t.join().unwrap(), 4, "set_header returns none");
        }
        assert_eq!(lock_state(&client_cell(c).unwrap()).headers.len(), 8);
    }

    /// A-09: socket state and every validation message stay byte-identical.
    #[test]
    fn socket_state_and_error_strings_are_unchanged() {
        let s = socket();
        assert_eq!(unsafe { obj(s).tag }, TAG_WEB_SOCKET);
        assert_eq!(display(dispatch("web_socket", "is_open", &[s]).unwrap()), "true");
        assert_eq!(display(dispatch("web_socket", "close", &[s]).unwrap()), "none");
        assert_eq!(display(dispatch("web_socket", "is_open", &[s]).unwrap()), "false");

        let bad = dispatch("web_socket", "is_open", &[unrt_obj_int(0)]).unwrap();
        assert_eq!(unsafe { obj(bad).tag }, 5);
        assert_eq!(display(bad), "expected 'socket' object");
        let bad = dispatch("http_client", "clear_headers", &[unrt_obj_int(0)]).unwrap();
        assert_eq!(unsafe { obj(bad).tag }, 5);
        assert_eq!(display(bad), "expected 'http' object");
        let bad = dispatch("web_socket", "send", &[s, unrt_obj_int(0)]).unwrap();
        assert_eq!(display(bad), "expected text is string");
    }
}
