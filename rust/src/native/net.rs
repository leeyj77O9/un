use std::io::Read;
use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;
use super::random::Rng;
const EMPTY: &str = "Invalid URI: The URI is empty.";
const FORMAT: &str = "Invalid URI: The format of the URI could not be determined.";
const AUTHORITY: &str = "Invalid URI: The Authority/Host could not be parsed.";
const HOST: &str = "Invalid URI: The hostname could not be parsed.";
const PORT: &str = "Invalid URI: Invalid port specified.";
const SCHEME_HTTP: &str = "Only Uris starting with 'http://' are supported. (Parameter 'uri')";
const SCHEME_WS: &str = "Only Uris starting with 'ws://' are supported. (Parameter 'uri')";
const MAX_FRAME_PAYLOAD: u64 = 1_048_576;
const MAX_HTTP_RESPONSE: u64 = 64 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const IO_TIMEOUT: Duration = Duration::from_secs(30);

pub const CONNECT: &str = "Unable to connect to the remote server";
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

fn connect_tcp(host: &str, port: u16) -> Result<TcpStream, String> {
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|_| CONNECT.to_owned())?;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
                let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
                return Ok(stream);
            }
            Err(_) => continue,
        }
    }
    Err(CONNECT.to_owned())
}
#[derive(Clone)]
pub struct Uri {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
}
fn default_port(scheme: &str) -> u16 {
    match scheme {
        "http" | "ws" => 80,
        "https" | "wss" => 443,
        "ftp" => 21,
        _ => 0,
    }
}
fn valid_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}
fn valid_host(host: &str) -> bool {
    if host.is_empty() {
        return false;
    }
    if host.starts_with('[') {
        return host.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.' || c == '%');
    }
    host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == '%')
}
pub fn parse_uri(text: &str) -> Result<Uri, String> {
    if text.is_empty() {
        return Err(EMPTY.to_owned());
    }
    if text.contains('\r') || text.contains('\n') {
        return Err(FORMAT.to_owned());
    }

    let colon = match text.find(':') {
        Some(index) => index,
        None => return Err(FORMAT.to_owned()),
    };
    let scheme = text[..colon].to_lowercase();
    if !valid_scheme(&scheme) {
        return Err(FORMAT.to_owned());
    }
    let rest = &text[colon + 1..];
    if rest.is_empty() {
        return Err(AUTHORITY.to_owned());
    }

    if !rest.starts_with("//") {
        return Err(AUTHORITY.to_owned());
    }
    let after = &rest[2..];
    let authority = match after.find(['/', '?', '#']) {
        Some(index) => &after[..index],
        None => after,
    };
    if authority.is_empty() {
        return Err(HOST.to_owned());
    }

    let authority = match authority.rfind('@') {
        Some(index) => &authority[index + 1..],
        None => authority,
    };

    let (host, port) = if authority.starts_with('[') {
        let end = match authority.find(']') {
            Some(index) => index,
            None => return Err(HOST.to_owned()),
        };
        let host = &authority[..=end];
        let tail = &authority[end + 1..];
        if tail.is_empty() {
            (host.to_owned(), None)
        } else if tail.starts_with(':') {
            (host.to_owned(), Some(&tail[1..]))
        } else {
            return Err(HOST.to_owned());
        }
    } else {
        match authority.rfind(':') {
            Some(index) => (authority[..index].to_owned(), Some(&authority[index + 1..])),
            None => (authority.to_owned(), None),
        }
    };
    if !valid_host(&host) {
        return Err(HOST.to_owned());
    }
    let port = match port {
        Some(text) => {
            if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
                return Err(PORT.to_owned());
            }
            let value: u32 = match text.parse() {
                Ok(value) => value,
                Err(_) => return Err(PORT.to_owned()),
            };
            if value > 65535 {
                return Err(PORT.to_owned());
            }
            value as u16
        }
        None => default_port(&scheme),
    };

    let path = match after.find(['/', '?', '#']) {
        Some(index) => {
            let path = &after[index..];
            match path.find('#') {
                Some(index) => &path[..index],
                None => path,
            }
            .to_owned()
        }
        None => "/".to_owned(),
    };
    Ok(Uri {
        scheme,
        host,
        port,
        path,
    })
}
#[derive(Debug)]
pub struct HttpState {
    pub headers: Vec<(String, String)>,
}
#[derive(Debug)]
pub struct Response {
    pub code: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}
