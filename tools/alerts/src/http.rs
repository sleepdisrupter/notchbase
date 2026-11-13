//! A small HTTP/1.1 reader, just enough for a local webhook. Bodies need a Content-Length and
//! stay under 16 KiB.

use std::io::{ErrorKind, Read};

use serde_json::json;

pub const BODY_LIMIT: usize = 16 * 1024;
// curl sends `Expect: 100-continue` for larger bodies and waits for this.
pub const CONTINUE: &[u8] = b"HTTP/1.1 100 Continue\r\n\r\n";
const HEAD_LIMIT: usize = 8 * 1024;
const CHUNK: usize = 4096;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    pub fn param(&self, name: &str) -> Option<String> {
        pairs(&self.query).into_iter().find_map(|(key, value)| (key == name).then_some(value))
    }

    fn head(text: &str) -> Result<Self, Reply> {
        let mut lines = text.lines();
        let mut words = lines.next().unwrap_or_default().split_whitespace();
        let (Some(method), Some(target), Some(version), None) =
            (words.next(), words.next(), words.next(), words.next())
        else {
            return Err(Reply::bad("the request line is malformed"));
        };
        if !version.starts_with("HTTP/1.") || !target.starts_with('/') {
            return Err(Reply::bad("the request line is malformed"));
        }
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let headers = lines
            .filter(|line| !line.is_empty())
            .map(|line| {
                let (name, value) = line.split_once(':').ok_or_else(|| Reply::bad("a header is malformed"))?;
                Ok((name.trim().to_ascii_lowercase(), value.trim().to_owned()))
            })
            .collect::<Result<_, Reply>>()?;
        Ok(Self {
            method: method.to_owned(),
            path: path.to_owned(),
            query: query.to_owned(),
            headers,
            body: Vec::new(),
        })
    }

    fn length(&self) -> Result<usize, Reply> {
        if self.header("transfer-encoding").is_some() {
            return Err(Reply::fail(411, "send the body with a Content-Length"));
        }
        let Some(length) = self.header("content-length") else { return Ok(0) };
        match length.parse() {
            Ok(length) if length > BODY_LIMIT => Err(Reply::too_large()),
            Ok(length) => Ok(length),
            Err(_) => Err(Reply::bad("Content-Length isn't a number")),
        }
    }

    fn continues(&self) -> bool {
        self.header("expect").is_some_and(|expect| expect.eq_ignore_ascii_case("100-continue"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    pub error: Option<String>,
    pub allow: Option<&'static str>,
}

impl Reply {
    pub fn ok() -> Self {
        Self { status: 200, error: None, allow: None }
    }

    pub fn fail(status: u16, error: impl Into<String>) -> Self {
        Self { status, error: Some(error.into()), allow: None }
    }

    pub fn bad(error: impl Into<String>) -> Self {
        Self::fail(400, error)
    }

    pub fn too_large() -> Self {
        Self::fail(413, format!("requests are limited to {} KiB", BODY_LIMIT / 1024))
    }

    pub fn not_allowed(allow: &'static str) -> Self {
        Self { allow: Some(allow), ..Self::fail(405, format!("use {allow}")) }
    }

    pub fn bytes(&self) -> Vec<u8> {
        let body = match &self.error {
            None => json!({ "ok": true }),
            Some(error) => json!({ "ok": false, "error": error }),
        }
        .to_string();
        let mut extra = String::new();
        if let Some(allow) = self.allow {
            extra.push_str(&format!("Allow: {allow}\r\n"));
        }
        if self.status == 401 {
            extra.push_str("WWW-Authenticate: Bearer\r\n");
        }
        format!(
            "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n{extra}\r\n{body}",
            self.status,
            reason(self.status),
            body.len(),
        )
        .into_bytes()
    }
}

pub fn receive(mut reader: impl Read, proceed: impl FnOnce()) -> Result<Request, Reply> {
    let mut buffer = Vec::new();
    let end = loop {
        if let Some(end) = head_end(&buffer) {
            break end;
        }
        if buffer.len() > HEAD_LIMIT {
            return Err(Reply::too_large());
        }
        fill(&mut reader, &mut buffer)?;
    };
    if end > HEAD_LIMIT {
        return Err(Reply::too_large());
    }
    let head = std::str::from_utf8(&buffer[..end]).map_err(|_| Reply::bad("the request head isn't UTF-8"))?;
    let mut request = Request::head(head)?;
    let total = end + request.length()?;
    if buffer.len() < total && request.continues() {
        proceed();
    }
    while buffer.len() < total {
        fill(&mut reader, &mut buffer)?;
    }
    request.body = buffer[end..total].to_vec();
    Ok(request)
}

pub fn pairs(text: &str) -> Vec<(String, String)> {
    text.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(key), decode(value))
        })
        .collect()
}

