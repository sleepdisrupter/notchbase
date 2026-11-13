use serde_json::{Value, json};

use super::render::Render;
use super::{Plan, claude, codex, fingerprint};
use crate::assistant::prompt::{Offer, PromptKind, Provider, Question};
use crate::assistant::wire::{Answer, Signal};

const AT: u64 = 1_700_000_000_000;

fn claude_plan(payload: &Value) -> Plan {
    claude::plan(payload, AT).expect("a plan")
}

fn output(render: &Render, answer: Answer) -> Value {
    serde_json::from_str(&render.output(&answer).expect("some output")).unwrap()
}

fn picks(picks: &[&[usize]]) -> Answer {
    Answer::Answers { picks: picks.iter().map(|picked| picked.to_vec()).collect() }
}

fn bash_request() -> Value {
    json!({
        "session_id": "s1",
        "cwd": "C:\\work\\notchbase",
        "hook_event_name": "PermissionRequest",
        "permission_mode": "default",
        "tool_name": "Bash",
        "tool_input": { "command": "rm -rf node_modules", "description": "Remove node_modules" },
        "permission_suggestions": [],
    })
}

fn question_request(options: &[&str], multi: bool) -> Value {
    let options: Vec<Value> = options.iter().map(|label| json!({ "label": label, "description": "" })).collect();
    json!({
        "session_id": "s1",
        "cwd": "/Users/me/app",
        "hook_event_name": "PermissionRequest",
        "tool_name": "AskUserQuestion",
        "tool_input": {
            "questions": [{ "question": "Ship it?", "header": "Ship", "options": options, "multiSelect": multi }]
        },
    })
}

#[test]
fn claude_permission_becomes_a_held_ask() {
    let plan = claude_plan(&bash_request());
    assert_eq!(plan.event.provider, Provider::Claude);
    assert_eq!(plan.event.session, "s1");
    assert_eq!(plan.event.cwd, "C:\\work\\notchbase");
    assert_eq!(plan.event.started_ms, AT);
    assert!(plan.event.agent.is_none());
    let Signal::Ask { prompt, .. } = &plan.event.signal else { panic!("expected an ask") };
    let expected = PromptKind::Permission { tool: "Bash".into(), detail: "rm -rf node_modules".into(), always: false };
    assert_eq!(prompt, &expected);
    assert!(plan.event.signal.holds());
}

#[test]
fn suggestions_become_always_allow() {
    let suggestion = json!({
        "type": "addRules",
        "rules": [{ "toolName": "Bash", "ruleContent": "rm -rf node_modules" }],
        "behavior": "allow",
        "destination": "localSettings",
    });
    let mut request = bash_request();
    request["permission_suggestions"] = json!([suggestion.clone()]);
    let plan = claude_plan(&request);
    let Signal::Ask { prompt: PromptKind::Permission { always, .. }, .. } = &plan.event.signal else { panic!() };
    assert!(always);
    let decision = &output(&plan.render, Answer::Always)["hookSpecificOutput"]["decision"];
    assert_eq!(decision, &json!({ "behavior": "allow", "updatedPermissions": [suggestion] }));

    let plain = claude_plan(&bash_request());
    assert_eq!(output(&plain.render, Answer::Always)["hookSpecificOutput"]["decision"], json!({ "behavior": "allow" }));
}

#[test]
fn permission_decisions_match_contract() {
    let plan = claude_plan(&bash_request());
    assert_eq!(
        output(&plan.render, Answer::Allow),
        json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": { "behavior": "allow" } } })
    );
    let denied = output(&plan.render, Answer::Deny);
    assert_eq!(denied["hookSpecificOutput"]["decision"]["behavior"], "deny");
    assert_eq!(denied["hookSpecificOutput"]["decision"]["interrupt"], true);
    assert!(denied["hookSpecificOutput"]["decision"]["message"].is_string());
    assert!(plan.render.output(&Answer::Release).is_none());
}