fn build_request(state: &HttpState, method: &str, uri: &Uri, body: Option<&str>) -> String {
    let mut request = format!("{method} {} HTTP/1.1\r\n", uri.path);

    if uri.port == default_port(&uri.scheme) {
        request.push_str(&format!("Host: {}\r\n", uri.host));
    } else {
        request.push_str(&format!("Host: {}:{}\r\n", uri.host, uri.port));
    }
    request.push_str("User-Agent: Un\r\nAccept: */*\r\nConnection: close\r\n");
    for (name, value) in &state.headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(body) = body {
        request.push_str(&format!(
            "Content-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n",
            body.len()
        ));
        request.push_str(body);
    } else {
        request.push_str("\r\n");
    }
    request
}
fn head_end(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|window| window == b"\r\n\r\n")
}
fn line_end(data: &[u8], from: usize) -> Option<usize> {
    data[from..].windows(2).position(|window| window == b"\r\n")
}
fn decode_chunked(data: &[u8]) -> Result<String, String> {
    let mut out = Vec::new();
    let mut index = 0;
    loop {
        let end = match line_end(data, index) {
            Some(offset) => index + offset,
            None => return Err(CONNECT.to_owned()),
        };
        let size = match u32::from_str_radix(
            String::from_utf8_lossy(&data[index..end]).split(';').next().unwrap_or("").trim(),
            16,
        ) {
            Ok(size) => size as usize,
            Err(_) => return Err(CONNECT.to_owned()),
        };
        if size == 0 {
            return Ok(String::from_utf8_lossy(&out).to_string());
        }
        let start = end + 2;
        if start + size > data.len() {
            return Err(CONNECT.to_owned());
        }
        out.extend_from_slice(&data[start..start + size]);
        index = start + size;

        match line_end(data, index) {
            Some(offset) => index += offset + 2,
            None => return Ok(String::from_utf8_lossy(&out).to_string()),
        }
    }
}
fn parse_response(method: &str, data: &[u8]) -> Result<Response, String> {
    let split = match head_end(data) {
        Some(index) => index,
        None => return Err(CONNECT.to_owned()),
    };
    let head = String::from_utf8_lossy(&data[..split]).to_string();
    let mut lines = head.split("\r\n");

    let fields: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
    let code = match fields.get(1).and_then(|field| field.parse::<u16>().ok()) {
        Some(code) => code,
        None => return Err(CONNECT.to_owned()),
    };
    let reason = fields[2..].join(" ");
    let mut headers = Vec::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_owned(), value.trim().to_string()));
        }
    }
    let chunked = headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("Transfer-Encoding") && value.to_lowercase().contains("chunked")
    });
    let body = if chunked {
        decode_chunked(&data[split + 4..])?
    } else {
        let bodyless = (100..200).contains(&code) || code == 204 || code == 304
            || method.eq_ignore_ascii_case("HEAD");
        let declared = if bodyless {
            None
        } else {
            headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("Content-Length"))
                .and_then(|(_, value)| value.trim().parse::<u64>().ok())
        };
        let body_bytes = &data[split + 4..];
        if let Some(expected) = declared {
            if body_bytes.len() as u64 != expected {
                return Err("unexpected end of http response".to_string());
            }
        }
        String::from_utf8_lossy(body_bytes).to_string()
    };
    Ok(Response {
        code,
        reason,
        headers,
        body,
    })
}
fn send_once(
    state: &HttpState,
    method: &str,
    uri: &Uri,
    body: Option<&str>,
) -> Result<Response, String> {
    if uri.scheme != "http" {
        return Err(SCHEME_HTTP.to_owned());
    }
    let mut stream = connect_tcp(&uri.host, uri.port)?;
    if write!(stream, "{}", build_request(state, method, uri, body)).is_err() {
        return Err(CONNECT.to_owned());
    }
    let mut limited = stream.take(MAX_HTTP_RESPONSE + 1);
    let mut data = Vec::new();
    if limited.read_to_end(&mut data).is_err() {
        return Err(CONNECT.to_owned());
    }
    if data.len() as u64 > MAX_HTTP_RESPONSE {
        return Err("http response too large".to_string());
    }
    parse_response(method, &data)
}