pub fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = if bytes[at] == b'%' { bytes.get(at + 1..at + 3).and_then(hex) } else { None };
        match (bytes[at], escaped) {
            (_, Some(byte)) => {
                decoded.push(byte);
                at += 3;
            }
            (b'+', None) => {
                decoded.push(b' ');
                at += 1;
            }
            (byte, None) => {
                decoded.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex(pair: &[u8]) -> Option<u8> {
    if !pair.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()
}

fn head_end(bytes: &[u8]) -> Option<usize> {
    let find =
        |needle: &[u8]| bytes.windows(needle.len()).position(|window| window == needle).map(|at| at + needle.len());
    [find(b"\r\n\r\n"), find(b"\n\n")].into_iter().flatten().min()
}

fn fill(reader: &mut impl Read, buffer: &mut Vec<u8>) -> Result<(), Reply> {
    let mut chunk = [0; CHUNK];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => return Err(Reply::bad("the request ended early")),
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                return Ok(());
            }
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return Err(Reply::fail(408, "the request took too long"));
            }
            Err(_) => return Err(Reply::bad("the connection failed")),
        }
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        411 => "Length Required",
        413 => "Content Too Large",
        _ => "Error",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    fn read(raw: &str) -> Result<Request, Reply> {
        receive(raw.as_bytes(), || {})
    }

    fn status(raw: &str) -> u16 {
        read(raw).unwrap_err().status
    }

    #[test]
    fn request_splits_into_parts() {
        let request =
            read("POST /alert?token=abc HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 5\r\n\r\nhello").unwrap();
        assert_eq!(
            (request.method.as_str(), request.path.as_str(), request.query.as_str()),
            ("POST", "/alert", "token=abc")
        );
        assert_eq!(request.header("HOST"), Some("127.0.0.1"));
        assert_eq!(request.param("token").as_deref(), Some("abc"));
        assert_eq!(request.body, b"hello");
    }

    #[test]
    fn tolerates_bare_newlines() {
        let request = read("GET /alert HTTP/1.0\nHost: localhost\n\nleftover").unwrap();
        assert_eq!(request.header("host"), Some("localhost"));
        assert!(request.body.is_empty());
        let request = read("POST /alert HTTP/1.1\nContent-Length: 6\n\na\r\n\r\nb").unwrap();
        assert_eq!(request.body, b"a\r\n\r\nb");
    }

    #[test]
    fn refuses_bodies_over_16_kib() {
        assert_eq!(status(&format!("POST /alert HTTP/1.1\r\nContent-Length: {}\r\n\r\n", BODY_LIMIT + 1)), 413);
        let full = format!("POST /alert HTTP/1.1\r\nContent-Length: {BODY_LIMIT}\r\n\r\n{}", "x".repeat(BODY_LIMIT));
        assert_eq!(read(&full).unwrap().body.len(), BODY_LIMIT);
    }

    #[test]
    fn refuses_endless_heads() {
        assert_eq!(status(&format!("GET /alert HTTP/1.1\r\nX-Pad: {}\r\n\r\n", "x".repeat(HEAD_LIMIT))), 413);
        assert_eq!(status(&format!("GET /alert HTTP/1.1\r\nX-Pad: {}", "x".repeat(3 * HEAD_LIMIT))), 413);
    }

    #[test]
    fn rejects_malformed_requests() {
        assert_eq!(status("HELLO\r\n\r\n"), 400);
        assert_eq!(status("GET alert HTTP/1.1\r\n\r\n"), 400);
        assert_eq!(status("GET /alert SPDY/3\r\n\r\n"), 400);
        assert_eq!(status("GET /alert HTTP/1.1\r\nNoColon\r\n\r\n"), 400);
        assert_eq!(status("GET /alert HTTP/1.1\r\n"), 400);
        assert_eq!(status("POST /alert HTTP/1.1\r\nContent-Length: 10\r\n\r\nshort"), 400);
        assert_eq!(status("POST /alert HTTP/1.1\r\nContent-Length: ten\r\n\r\n"), 400);
        assert_eq!(status("POST /alert HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"), 411);
    }

    #[test]
    fn continue_only_while_body_is_due() {
        let head = b"POST /alert HTTP/1.1\r\nExpect: 100-continue\r\nContent-Length: 2\r\n\r\n";
        let mut told = 0;
        let request = receive((&head[..]).chain(&b"hi"[..]), || told += 1).unwrap();
        assert_eq!((told, request.body.as_slice()), (1, &b"hi"[..]));
        let mut told = 0;
        receive([&head[..], b"hi"].concat().as_slice(), || told += 1).unwrap();
        assert_eq!(told, 0);
    }

    #[test]
    fn query_is_percent_decoded() {
        assert_eq!(decode("Build+done%21%20%F0%9F%8E%89"), "Build done! 🎉");
        assert_eq!(decode("100%+sure%zz%4"), "100% sure%zz%4");
        assert_eq!(decode("%2B%2b"), "++");
        let decoded = pairs("title=a%26b&empty=&flag&=x&&tone=red");
        let expected = [("title", "a&b"), ("empty", ""), ("flag", ""), ("", "x"), ("tone", "red")];
        assert_eq!(decoded, expected.map(|(key, value)| (key.to_owned(), value.to_owned())));
    }

    #[test]
    fn replies_are_closing_json() {
        let text = String::from_utf8(Reply::not_allowed("POST").bytes()).unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap();
        assert!(head.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"));
        assert!(head.contains("Allow: POST") && head.contains("Connection: close"));
        assert!(head.contains(&format!("Content-Length: {}", body.len())));
        assert_eq!(serde_json::from_str::<Value>(body).unwrap(), json!({ "ok": false, "error": "use POST" }));
        let ok = String::from_utf8(Reply::ok().bytes()).unwrap();
        assert!(ok.starts_with("HTTP/1.1 200 OK\r\n") && ok.ends_with("\r\n\r\n{\"ok\":true}"));
        assert!(String::from_utf8(Reply::fail(401, "no").bytes()).unwrap().contains("WWW-Authenticate: Bearer\r\n"));
    }
}