#[test]
fn question_answers_via_updated_input() {
    let plan = claude_plan(&question_request(&["Yes", "No"], false));
    let Signal::Ask { prompt, .. } = &plan.event.signal else { panic!("expected an ask") };
    let offer = |label: &str| Offer { label: label.into(), detail: String::new() };
    let question = Question {
        header: "Ship".into(),
        text: "Ship it?".into(),
        offers: vec![offer("Yes"), offer("No")],
        multi: false,
    };
    assert_eq!(prompt, &PromptKind::Questions { questions: vec![question], answerable: true });
    let picked = output(&plan.render, picks(&[&[1]]));
    let decision = &picked["hookSpecificOutput"]["decision"];
    assert_eq!(decision["behavior"], "allow");
    assert_eq!(decision["updatedInput"]["answers"], json!({ "Ship it?": "No" }));
    assert_eq!(decision["updatedInput"]["questions"][0]["question"], "Ship it?");
    assert!(plan.render.output(&picks(&[&[5]])).is_none());
}

#[test]
fn any_options_and_multi_select_work() {
    for (options, multi) in [(&["A", "B", "C", "D"][..], false), (&["A", "B", "C"][..], true), (&["Only"][..], false)] {
        let plan = claude_plan(&question_request(options, multi));
        let Signal::Ask { prompt: PromptKind::Questions { answerable, .. }, .. } = plan.event.signal else {
            panic!("expected a question");
        };
        assert!(answerable, "{options:?} multi={multi}");
    }
    let plan = claude_plan(&question_request(&["Logs", "Cache", "Metrics"], true));
    let chosen = output(&plan.render, picks(&[&[0, 2]]));
    assert_eq!(
        chosen["hookSpecificOutput"]["decision"]["updatedInput"]["answers"],
        json!({ "Ship it?": "Logs, Metrics" })
    );
}

#[test]
fn several_questions_answer_together() {
    let options = |labels: [&str; 2]| labels.map(|label| json!({ "label": label, "description": label }));
    let payload = json!({
        "session_id": "s1", "cwd": "/x", "hook_event_name": "PermissionRequest", "tool_name": "AskUserQuestion",
        "tool_input": { "questions": [
            { "question": "Framework?", "header": "Stack", "options": options(["React", "Svelte"]), "multiSelect": false },
            { "question": "Tests?", "header": "Tests", "options": options(["Vitest", "Jest"]), "multiSelect": false },
        ] },
    });
    let plan = claude_plan(&payload);
    let Signal::Ask { prompt: PromptKind::Questions { questions, answerable: true }, .. } = &plan.event.signal else {
        panic!("expected answerable questions");
    };
    assert_eq!((questions.len(), questions[1].offers[0].detail.as_str()), (2, "Vitest"));
    let answers = output(&plan.render, picks(&[&[1], &[0]]));
    assert_eq!(
        answers["hookSpecificOutput"]["decision"]["updatedInput"]["answers"],
        json!({ "Framework?": "Svelte", "Tests?": "Vitest" })
    );
    assert!(plan.render.output(&picks(&[&[1]])).is_none(), "every question needs an answer");
}

#[test]
fn plan_review_is_attention() {
    let payload = json!({
        "session_id": "s1", "cwd": "/x", "hook_event_name": "PermissionRequest",
        "tool_name": "ExitPlanMode", "tool_input": { "plan": "## Do it" },
    });
    let Signal::Ask { prompt, .. } = claude_plan(&payload).event.signal else { panic!("expected an ask") };
    assert!(matches!(prompt, PromptKind::Attention { .. }));
}

#[test]
fn post_tool_use_matches_fingerprint() {
    let Signal::Ask { key: asked, .. } = claude_plan(&bash_request()).event.signal else { panic!() };
    let mut done = bash_request();
    done["hook_event_name"] = json!("PostToolUse");
    done["tool_response"] = json!({ "stdout": "", "stderr": "" });
    assert_eq!(claude_plan(&done).event.signal, Signal::ToolDone { key: asked });

    let Signal::Ask { key: asked, .. } = claude_plan(&question_request(&["Yes", "No"], false)).event.signal else {
        panic!()
    };
    let mut answered = question_request(&["Yes", "No"], false);
    answered["hook_event_name"] = json!("PostToolUse");
    answered["tool_input"]["answers"] = json!({ "Ship it?": "Yes" });
    assert_eq!(claude_plan(&answered).event.signal, Signal::ToolDone { key: asked });
}

