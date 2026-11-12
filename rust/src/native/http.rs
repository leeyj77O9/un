//! `Un.Native.Http` and `Un.Object.Web.HttpClient` — the native `http` module and the
//! `http` client object.
//!
//! the client's `UnType` is `http_client` (aligned with its `[NativeType]` registration
//! key in the reference), so member lookup resolves the 16 members registered under the
//! `http_client` key, matching the reference's behavior after the C# fix.

use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::collections;
use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};

use super::{arg, native};
use super::net::{self, HttpState};

/// the url of the call, `expected url is string` then the `System.Uri` text.
fn url(scope: &Scope) -> Result<net::Uri, Obj> {
    let text = match obj::payload(&scope::get(scope, "url")) {
        Payload::Str(s) => s.to_string(),
        _ => return Err(obj::err("expected url is string")),
    };

    match net::parse_uri(&text) {
        Ok(uri) => Ok(uri),
        Err(message) => Err(obj::err(message)),
    }
}

/// the body of the call, `expected body is string`.
fn body(scope: &Scope) -> Result<String, Obj> {
    match obj::payload(&scope::get(scope, "body")) {
        Payload::Str(s) => Ok(s.to_string()),
        _ => Err(obj::err("expected body is string")),
    }
}

/// the receiver of a member call, the `self.Value` of the reference.
fn client_self(scope: &Scope) -> Option<Rc<RefCell<HttpState>>> {
    match obj::payload(&scope::get(scope, "self")) {
        Payload::HttpClient(state) => Some(state.clone()),
        _ => None,
    }
}

/// `client.get` and the `Async` variant, the same work on the calling thread.
fn get_impl(scope: &Scope) -> Obj {
    let Some(state) = client_self(scope) else {
        return obj::err("expected 'http' object");
    };

    let uri = match url(scope) {
        Ok(uri) => uri,
        Err(error) => return error,
    };

    let result = net::request(&state.borrow(), "GET", &uri);
    match result {
        Ok(response) => {
            // `GetStringAsync` throws on a response that is not a success
            if !(200..300).contains(&response.code) {
                return obj::err(format!(
                    "Response status code does not indicate success: {} ({}).",
                    response.code, response.reason
                ));
            }
            obj::str(&response.body)
        }
        Err(message) => obj::err(message),
    }
}

/// `client.delete`, the response text of `HttpResponseMessage`.
fn delete_impl(scope: &Scope) -> Obj {
    let Some(state) = client_self(scope) else {
        return obj::err("expected 'http' object");
    };

    let uri = match url(scope) {
        Ok(uri) => uri,
        Err(error) => return error,
    };

    let result = net::request(&state.borrow(), "DELETE", &uri);
    match result {
        Ok(response) => obj::str(&response.body),
        Err(message) => obj::err(message),
    }
}

/// the `POST` request, `StringContent` with `text/plain`.
fn post_impl(scope: &Scope) -> Obj {
    let Some(state) = client_self(scope) else {
        return obj::err("expected 'http' object");
    };

    let uri = match url(scope) {
        Ok(uri) => uri,
        Err(error) => return error,
    };
    let body = match body(scope) {
        Ok(body) => body,
        Err(error) => return error,
    };

    let result = net::request_body(&state.borrow(), "POST", &uri, Some(&body));
    match result {
        Ok(response) => obj::str(&response.body),
        Err(message) => obj::err(message),
    }
}

/// the `PUT` request.
fn put_impl(scope: &Scope) -> Obj {
    let Some(state) = client_self(scope) else {
        return obj::err("expected 'http' object");
    };

    let uri = match url(scope) {
        Ok(uri) => uri,
        Err(error) => return error,
    };
    let body = match body(scope) {
        Ok(body) => body,
        Err(error) => return error,
    };

    let result = net::request_body(&state.borrow(), "PUT", &uri, Some(&body));
    match result {
        Ok(response) => obj::str(&response.body),
        Err(message) => obj::err(message),
    }
}

/// the `PATCH` request.
fn patch_impl(scope: &Scope) -> Obj {
    let Some(state) = client_self(scope) else {
        return obj::err("expected 'http' object");
    };

    let uri = match url(scope) {
        Ok(uri) => uri,
        Err(error) => return error,
    };
    let body = match body(scope) {
        Ok(body) => body,
        Err(error) => return error,
    };

    let result = net::request_body(&state.borrow(), "PATCH", &uri, Some(&body));
    match result {
        Ok(response) => obj::str(&response.body),
        Err(message) => obj::err(message),
    }
}

