//! the TCP client behind the `http` and `ws` modules.
//!
//! the reference runs on `System.Net.Http.HttpClient` and `System.Net.WebSockets.ClientWebSocket`.
//! both use the system TLS stack, and this crate is std only, so `https://` and `wss://` are not
//! reachable. the .NET network texts are not reproducible, so every connect and handshake failure
//! answers the .NET connect failure text (D-055). the `System.Uri` texts are deterministic, so
//! `parse_uri` reproduces them.

use std::io::Read;
use std::io::Write;
use std::net::TcpStream;
use std::time::{SystemTime, UNIX_EPOCH};

use super::random::Rng;

/// `new Uri("")`
const EMPTY: &str = "Invalid URI: The URI is empty.";
/// `new Uri("not a url")`
const FORMAT: &str = "Invalid URI: The format of the URI could not be determined.";
/// `new Uri("ws:")`
const AUTHORITY: &str = "Invalid URI: The Authority/Host could not be parsed.";
/// `new Uri("ws://")`
const HOST: &str = "Invalid URI: The hostname could not be parsed.";
/// `new Uri("ws://x:99999/")`
const PORT: &str = "Invalid URI: Invalid port specified.";

/// the .NET text of every connect failure, the message the reference gives for
/// a refused, a dropped, and an unreachable peer.
pub const CONNECT: &str = "Unable to connect to the remote server";

/// the RFC 6455 handshake magic string.
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// a parsed `System.Uri`. the request target keeps the path and the query, and drops the fragment.
pub struct Uri {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
}

/// the port of a scheme with no explicit port.
fn default_port(scheme: &str) -> u16 {
    match scheme {
        "http" | "ws" => 80,
        "https" | "wss" => 443,
        "ftp" => 21,
        _ => 0,
    }
}

/// a scheme is a letter, then letters, digits, `+`, `-`, `.`.
fn valid_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

/// a host is letters, digits, `.`, `-`, `_`, `%`, and the brackets, digits, letters, `:` and
/// `.` of a bracketed IPv6 host.
fn valid_host(host: &str) -> bool {
    if host.is_empty() {
        return false;
    }
    if host.starts_with('[') {
        return host.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.' || c == '%');
    }
    host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == '%')
}

/// `new Uri(text)` — the messages of .NET's `Uri` parser.
pub fn parse_uri(text: &str) -> Result<Uri, String> {
    if text.is_empty() {
        return Err(EMPTY.to_owned());
    }

    // the scheme is the text before the first ':'
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

    // the authority is the text between `//` and the first `/`, `?` or `#`
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

    // the userinfo is dropped
    let authority = match authority.rfind('@') {
        Some(index) => &authority[index + 1..],
        None => authority,
    };

    // the host and the port, the bracketed IPv6 host keeps its brackets
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

    // the request target keeps the path and the query, the fragment is dropped
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

// ---------------------------------------------------------------------------
// the HTTP/1.1 client
// ---------------------------------------------------------------------------

/// the default request headers of `System.Net.Http.HttpClient`.
pub struct HttpState {
    pub headers: Vec<(String, String)>,
}

/// a response line, the headers and the body text.
pub struct Response {
    pub code: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// the request line and the headers, the `Connection: close` header ends the body so the
/// read reaches the end of the stream.
fn build_request(state: &HttpState, method: &str, uri: &Uri, body: Option<&str>) -> String {
    let mut request = format!("{method} {} HTTP/1.1\r\n", uri.path);

    // the `Host` header keeps the port when the port is not the scheme default
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

/// the bytes up to the first `\r\n\r\n`, the end of the header block.
fn head_end(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|window| window == b"\r\n\r\n")
}

/// the offset of the first `\r\n` after `from`, the end of a chunk size line.
fn line_end(data: &[u8], from: usize) -> Option<usize> {
    data[from..].windows(2).position(|window| window == b"\r\n")
}

/// `Transfer-Encoding: chunked` bodies, the reference's chunked reader.
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
        // the chunk is followed by a line, the trailer lines are ignored
        match line_end(data, index) {
            Some(offset) => index += offset + 2,
            None => return Ok(String::from_utf8_lossy(&out).to_string()),
        }
    }
}