#[test]
fn fingerprint_ignores_order_not_tool() {
    let one = json!({ "command": { "a": 1, "b": [1, 2] } });
    let two = json!({ "command": { "b": [1, 2], "a": 1 } });
    assert_eq!(fingerprint::key("Bash", &one), fingerprint::key("Bash", &two));
    assert_ne!(fingerprint::key("Bash", &one), fingerprint::key("PowerShell", &one));
}

#[test]
fn claude_lifecycle_events() {
    let base = |name: &str| json!({ "session_id": "s1", "cwd": "/x", "hook_event_name": name });
    let mut stop = base("Stop");
    stop["last_assistant_message"] = json!("All   done.\n\nTests pass.");
    assert_eq!(claude_plan(&stop).event.signal, Signal::TurnEnded { message: "All done.\nTests pass.".into() });
    assert_eq!(claude_plan(&base("UserPromptSubmit")).event.signal, Signal::TurnStarted);
    assert_eq!(claude_plan(&base("SessionEnd")).event.signal, Signal::SessionEnded);
    assert!(claude::plan(&base("PreCompact"), AT).is_none());

    let mut notice = base("Notification");
    notice["notification_type"] = json!("permission_prompt");
    notice["message"] = json!("Claude needs your permission to use Bash");
    assert_eq!(
        claude_plan(&notice).event.signal,
        Signal::Attention { message: "Claude needs your permission to use Bash".into() }
    );
    notice["notification_type"] = json!("idle_prompt");
    assert_eq!(claude_plan(&notice).event.signal, Signal::Idle);
    notice["notification_type"] = json!("auth_success");
    assert!(claude::plan(&notice, AT).is_none());
}

#[test]
fn finished_messages_drop_markdown() {
    let stop = json!({
        "session_id": "s", "cwd": "/x", "hook_event_name": "Stop",
        "last_assistant_message": "## Summary\n\n- Fixed **the** `parser`\n- Added snake_case tests",
    });
    assert_eq!(
        claude_plan(&stop).event.signal,
        Signal::TurnEnded { message: "Summary\nFixed the parser\nAdded snake_case tests".into() }
    );
}

#[test]
fn paragraphs_keep_one_line_break() {
    let stop = json!({
        "session_id": "s", "cwd": "/x", "hook_event_name": "Stop",
        "last_assistant_message": "  Fixed   it.  \r\n\r\n   \r\n```\nlet x = 1;\n```\n\n\tTests\tpass. ",
    });
    assert_eq!(
        claude_plan(&stop).event.signal,
        Signal::TurnEnded { message: "Fixed it.\nlet x = 1;\nTests pass.".into() }
    );
    let long = json!({
        "session_id": "s", "cwd": "/x", "hook_event_name": "Stop",
        "last_assistant_message": format!("First\n\n{}", "word ".repeat(200)),
    });
    let Signal::TurnEnded { message } = claude_plan(&long).event.signal else { panic!("a finished turn") };
    assert_eq!(message.chars().count(), 400);
    assert!(message.starts_with("First\nword word") && message.ends_with('\u{2026}'));
}

#[test]
fn subagent_fields_are_forwarded() {
    let mut request = bash_request();
    request["agent_id"] = json!("a1");
    request["agent_type"] = json!("Explore");
    let plan = claude_plan(&request);
    assert_eq!(plan.event.agent.as_deref(), Some("a1"));
    assert_eq!(plan.event.agent_type.as_deref(), Some("Explore"));
}

