use std::io::Cursor;

use crate::assistant::prompt::{PromptKind, Provider};
use crate::assistant::token;
use crate::assistant::wire::{self, Answer, Envelope, Event, MAX_LINE, Signal};

#[test]
fn envelopes_round_trip_as_single_lines() {
    let envelope = Envelope {
        token: "abc".into(),
        event: Event {
            provider: Provider::Codex,
            session: "thr".into(),
            agent: None,
            agent_type: None,
            cwd: "/repo".into(),
            started_ms: u64::MAX,
            window: Some(u32::MAX),
            signal: Signal::Ask {
                key: u64::MAX - 1,
                prompt: PromptKind::Permission { tool: "Bash".into(), detail: "echo \"hi\"\nls".into(), always: false },
            },
        },
    };
    let line = wire::encode(&envelope).unwrap();
    assert_eq!(line.iter().filter(|byte| **byte == b'\n').count(), 1);
    let decoded: Envelope = wire::decode(&mut Cursor::new(line), MAX_LINE).unwrap();
    assert_eq!(decoded.event, envelope.event);

    let answers = [Answer::Allow, Answer::Always, Answer::Deny, Answer::Answers { picks: vec![vec![3], vec![0, 2]] }];
    for answer in answers.into_iter().chain([Answer::Release]) {
        let line = wire::encode(&answer).unwrap();
        assert_eq!(wire::decode::<Answer>(&mut Cursor::new(line), wire::MAX_ANSWER), Some(answer));
    }
}

#[test]
fn older_hooks_send_no_window() {
    let line = br#"{"token":"t","event":{"provider":"claude","session":"s","agent":null,"agent_type":null,"cwd":"/x","started_ms":1,"signal":{"type":"turn_started"}}}
"#;
    let decoded: Envelope = wire::decode(&mut Cursor::new(line.to_vec()), MAX_LINE).unwrap();
    assert_eq!(decoded.event.window, None);
    let line = wire::encode(&decoded).unwrap();
    assert!(!String::from_utf8(line).unwrap().contains("window"), "no window, no field");
}

#[test]
fn decode_rejects_bad_lines() {
    let mut huge = vec![b' '; MAX_LINE + 10];
    huge.push(b'\n');
    assert!(wire::decode::<Answer>(&mut Cursor::new(huge), MAX_LINE).is_none());
    assert!(wire::decode::<Answer>(&mut Cursor::new(b"{\"answer\":\"allow\"}".to_vec()), MAX_LINE).is_none());
    assert!(wire::decode::<Answer>(&mut Cursor::new(b"GET / HTTP/1.1\r\n".to_vec()), MAX_LINE).is_none());
    assert!(wire::decode::<Answer>(&mut Cursor::new(b"{\"answer\":\"maybe\"}\n".to_vec()), MAX_LINE).is_none());
    assert!(wire::decode::<Answer>(&mut Cursor::new(Vec::new()), MAX_LINE).is_none());
}

#[test]
fn tokens_are_random_and_exact() {
    let first = token::fresh().unwrap();
    let second = token::fresh().unwrap();
    assert_eq!(first.len(), 64);
    assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_ne!(first, second);
    assert!(token::same(&first, &first.clone()));
    assert!(!token::same(&first, &second));
    assert!(!token::same(&first, &first[..63]));
    assert!(!token::same("", &first));
}