/// the response text, `code` `reason`, `headers`, `body`.
fn parse_response(data: &[u8]) -> Result<Response, String> {
    let split = match head_end(data) {
        Some(index) => index,
        None => return Err(CONNECT.to_owned()),
    };

    let head = String::from_utf8_lossy(&data[..split]).to_string();
    let mut lines = head.split("\r\n");

    // the status line is `HTTP/1.1 200 OK`
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
        String::from_utf8_lossy(&data[split + 4..]).to_string()
    };

    Ok(Response {
        code,
        reason,
        headers,
        body,
    })
}

/// one request, `Connection: close`, so the whole response is read to the end of the stream.
pub fn request(state: &HttpState, method: &str, uri: &Uri) -> Result<Response, String> {
    request_body(state, method, uri, None)
}

/// one request with a body, `Content-Type: text/plain`, the reference's `StringContent`.
pub fn request_body(
    state: &HttpState,
    method: &str,
    uri: &Uri,
    body: Option<&str>,
) -> Result<Response, String> {
    // `https` is out of scope, the message is the connect failure text
    if uri.scheme != "http" {
        return Err(CONNECT.to_owned());
    }

    let mut stream = match TcpStream::connect((uri.host.as_str(), uri.port)) {
        Ok(stream) => stream,
        Err(_) => return Err(CONNECT.to_owned()),
    };

    if write!(stream, "{}", build_request(state, method, uri, body)).is_err() {
        return Err(CONNECT.to_owned());
    }

    let mut data = Vec::new();
    if stream.read_to_end(&mut data).is_err() {
        return Err(CONNECT.to_owned());
    }

    parse_response(&data)
}

// ---------------------------------------------------------------------------
// the RFC 6455 client
// ---------------------------------------------------------------------------

/// the state of a socket, the connected stream and the .NET state name of `WebSocketState`.
pub struct WsState {
    pub stream: Option<TcpStream>,
    pub open: bool,
    pub state: String,
}

/// a value seeded from the system clock, the mask key and the handshake key.
fn random_bytes(count: usize) -> Vec<u8> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    let mut rng = Rng::new(nanos);
    let mut bytes = Vec::new();
    for _ in 0..count {
        bytes.push((rng.next_u64() & 0xFF) as u8);
    }
    bytes
}

/// base64, the `Convert.ToBase64String` of the handshake.
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

/// SHA-1, the `SHA1` of the handshake accept value.
pub fn sha1(bytes: &[u8]) -> [u8; 20] {
    let mut hash = [
        0x6745_2301u32,
        0xEFCD_AB89,
        0x98BAD_CFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];

    // the padding is the 0x80 byte, the zeros of the block, and the bit length
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

/// the bytes up to the end of the header block, the handshake response.
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

/// the upgrade request, and the 101 response check.
pub fn ws_connect(uri: &Uri) -> Result<WsState, String> {
    // `wss` is out of scope, the message is the connect failure text
    if uri.scheme != "ws" {
        return Err(CONNECT.to_owned());
    }

    let mut stream = match TcpStream::connect((uri.host.as_str(), uri.port)) {
        Ok(stream) => stream,
        Err(_) => return Err(CONNECT.to_owned()),
    };

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

    // the status line is `HTTP/1.1 101 Switching Protocols`, so the code is the second field
    if head.lines().next().unwrap_or("").split_whitespace().nth(1) != Some("101") {
        return Err(CONNECT.to_owned());
    }

    // the accept value is the SHA-1 of the key and the magic string
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

/// one frame, `payload` is the unmasked body, `opcode` is 1 (text), 8 (close), 0 (continuation),
/// and `fin` tells whether the message ends with this frame.
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
        100 => {
            let mut buffer = [0u8; 2];
            stream
                .read_exact(&mut buffer)
                .map_err(|_| CONNECT.to_owned())?;
            u16::from_be_bytes(buffer) as u64
        }
        101 => {
            let mut buffer = [0u8; 8];
            stream
                .read_exact(&mut buffer)
                .map_err(|_| CONNECT.to_owned())?;
            u64::from_be_bytes(buffer)
        }
        value => value,
    };

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

/// one masked client frame, the RFC 6455 client must mask every frame.
pub fn write_frame(stream: &mut TcpStream, opcode: u8, payload: &[u8]) -> Result<(), String> {
    let mut out = vec![0x80 | opcode];

    match payload.len() {
        len if len < 100 => out.push(len as u8 | 0x80),
        len if len < 65536 => {
            out.push(100 | 0x80);
            out.extend_from_slice(&(len as u16).to_be_bytes());
        }
        len => {
            out.push(101 | 0x80);
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