#[test]
fn details_clip_and_mcp_names_shorten() {
    let payload = json!({
        "session_id": "s", "cwd": "/x", "hook_event_name": "PermissionRequest",
        "tool_name": "mcp__github__create_issue", "tool_input": { "title": "t".repeat(2000) },
    });
    let Signal::Ask { prompt: PromptKind::Permission { tool, detail, .. }, .. } = claude_plan(&payload).event.signal
    else {
        panic!()
    };
    assert_eq!(tool, "github/create_issue");
    assert!(detail.chars().count() <= 600);
    assert!(detail.ends_with('\u{2026}'));
}

#[test]
fn codex_permission_and_outputs() {
    let payload = json!({
        "session_id": "thr_1", "turn_id": "t1", "cwd": "/repo", "hook_event_name": "PermissionRequest",
        "model": "gpt-5", "permission_mode": "default", "tool_name": "apply_patch",
        "tool_input": { "command": "*** Begin Patch\n*** Update File: src/main.rs\n@@\n*** Add File: a.txt\n*** End Patch" },
    });
    let plan = codex::plan(&payload, AT).expect("a plan");
    let Signal::Ask { prompt, .. } = &plan.event.signal else { panic!() };
    let expected =
        PromptKind::Permission { tool: "apply_patch".into(), detail: "src/main.rs, a.txt".into(), always: false };
    assert_eq!(prompt, &expected);
    let denied = output(&plan.render, Answer::Deny);
    assert_eq!(denied["hookSpecificOutput"]["decision"]["behavior"], "deny");
    assert!(denied["hookSpecificOutput"]["decision"].get("interrupt").is_none());
    assert_eq!(output(&plan.render, Answer::Allow)["hookSpecificOutput"]["decision"]["behavior"], "allow");
    assert!(plan.render.output(&picks(&[&[0]])).is_none());
}

#[test]
fn codex_question_note_and_lifecycle() {
    let ask = json!({
        "session_id": "thr_1", "cwd": "/repo", "hook_event_name": "PreToolUse", "tool_name": "request_user_input",
        "tool_input": { "questions": [{ "id": "q", "question": "Which DB?", "options": [{ "label": "Postgres" }, { "label": "SQLite" }] }] },
    });
    let Signal::Note { prompt, .. } = codex::plan(&ask, AT).unwrap().event.signal else { panic!() };
    assert!(matches!(prompt, PromptKind::Questions { answerable: false, .. }));
    let mut other = ask.clone();
    other["tool_name"] = json!("Bash");
    assert!(codex::plan(&other, AT).is_none());

    let stop =
        json!({ "session_id": "thr_1", "cwd": "/repo", "hook_event_name": "Stop", "last_assistant_message": null });
    assert_eq!(codex::plan(&stop, AT).unwrap().event.signal, Signal::TurnEnded { message: String::new() });
}

#[test]
fn codex_notify_argument() {
    let argument = json!({
        "type": "agent-turn-complete", "thread-id": "thr_9", "turn-id": "1", "cwd": "/repo",
        "input-messages": ["hi"], "last-assistant-message": "Done!",
    })
    .to_string();
    let plan = codex::notify(&argument, AT).expect("a plan");
    assert_eq!(plan.event.session, "thr_9");
    assert_eq!(plan.event.signal, Signal::TurnEnded { message: "Done!".into() });
    assert!(codex::notify(&json!({ "type": "other" }).to_string(), AT).is_none());
    assert!(codex::notify("not json", AT).is_none());
}

#[test]
fn garbage_payloads_are_ignored() {
    assert!(claude::plan(&json!([]), AT).is_none());
    assert!(claude::plan(&json!({ "hook_event_name": 7 }), AT).is_none());
    assert!(claude::plan(&json!({ "hook_event_name": "PermissionRequest" }), AT).is_none());
    assert!(codex::plan(&json!("x"), AT).is_none());
    assert!(super::respond(&["claude".into()], &b"{not json"[..], None).is_none());
    assert!(super::respond(&["gemini".into()], &b"{}"[..], None).is_none());
    assert!(super::respond(&[], &b"{}"[..], None).is_none());
}