fn resolve_redirect(base: &Uri, location: &str) -> Result<Uri, String> {
    let location = location.trim();
    let lower = location.to_ascii_lowercase();
    if lower.starts_with("http://") {
        return parse_uri(location);
    }
    if lower.starts_with("https://") {
        return Err(SCHEME_HTTP.to_owned());
    }
    if let Some(rest) = location.strip_prefix("//") {
        return parse_uri(&format!("http://{rest}"));
    }
    if location.starts_with('/') {
        return Ok(Uri {
            scheme: base.scheme.clone(),
            host: base.host.clone(),
            port: base.port,
            path: normalize_path(location),
        });
    }
    if let Some(query) = location.strip_prefix('?') {
        let mut path = base.path.clone();
        if let Some(q) = path.find('?') {
            path.truncate(q);
        }
        path.push('?');
        path.push_str(query);
        return Ok(Uri {
            scheme: base.scheme.clone(),
            host: base.host.clone(),
            port: base.port,
            path,
        });
    }
    if location.starts_with('#') {
        return Ok(base.clone());
    }
    let mut base_path = base.path.clone();
    if let Some(q) = base_path.find('?') {
        base_path.truncate(q);
    }
    let dir_end = base_path.rfind('/').map(|i| i + 1).unwrap_or(0);
    let merged = format!("{}{}", &base_path[..dir_end], location);
    Ok(Uri {
        scheme: base.scheme.clone(),
        host: base.host.clone(),
        port: base.port,
        path: normalize_path(&merged),
    })
}

