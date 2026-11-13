//! The webhook's routes: what a request asks for and whether it's allowed.

use notchbase_sdk::prelude::Value;

use crate::alert::{Alert, Fields};
use crate::http::{Reply, Request, pairs};

pub const ALERT: &str = "/alert";
const DISMISS: &str = "/alert/dismiss";
const LOCAL_HOSTS: [&str; 2] = ["127.0.0.1", "localhost"];
const BOM: &[u8] = b"\xEF\xBB\xBF"; // some Windows tools start the body with a byte order mark

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Show(Alert),
    Dismiss,
}

enum Route {
    Show,
    Dismiss,
}

pub fn answer(request: &Request, token: Option<&str>) -> Result<Action, Reply> {
    screen(request)?;
    let route = route(request)?;
    authorize(request, token)?;
    match route {
        Route::Show => Alert::read(&fields(request)?).map(Action::Show),
        Route::Dismiss => Ok(Action::Dismiss),
    }
}

/// Turns away web pages, which always send Origin or Sec-Fetch-Site, and any Host other than
/// this computer, which stops DNS rebinding.
fn screen(request: &Request) -> Result<(), Reply> {
    let from_a_page = request.header("origin").is_some()
        || request.header("sec-fetch-site").is_some_and(|site| !site.eq_ignore_ascii_case("none"));
    let foreign = request
        .header("host")
        .is_some_and(|host| !LOCAL_HOSTS.iter().any(|local| local.eq_ignore_ascii_case(hostname(host))));
    if from_a_page || foreign { Err(Reply::fail(403, "only apps on this computer can send alerts")) } else { Ok(()) }
}

fn hostname(host: &str) -> &str {
    host.rsplit_once(':')
        .filter(|(_, port)| port.bytes().all(|byte| byte.is_ascii_digit()))
        .map_or(host, |(name, _)| name)
}

fn route(request: &Request) -> Result<Route, Reply> {
    match (request.path.trim_end_matches('/'), request.method.as_str()) {
        (ALERT, "GET" | "POST") => Ok(Route::Show),
        (ALERT, "DELETE") | (DISMISS, "POST") => Ok(Route::Dismiss),
        (ALERT, _) => Err(Reply::not_allowed("GET, POST, DELETE")),
        (DISMISS, _) => Err(Reply::not_allowed("POST")),
        _ => Err(Reply::fail(404, "not found")),
    }
}

fn authorize(request: &Request, token: Option<&str>) -> Result<(), Reply> {
    let Some(token) = token else { return Ok(()) };
    let bearer = request
        .header("authorization")
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, given)| given.trim().to_owned());
    let given = bearer.or_else(|| request.param("token"));
    if given.is_some_and(|given| same(given.as_bytes(), token.as_bytes())) {
        Ok(())
    } else {
        Err(Reply::fail(401, "a valid token is required"))
    }
}

/// Compares in constant time, so the token can't be guessed from how long a refusal takes.
fn same(given: &[u8], token: &[u8]) -> bool {
    given.len() == token.len() && given.iter().zip(token).fold(0, |diff, (a, b)| diff | (a ^ b)) == 0
}

fn fields(request: &Request) -> Result<Fields, Reply> {
    let mut fields = form(&request.query);
    let body = request.body.strip_prefix(BOM).unwrap_or(&request.body).trim_ascii();
    match body.first() {
        None => {}
        Some(b'{' | b'[') => match serde_json::from_slice(body) {
            Ok(Value::Object(object)) => fields.extend(object),
            Ok(_) => return Err(Reply::bad("send a JSON object")),
            Err(err) => return Err(Reply::bad(format!("the JSON is invalid: {err}"))),
        },
        Some(_) => fields.extend(form(&String::from_utf8_lossy(body))),
    }
    Ok(fields)
}