/// `client.head`, the status code of the response.
fn head_impl(scope: &Scope) -> Obj {
    let Some(state) = client_self(scope) else {
        return obj::err("expected 'http' object");
    };

    let uri = match url(scope) {
        Ok(uri) => uri,
        Err(error) => return error,
    };

    let result = net::request(&state.borrow(), "HEAD", &uri);
    match result {
        Ok(response) => obj::int(response.code as i64),
        Err(message) => obj::err(message),
    }
}

/// `client.options`, the methods of the `Allow` header.
fn options_impl(scope: &Scope) -> Obj {
    let Some(state) = client_self(scope) else {
        return obj::err("expected 'http' object");
    };

    let uri = match url(scope) {
        Ok(uri) => uri,
        Err(error) => return error,
    };

    let result = net::request(&state.borrow(), "OPTIONS", &uri);
    match result {
        Ok(response) => {
            let mut methods: Vec<Obj> = Vec::new();

            for (name, value) in &response.headers {
                if !name.eq_ignore_ascii_case("Allow") {
                    continue;
                }
                for method in value.split(',') {
                    let method = method.trim();
                    if !method.is_empty() {
                        methods.push(obj::str(method));
                    }
                }
            }

            collections::list(methods)
        }
        Err(message) => obj::err(message),
    }
}

fn connect_impl(_scope: &Scope) -> Obj {
    obj::obj_new(
        "http_client",
        Payload::HttpClient(Rc::new(RefCell::new(HttpState {
            headers: Vec::new(),
        }))),
    )
}

fn set_header_impl(scope: &Scope) -> Obj {
    let Some(state) = client_self(scope) else {
        return obj::err("expected 'http' object");
    };

    let key = match obj::payload(&scope::get(scope, "key")) {
        Payload::Str(s) => s.to_string(),
        _ => return obj::err("expected key is string"),
    };
    let value = match obj::payload(&scope::get(scope, "value")) {
        Payload::Str(s) => s.to_string(),
        _ => return obj::err("expected value is string"),
    };

    // the reference removes the header and adds it again
    let mut headers = state.borrow_mut();
    headers.headers.retain(|(name, _)| name != &key);
    headers.headers.push((key, value));

    obj::none()
}

fn clear_headers_impl(scope: &Scope) -> Obj {
    let Some(state) = client_self(scope) else {
        return obj::err("expected 'http' object");
    };

    state.borrow_mut().headers.clear();

    obj::none()
}

/// `Un.Native.Http`'s member map, the module `use http` binds.
pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    vec![(
        "connect".to_owned(),
        native(closure, "connect", Vec::new(), connect_impl),
    )]
}

/// `Un.Object.Web.HttpClient`'s `[Native]` member table, the `Global.originalClasses` entry of
/// the `http_client` type. the object's type name is `http_client`, so member lookup resolves it.
pub fn client_members(closure: &Context) -> Vec<(String, Obj)> {
    let one = || vec![arg("url", true, false, false)];
    let two = || vec![arg("url", true, false, false), arg("body", true, false, false)];

    vec![
        (
            "set_header".to_owned(),
            native(
                closure,
                "set_header",
                vec![arg("key", true, false, false), arg("value", true, false, false)],
                set_header_impl,
            ),
        ),
        (
            "clear_headers".to_owned(),
            native(closure, "clear_headers", Vec::new(), clear_headers_impl),
        ),
        (
            "get".to_owned(),
            native(closure, "get", one(), get_impl),
        ),
        (
            "get_async".to_owned(),
            native(closure, "get_async", one(), get_impl),
        ),
        (
            "delete".to_owned(),
            native(closure, "delete", one(), delete_impl),
        ),
        (
            "delete_async".to_owned(),
            native(closure, "delete_async", one(), delete_impl),
        ),
        (
            "post".to_owned(),
            native(closure, "post", two(), post_impl),
        ),
        (
            "post_async".to_owned(),
            native(closure, "post_async", two(), post_impl),
        ),
        (
            "put".to_owned(),
            native(closure, "put", two(), put_impl),
        ),
        (
            "put_async".to_owned(),
            native(closure, "put_async", two(), put_impl),
        ),
        (
            "patch".to_owned(),
            native(closure, "patch", two(), patch_impl),
        ),
        (
            "patch_async".to_owned(),
            native(closure, "patch_async", two(), patch_impl),
        ),
        (
            "head".to_owned(),
            native(closure, "head", one(), head_impl),
        ),
        (
            "head_async".to_owned(),
            native(closure, "head_async", one(), head_impl),
        ),
        (
            "options".to_owned(),
            native(closure, "options", one(), options_impl),
        ),
        (
            "options_async".to_owned(),
            native(closure, "options_async", one(), options_impl),
        ),
    ]
}
