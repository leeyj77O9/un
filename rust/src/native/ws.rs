use std::cell::RefCell;
use std::rc::Rc;
use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use super::{arg, native};
use super::net::{self, WsState};
const SCHEME: &str = "Only Uris starting with 'ws://' or 'wss://' are supported. (Parameter 'uri')";
fn url(scope: &Scope) -> Result<net::Uri, Obj> {
    let text = match obj::payload(&scope::get(scope, "url")) {
        Payload::Str(s) => s.to_string(),
        _ => return Err(obj::err("expected value is string")),
    };
    match net::parse_uri(&text) {
        Ok(uri) => Ok(uri),
        Err(message) => Err(obj::err(message)),
    }
}
fn socket_self(scope: &Scope) -> Option<Rc<RefCell<WsState>>> {
    match obj::payload(&scope::get(scope, "self")) {
        Payload::WebSocket(state) => Some(state.clone()),
        _ => None,
    }
}
fn connect_impl(scope: &Scope) -> Obj {
    let uri = match url(scope) {
        Ok(uri) => uri,
        Err(error) => return error,
    };

    if uri.scheme != "ws" && uri.scheme != "wss" {
        return obj::err(SCHEME);
    }
    match net::ws_connect(&uri) {
        Ok(state) => obj::obj_new("web_socket", Payload::WebSocket(Rc::new(RefCell::new(state)))),
        Err(message) => obj::err(message),
    }
}
fn send_impl(scope: &Scope) -> Obj {
    let Some(state) = socket_self(scope) else {
        return obj::err("expected 'socket' object");
    };
    let text = match obj::payload(&scope::get(scope, "text")) {
        Payload::Str(s) => s.to_string(),
        _ => return obj::err("expected text is string"),
    };
    let mut state = state.borrow_mut();
    if state.state != "Open" {
        return obj::err("cannot send, the socket is not open");
    }
    let result = state
        .stream
        .as_mut()
        .map(|stream| net::write_frame(stream, 1, text.as_bytes()));
    match result {
        Some(Ok(())) => obj::none(),
        Some(Err(message)) => obj::err(message),
        None => obj::err("cannot send, the socket is not open"),
    }
}
fn receive_impl(scope: &Scope) -> Obj {
    let Some(cell) = socket_self(scope) else {
        return obj::err("expected 'socket' object");
    };

    let mut message = Vec::new();
    loop {
        let mut state = cell.borrow_mut();
        let Some(stream) = state.stream.as_mut() else {
            return obj::err("cannot receive, the socket is not open");
        };
        let frame = net::read_frame(stream);

        drop(state);
        let (opcode, payload, fin) = match frame {
            Ok(frame) => frame,
            Err(text) => return obj::err(text),
        };
        if opcode == 8 {
            let mut state = cell.borrow_mut();
            state.state = "Closed".to_owned();
            state.open = false;
            state.stream = None;
            return obj::none();
        }
        message.extend_from_slice(&payload);
        if fin {
            break;
        }
    }
    match String::from_utf8(message) {
        Ok(text) => obj::str(&text),
        Err(_) => obj::str(""),
    }
}
fn close_impl(scope: &Scope) -> Obj {
    let Some(state) = socket_self(scope) else {
        return obj::err("expected 'socket' object");
    };
    let mut state = state.borrow_mut();
    if state.state != "Open" {
        return obj::err("cannot close, the socket is not open");
    }

    let code = [0x03, 0xE8];
    let reason = b"closed";
    let mut payload = Vec::new();
    payload.extend_from_slice(&code);
    payload.extend_from_slice(reason);
    let result = state
        .stream
        .as_mut()
        .map(|stream| net::write_frame(stream, 8, &payload));
    match result {
        Some(Err(message)) => return obj::err(message),
        Some(Ok(())) => state.state = "Closing".to_owned(),
        None => return obj::err("cannot close, the socket is not open"),
    }

    let answer = state
        .stream
        .as_mut()
        .map(|stream| net::read_frame(stream));
    match answer {
        Some(Ok((8, _, _))) | Some(Err(_)) | None => {
            state.state = "Closed".to_owned();
            state.open = false;
            state.stream = None;
        }
        Some(Ok(_)) => {}
    }
    obj::none()
}
fn is_open_impl(scope: &Scope) -> Obj {
    let Some(state) = socket_self(scope) else {
        return obj::err("expected 'socket' object");
    };
    let open = state.borrow().state == "Open";
    obj::boolean(open)
}
pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    vec![(
        "connect".to_owned(),
        native(
            closure,
            "connect",
            vec![arg("url", true, false, false)],
            connect_impl,
        ),
    )]
}
pub fn socket_members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "send".to_owned(),
            native(
                closure,
                "send",
                vec![arg("text", true, false, false)],
                send_impl,
            ),
        ),
        (
            "receive".to_owned(),
            native(closure, "receive", Vec::new(), receive_impl),
        ),
        ("close".to_owned(), native(closure, "close", Vec::new(), close_impl)),
        (
            "is_open".to_owned(),
            native(closure, "is_open", Vec::new(), is_open_impl),
        ),
    ]
}