fn form(text: &str) -> Fields {
    let mut fields = Fields::new();
    for (key, value) in pairs(text) {
        fields.entry(key).or_insert(Value::String(value));
    }
    fields
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use notchbase_sdk::prelude::{Icon, Tone};

    use super::*;
    use crate::alert::Stay;
    use crate::http::receive;

    fn ask(raw: &str, token: Option<&str>) -> Result<Action, Reply> {
        receive(raw.as_bytes(), || {}).and_then(|request| answer(&request, token))
    }

    fn post(body: &str) -> Result<Action, Reply> {
        let head = "POST /alert HTTP/1.1\r\nHost: 127.0.0.1:47832\r\nContent-Type: application/json";
        ask(&format!("{head}\r\nContent-Length: {}\r\n\r\n{body}", body.len()), None)
    }

    fn get(query: &str) -> Result<Action, Reply> {
        ask(&format!("GET /alert?{query} HTTP/1.1\r\nHost: localhost:47832\r\n\r\n"), None)
    }

    fn with(line: &str, token: Option<&str>) -> Result<Action, Reply> {
        ask(&format!("GET /alert?title=Hi HTTP/1.1\r\nHost: 127.0.0.1:47832\r\n{line}\r\n\r\n"), token)
    }

    fn shown(result: Result<Action, Reply>) -> Alert {
        match result {
            Ok(Action::Show(alert)) => alert,
            other => panic!("{other:?}"),
        }
    }

    fn status(result: Result<Action, Reply>) -> u16 {
        result.unwrap_err().status
    }

    #[test]
    fn json_alert_carries_every_field() {
        let body = r#"{"title":"Deploy","message":"All green","icon":"checkmark","tone":"green","duration_ms":6000,"shake":false}"#;
        let alert = shown(post(body));
        assert_eq!((alert.title.as_str(), alert.message.as_deref()), ("Deploy", Some("All green")));
        assert_eq!((alert.icon, alert.tone), (Icon::Checkmark, Tone::Green));
        assert_eq!((alert.stay, alert.shake), (Stay::For(Duration::from_millis(6000)), false));
    }

    #[test]
    fn text_stands_in_for_message() {
        let alert = shown(post("\u{feff}{\"title\":\"Hi\",\"text\":\"there\",\"extra\":[1]}"));
        assert_eq!(alert.message.as_deref(), Some("there"));
        assert_eq!((alert.icon, alert.tone), (Icon::Bell, Tone::Blue));
        assert_eq!((alert.stay, alert.shake), (Stay::For(Duration::from_millis(4000)), true));
        assert_eq!(shown(post(r#"{"title":"Hi","message":null}"#)).message, None);
    }

    #[test]
    fn get_reads_the_query() {
        let alert = shown(get("title=Build+done&message=100%25%20passing&tone=Orange&icon=Chevron-Down&shake=0"));
        assert_eq!((alert.title.as_str(), alert.message.as_deref()), ("Build done", Some("100% passing")));
        assert_eq!((alert.icon, alert.tone, alert.shake), (Icon::ChevronDown, Tone::Orange, false));
        assert_eq!(shown(get("title=%E2%9C%85+Tests&text=ok")).title, "✅ Tests");
    }

    #[test]
    fn form_and_query_combine() {
        let body = "message=From+the+form&tone=pink";
        let head = "POST /alert?title=Hello&tone=red HTTP/1.1\r\nContent-Type: application/x-www-form-urlencoded";
        let alert = shown(ask(&format!("{head}\r\nContent-Length: {}\r\n\r\n{body}", body.len()), None));
        assert_eq!((alert.title.as_str(), alert.message.as_deref()), ("Hello", Some("From the form")));
        assert_eq!(alert.tone, Tone::Pink);
    }

    #[test]
    fn durations_are_clamped() {
        let stay = |body: &str| shown(post(&format!(r#"{{"title":"T",{body}}}"#))).stay;
        assert_eq!(stay(r#""duration_ms":1"#), Stay::For(Duration::from_millis(1)));
        assert_eq!(stay(r#""duration_ms":"90000""#), Stay::For(Duration::from_secs(90)));
        assert_eq!(stay(r#""duration_ms":1e15"#), Stay::For(Duration::from_secs(86_400)));
        assert_eq!(stay(r#""duration_ms":0"#), Stay::UntilClicked);
        assert_eq!(stay(r#""sticky":true,"duration_ms":5000"#), Stay::UntilClicked);
        assert_eq!(stay(r#""sticky":false"#), Stay::For(Duration::from_millis(4000)));
        assert_eq!(shown(get("title=T&sticky")).stay, Stay::UntilClicked);
        assert_eq!(status(post(r#"{"title":"T","duration_ms":-1}"#)), 400);
        assert_eq!(status(post(r#"{"title":"T","duration_ms":"soon"}"#)), 400);
    }

    #[test]
    fn titles_and_messages_are_cut() {
        let body =
            serde_json::json!({ "title": format!("  Line\none {}", "t".repeat(100)), "message": "m".repeat(300) });
        let alert = shown(post(&body.to_string()));
        assert!(alert.title.starts_with("Line one tt"));
        assert_eq!((alert.title.chars().count(), alert.message.unwrap().chars().count()), (60, 140));
        assert_eq!(shown(post(r#"{"title":"T","message":"   "}"#)).message, None);
    }

    #[test]
    fn bad_alerts_are_rejected_with_400() {
        for body in [
            r#"{"message":"no title"}"#,
            r#"{"title":"   "}"#,
            r#"{"title":{"nested":true}}"#,
            r#"{"title":"T","tone":"magenta"}"#,
            r#"{"title":"T","tone":"primary"}"#,
            r#"{"title":"T","icon":"rocket"}"#,
            r#"{"title":"T","shake":"maybe"}"#,
            r#"{"title":"T""#,
            "[1, 2]",
        ] {
            assert_eq!(status(post(body)), 400, "{body}");
        }
        let unknown = post(r#"{"title":"T","tone":"magenta"}"#).unwrap_err();
        assert_eq!(unknown.error.as_deref(), Some("unknown tone \"magenta\""));
        assert_eq!(status(get("message=only")), 400);
    }

    #[test]
    fn token_in_header_or_query() {
        let token = Some("s3cret");
        assert_eq!(status(with("", token)), 401);
        assert_eq!(status(with("Authorization: Bearer nope", token)), 401);
        assert_eq!(status(with("Authorization: Basic s3cret", token)), 401);
        assert!(with("Authorization: bearer s3cret", token).is_ok());
        assert!(with("Authorization: anything", None).is_ok());
        let query = |query: &str| ask(&format!("GET /alert?title=Hi&{query} HTTP/1.1\r\n\r\n"), token);
        assert!(query("token=s3cret").is_ok());
        assert_eq!(status(query("token=s3cre")), 401);
        assert_eq!(status(query("token=s3cret1")), 401);
    }

    #[test]
    fn unknown_routes_are_refused() {
        assert_eq!(status(ask("GET /nope HTTP/1.1\r\n\r\n", None)), 404);
        assert_eq!(status(ask("GET / HTTP/1.1\r\n\r\n", None)), 404);
        let put = ask("PUT /alert HTTP/1.1\r\n\r\n", None).unwrap_err();
        assert_eq!((put.status, put.allow), (405, Some("GET, POST, DELETE")));
        let get = ask("GET /alert/dismiss HTTP/1.1\r\n\r\n", None).unwrap_err();
        assert_eq!((get.status, get.allow), (405, Some("POST")));
        assert_eq!(status(ask("OPTIONS /alert HTTP/1.1\r\n\r\n", None)), 405);
        assert!(ask("GET /alert/?title=Hi HTTP/1.1\r\n\r\n", None).is_ok());
    }

    #[test]
    fn delete_or_dismiss_clears() {
        assert_eq!(ask("DELETE /alert HTTP/1.1\r\n\r\n", None), Ok(Action::Dismiss));
        assert_eq!(ask("POST /alert/dismiss HTTP/1.1\r\nContent-Length: 0\r\n\r\n", None), Ok(Action::Dismiss));
        assert_eq!(status(ask("DELETE /alert HTTP/1.1\r\n\r\n", Some("s3cret"))), 401);
        assert_eq!(
            ask("DELETE /alert HTTP/1.1\r\nAuthorization: Bearer s3cret\r\n\r\n", Some("s3cret")),
            Ok(Action::Dismiss)
        );
    }

    #[test]
    fn browsers_and_rebinding_refused() {
        assert_eq!(status(with("Origin: https://example.com", None)), 403);
        assert_eq!(status(with("Origin: null", None)), 403);
        assert_eq!(status(with("Sec-Fetch-Site: cross-site", None)), 403);
        assert!(with("Sec-Fetch-Site: none", None).is_ok());
        let host = |host: &str| ask(&format!("GET /alert?title=Hi HTTP/1.1\r\nHost: {host}\r\n\r\n"), None);
        assert_eq!(status(host("attacker.example:47832")), 403);
        assert_eq!(status(host("127.0.0.1.attacker.example")), 403);
        assert!(host("LOCALHOST").is_ok());
        assert!(host("127.0.0.1:47832").is_ok());
    }
}