fn normalize_path(path: &str) -> String {
    let (path_part, query) = match path.find('?') {
        Some(i) => (&path[..i], Some(&path[i..])),
        None => (path, None),
    };
    let absolute = path_part.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in path_part.split('/') {
        match seg {
            "" => {}
            "." => {}
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    let mut joined = out.join("/");
    if absolute {
        if !joined.starts_with('/') {
            joined.insert(0, '/');
        }
    }
    match query {
        Some(q) => format!("{joined}{q}"),
        None => joined,
    }
}

pub fn request(state: &HttpState, method: &str, uri: &Uri) -> Result<Response, String> {
    request_body(state, method, uri, None)
}
pub fn request_body(
    state: &HttpState,
    method: &str,
    uri: &Uri,
    body: Option<&str>,
) -> Result<Response, String> {
    let mut method = method.to_string();
    let mut uri = uri.clone();
    let mut body = body.map(String::from);
    const MAX_REDIRECTS: usize = 8;
    for hop in 0..=MAX_REDIRECTS {
        let response = send_once(state, &method, &uri, body.as_deref())?;
        let redirect = matches!(response.code, 301 | 302 | 303 | 307 | 308);
        if !redirect {
            return Ok(response);
        }
        let location = match response
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("Location"))
            .map(|(_, value)| value.trim().to_string())
        {
            Some(value) if !value.is_empty() => value,
            _ => return Ok(response),
        };
        if hop == MAX_REDIRECTS {
            return Err("http redirect limit exceeded".to_string());
        }
        let next = resolve_redirect(&uri, &location)?;
        let keep_method = matches!(response.code, 307 | 308)
            || matches!(method.as_str(), "GET" | "HEAD");
        if !keep_method {
            method = "GET".to_string();
            body = None;
        }
        uri = next;
    }
    unreachable!()
}
pub struct WsState {
    pub stream: Option<TcpStream>,
    pub open: bool,
    pub state: String,
}
fn random_bytes(count: usize) -> Vec<u8> {
    let mut rng = Rng::new(crate::runtime::clock::seed_nanos());
    let mut bytes = Vec::new();
    for _ in 0..count {
        bytes.push((rng.next_u64() & 0xFF) as u8);
    }
    bytes
}
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        out.push(TABLE[(a >> 2) as usize] as char);
        out.push(TABLE[((a & 0x03) << 4 | b >> 4) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((b & 0x0F) << 2 | c >> 6) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(c & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
pub fn sha1(bytes: &[u8]) -> [u8; 20] {
    let mut hash = [
        0x6745_2301u32,
        0xEFCD_AB89,
        0x98BAD_CFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];

    let mut message = bytes.to_vec();
    let bits = (bytes.len() as u64) * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_be_bytes());
    for block in message.chunks(64) {
        let mut words = [0u32; 80];
        for index in 0..16 {
            words[index] = u32::from_be_bytes([
                block[index * 4],
                block[index * 4 + 1],
                block[index * 4 + 2],
                block[index * 4 + 3],
            ]);
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16]).rotate_left(1);
        }
        let mut a = hash[0];
        let mut b = hash[1];
        let mut c = hash[2];
        let mut d = hash[3];
        let mut e = hash[4];
        for index in 0..80 {
            let (f, k) = match index {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(words[index]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        hash[0] = hash[0].wrapping_add(a);
        hash[1] = hash[1].wrapping_add(b);
        hash[2] = hash[2].wrapping_add(c);
        hash[3] = hash[3].wrapping_add(d);
        hash[4] = hash[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for index in 0..5 {
        out[index * 4..index * 4 + 4].copy_from_slice(&hash[index].to_be_bytes());
    }
    out
}
fn read_head(stream: &mut TcpStream) -> Result<String, String> {
    let mut data = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if stream.read(&mut byte).map_err(|_| CONNECT.to_owned())? == 0 {
            return Err(CONNECT.to_owned());
        }
        data.push(byte[0]);
        if data.windows(4).any(|window| window == b"\r\n\r\n") {
            return Ok(String::from_utf8_lossy(&data).to_string());
        }
        if data.len() > 16384 {
            return Err(CONNECT.to_owned());
        }
    }
}
pub fn ws_connect(uri: &Uri) -> Result<WsState, String> {

    if uri.scheme != "ws" {
        return Err(SCHEME_WS.to_owned());
    }
    let mut stream = connect_tcp(&uri.host, uri.port)?;
    let key = base64(&random_bytes(16));
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n",
        uri.path, uri.host
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return Err(CONNECT.to_owned());
    }
    let head = read_head(&mut stream)?;

    if head.lines().next().unwrap_or("").split_whitespace().nth(1) != Some("101") {
        return Err(CONNECT.to_owned());
    }

    let accept = base64(&sha1(format!("{key}{WS_GUID}").as_bytes()));
    let matching = head.lines().any(|line| {
        let (name, value) = match line.split_once(':') {
            Some(pair) => pair,
            None => return false,
        };
        name.trim().eq_ignore_ascii_case("Sec-WebSocket-Accept") && value.trim() == accept
    });
    if !matching {
        return Err(CONNECT.to_owned());
    }
    Ok(WsState {
        stream: Some(stream),
        open: true,
        state: "Open".to_owned(),
    })
}
pub fn read_frame(stream: &mut TcpStream) -> Result<(u8, Vec<u8>, bool), String> {
    let mut header = [0u8; 2];
    stream
        .read_exact(&mut header)
        .map_err(|_| CONNECT.to_owned())?;
    let opcode = header[0] & 0x0F;
    let fin = header[0] & 0x80 != 0;
    let masked = header[1] & 0x80 != 0;
    let length = (header[1] & 0x7F) as u64;
    let length = match length {
        126 => {
            let mut buffer = [0u8; 2];
            stream
                .read_exact(&mut buffer)
                .map_err(|_| CONNECT.to_owned())?;
            u16::from_be_bytes(buffer) as u64
        }
        127 => {
            let mut buffer = [0u8; 8];
            stream
                .read_exact(&mut buffer)
                .map_err(|_| CONNECT.to_owned())?;
            u64::from_be_bytes(buffer)
        }
        value => value,
    };
    if length > MAX_FRAME_PAYLOAD {
        return Err(CONNECT.to_owned());
    }
    let mask = if masked {
        let mut buffer = [0u8; 4];
        stream
            .read_exact(&mut buffer)
            .map_err(|_| CONNECT.to_owned())?;
        buffer
    } else {
        [0u8; 4]
    };
    let mut payload = vec![0u8; length as usize];
    stream
        .read_exact(&mut payload)
        .map_err(|_| CONNECT.to_owned())?;
    if masked {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
    }
    Ok((opcode, payload, fin))
}
pub fn write_frame(stream: &mut TcpStream, opcode: u8, payload: &[u8]) -> Result<(), String> {
    let mut out = vec![0x80 | opcode];
    match payload.len() {
        len if len < 126 => out.push(len as u8 | 0x80),
        len if len < 65536 => {
            out.push(126 | 0x80);
            out.extend_from_slice(&(len as u16).to_be_bytes());
        }
        len => {
            out.push(127 | 0x80);
            out.extend_from_slice(&(len as u64).to_be_bytes());
        }
    }
    let mask = random_bytes(4);
    out.extend_from_slice(&mask);
    for (index, byte) in payload.iter().enumerate() {
        out.push(byte ^ mask[index % 4]);
    }
    stream.write_all(&out).map_err(|_| CONNECT.to_owned())
}

#[cfg(test)]
mod redirect_tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{channel, Receiver};
    use std::thread;

    struct RecordedRequest {
        method: String,
        path: String,
        body: String,
    }

    fn http_server(
        script: Vec<(u16, Option<&'static str>, &'static str)>,
    ) -> (String, Receiver<RecordedRequest>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = channel::<RecordedRequest>();
        thread::spawn(move || {
            let mut remaining = script;
            while let Some((code, location, body)) = remaining.first().copied() {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                let (method, path, body_read) = read_request(&mut stream);
                let _ = tx.send(RecordedRequest {
                    method,
                    path,
                    body: body_read,
                });
                let head = match location {
                    Some(loc) => format!(
                        "HTTP/1.1 {code} Redirect\r\nLocation: {loc}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    ),
                    None => format!(
                        "HTTP/1.1 {code} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    ),
                };
                let _ = stream.write_all(head.as_bytes());
                remaining.remove(0);
            }
        });
        (format!("http://127.0.0.1:{port}"), rx)
    }

    fn read_request(stream: &mut std::net::TcpStream) -> (String, String, String) {
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("request line");
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or("?").to_string();
        let path = parts.next().unwrap_or("/").to_string();
        let mut headers = Vec::new();
        loop {
            let mut h = String::new();
            reader.read_line(&mut h).expect("header");
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            headers.push(h.to_string());
        }
        let content_length: usize = headers
            .iter()
            .find_map(|h| {
                h.split_once(':')
                    .filter(|(name, _)| name.eq_ignore_ascii_case("Content-Length"))
                    .and_then(|(_, v)| v.trim().parse().ok())
            })
            .unwrap_or(0);
        let mut body = String::new();
        if content_length > 0 {
            let mut buf = vec![0u8; content_length];
            reader.read_exact(&mut buf).expect("body");
            body = String::from_utf8_lossy(&buf).to_string();
        }
        (method, path, body)
    }

    fn drain(rx: &Receiver<RecordedRequest>) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        while let Ok(req) = rx.recv_timeout(std::time::Duration::from_millis(500)) {
            out.push((req.method, req.path, req.body));
        }
        out
    }

    #[test]
    fn follows_301_and_302_chain_to_final_200() {
        let (base, rx) = http_server(vec![
            (302, Some("/middle"), "redirecting"),
            (301, Some("/final"), "redirecting again"),
            (200, None, "final-body"),
        ]);
        let uri = parse_uri(&format!("{base}/start")).expect("uri");
        let response = request_body(&HttpState { headers: Vec::new() }, "GET", &uri, None)
            .expect("ok");
        assert_eq!(response.code, 200);
        assert_eq!(response.body, "final-body");
        assert_eq!(
            drain(&rx),
            vec![
                ("GET".to_string(), "/start".to_string(), String::new()),
                ("GET".to_string(), "/middle".to_string(), String::new()),
                ("GET".to_string(), "/final".to_string(), String::new()),
            ]
        );
    }

    #[test]
    fn post_becomes_get_on_301() {
        let (base, rx) = http_server(vec![(301, Some("/echo"), "move"), (200, None, "ok")]);
        let uri = parse_uri(&format!("{base}/submit")).expect("uri");
        let response = request_body(
            &HttpState { headers: Vec::new() },
            "POST",
            &uri,
            Some("payload"),
        )
        .expect("ok");
        assert_eq!(response.code, 200);
        assert_eq!(
            drain(&rx),
            vec![
                ("POST".to_string(), "/submit".to_string(), "payload".to_string()),
                ("GET".to_string(), "/echo".to_string(), String::new()),
            ]
        );
    }

    #[test]
    fn head_is_preserved_on_301() {
        let (base, rx) = http_server(vec![(301, Some("/h1"), "move"), (200, None, "")]);
        let uri = parse_uri(&format!("{base}/h0")).expect("uri");
        let response =
            request_body(&HttpState { headers: Vec::new() }, "HEAD", &uri, None).expect("ok");
        assert_eq!(response.code, 200);
        assert_eq!(
            drain(&rx),
            vec![
                ("HEAD".to_string(), "/h0".to_string(), String::new()),
                ("HEAD".to_string(), "/h1".to_string(), String::new()),
            ]
        );
    }

    #[test]
    fn put_body_and_method_preserved_on_307() {
        let (base, rx) = http_server(vec![(307, Some("/p-next"), "temp"), (200, None, "ok")]);
        let uri = parse_uri(&format!("{base}/p-orig")).expect("uri");
        let response =
            request_body(&HttpState { headers: Vec::new() }, "PUT", &uri, Some("stuff"))
                .expect("ok");
        assert_eq!(response.code, 200);
        assert_eq!(
            drain(&rx),
            vec![
                ("PUT".to_string(), "/p-orig".to_string(), "stuff".to_string()),
                ("PUT".to_string(), "/p-next".to_string(), "stuff".to_string()),
            ]
        );
    }

    #[test]
    fn absolute_location_header_is_followed() {
        let (base, _rx) = http_server(vec![(200, None, "unreachable")]);
        let uri = parse_uri(&format!("{base}/absolute")).expect("uri");
        let resolved = resolve_redirect(&uri, &format!("{base}/elsewhere")).expect("resolve");
        assert_eq!(resolved.path, "/elsewhere");
        assert_eq!(
            format!("http://{}:{}", resolved.host, resolved.port),
            base
        );
    }

    #[test]
    fn relative_location_header_is_resolved() {
        let uri = parse_uri("http://example.com:8080/a/b/c?x=1").expect("uri");
        let resolved = resolve_redirect(&uri, "../sibling?y=2").expect("resolve");
        assert_eq!(resolved.path, "/a/sibling?y=2");
        let resolved = resolve_redirect(&uri, "/top").expect("resolve");
        assert_eq!(resolved.path, "/top");
        let resolved = resolve_redirect(&uri, "query-only?z=3").expect("resolve");
        assert_eq!(resolved.path, "/a/b/query-only?z=3");
        let resolved = resolve_redirect(&uri, "?onlyquery=1").expect("resolve");
        assert_eq!(resolved.path, "/a/b/c?onlyquery=1");
        let resolved = resolve_redirect(&uri, "plain-segment").expect("resolve");
        assert_eq!(resolved.path, "/a/b/plain-segment");
        let resolved = resolve_redirect(&uri, "..").expect("resolve");
        assert_eq!(resolved.path, "/a");
        let resolved = resolve_redirect(&uri, ".").expect("resolve");
        assert_eq!(resolved.path, "/a/b");
    }

    #[test]
    fn excess_redirects_error() {
        let (base, _rx) = http_server(vec![
            (301, Some("/loop"), "a"),
            (301, Some("/loop"), "b"),
            (301, Some("/loop"), "c"),
            (301, Some("/loop"), "d"),
            (301, Some("/loop"), "e"),
            (301, Some("/loop"), "f"),
            (301, Some("/loop"), "g"),
            (301, Some("/loop"), "h"),
            (301, Some("/loop"), "i"),
        ]);
        let uri = parse_uri(&format!("{base}/loop")).expect("uri");
        let response = request_body(&HttpState { headers: Vec::new() }, "GET", &uri, None);
        assert!(response.is_err());
        assert!(response.unwrap_err().contains("redirect"));
    }

    #[test]
    fn redirect_without_location_returns_response() {
        let (base, rx) = http_server(vec![(301, None, "body")]);
        let uri = parse_uri(&format!("{base}/nowhere")).expect("uri");
        let response = request_body(&HttpState { headers: Vec::new() }, "GET", &uri, None)
            .expect("ok");
        assert_eq!(response.code, 301);
        let req = rx
            .recv_timeout(std::time::Duration::from_millis(500))
            .expect("one request");
        assert_eq!(req.path, "/nowhere");
    }
}
