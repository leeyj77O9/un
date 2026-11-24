//! Native dispatch for `http`/`ws` in the no-VM runtime.
//!
//! The VM keeps an `Rc<RefCell<...>>` in `Payload::HttpClient` / `WebSocket`.
//! NRT uses leaked boxes behind tag 11/12 handles; program-visible behaviour
//! (validation messages, returned strings, error tags) matches the VM.

use crate::net;
use crate::obj::{alloc, err, obj, unrt_obj_bool, unrt_obj_int, unrt_obj_none, NObj};

const TAG_HTTP_CLIENT: i64 = 11;
const TAG_WEB_SOCKET: i64 = 12;

fn str_arg(p: u64, message: &str) -> Result<String, u64> {
    if unsafe { obj(p).tag } == 6 {
        Ok(unsafe { crate::strings::str_of(p) }.clone())
    } else {
        Err(err(message))
    }
}

fn client_state(p: u64) -> Result<&'static mut net::HttpState, u64> {
    unsafe {
        if obj(p).tag != TAG_HTTP_CLIENT {
            return Err(err("expected 'http' object"));
        }
        Ok(&mut *(obj(p).a as *mut net::HttpState))
    }
}

fn socket_state(p: u64) -> Result<&'static mut net::WsState, u64> {
    unsafe {
        if obj(p).tag != TAG_WEB_SOCKET {
            return Err(err("expected 'socket' object"));
        }
        Ok(&mut *(obj(p).a as *mut net::WsState))
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
    let state = client_state(args[0])?;
    let uri = url_of(&args[1..])?;
    let body = if with_body {
        Some(str_arg(args[2], "expected body is string")?)
    } else {
        None
    };
    net::request_body(state, method, &uri, body.as_deref()).map_err(|e| err(&e))
}

/// Returns `None` when this module/member pair is not one of ours, so the
/// caller can fall through to its generic unsupported-native diagnostic.
pub fn dispatch(m: &str, name: &str, args: &[u64]) -> Option<u64> {
    match (m, name) {
        ("http", "connect") => Some(unsafe {
            alloc(NObj {
                tag: TAG_HTTP_CLIENT,
                a: Box::into_raw(Box::new(net::HttpState { headers: Vec::new() })) as i64,
                b: 0,
            })
        }),
        ("http_client", "set_header") => {
            let key = match str_arg(args[1], "expected key is string") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let value = match str_arg(args[2], "expected value is string") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let state = match client_state(args[0]) {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            if key.is_empty()
                || key.contains(['\r', '\n', ':'])
                || value.contains(['\r', '\n'])
            {
                return Some(err("invalid header: names must be non-empty and free of ':', CR and LF; values must be free of CR and LF"));
            }
            state
                .headers
                .retain(|(k, _)| !k.eq_ignore_ascii_case(&key));
            state.headers.push((key, value));
            Some(unrt_obj_none())
        }
        ("http_client", "clear_headers") => match client_state(args[0]) {
            Ok(state) => {
                state.headers.clear();
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
                Ok(state) => Some(unsafe {
                    alloc(NObj {
                        tag: TAG_WEB_SOCKET,
                        a: Box::into_raw(Box::new(state)) as i64,
                        b: 0,
                    })
                }),
                Err(e) => Some(err(&e)),
            }
        }
        ("web_socket", "send") => {
            let text = match str_arg(args[1], "expected text is string") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let state = match socket_state(args[0]) {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
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
            let state = match socket_state(args[0]) {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
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
            let state = match socket_state(args[0]) {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            state.state = "Closed".to_owned();
            state.stream.take();
            Some(unrt_obj_none())
        }
        ("web_socket", "is_open") => match socket_state(args[0]) {
            Ok(state) => Some(unrt_obj_bool((state.state == "Open") as i64)),
            Err(e) => Some(e),
        },
        _ => None,
    }
}
