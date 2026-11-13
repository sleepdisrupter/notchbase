use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::{Hooks, claude, codex, exe, install_file, installed_file, uninstall_file};
use crate::assistant::tests::TempDir;

const EXE: &str = "C:\\Program Files\\notch base\\notchbase.exe";
const MOVED_EXE: &str = "D:\\Apps\\notchbase.exe";

fn read(file: &PathBuf) -> Value {
    serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap()
}

fn user_settings() -> Value {
    json!({
        "model": "opus",
        "permissions": { "allow": ["Bash(git status)"] },
        "hooks": {
            "Stop": [{ "hooks": [{ "type": "command", "command": "say done" }] }],
            "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "guard.sh" }] }]
        },
        "statusLine": { "type": "command", "command": "line.sh" }
    })
}

fn ours_in(settings: &Value, event: &str) -> usize {
    settings["hooks"][event]
        .as_array()
        .map(|groups| {
            groups
                .iter()
                .flat_map(|group| group["hooks"].as_array().cloned().unwrap_or_default())
                .filter(claude::ours)
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn install_merges_and_keeps_the_rest() {
    let dir = TempDir::new();
    let file = dir.path("settings.json");
    let original = serde_json::to_string_pretty(&user_settings()).unwrap();
    fs::write(&file, &original).unwrap();

    install_file(&file, &claude::spec(EXE)).unwrap();
    let merged = read(&file);
    assert_eq!(merged["model"], "opus");
    assert_eq!(merged["permissions"], user_settings()["permissions"]);
    assert_eq!(merged["statusLine"], user_settings()["statusLine"]);
    assert_eq!(merged["hooks"]["PreToolUse"][0], user_settings()["hooks"]["PreToolUse"][0]);
    assert_eq!(merged["hooks"]["PreToolUse"][1]["hooks"][0]["async"], true);
    assert_eq!(merged["hooks"]["Stop"][0], user_settings()["hooks"]["Stop"][0]);
    let events = ["PermissionRequest", "PreToolUse", "PostToolUse", "PostToolUseFailure", "UserPromptSubmit", "Stop"];
    for event in events.into_iter().chain(["Notification"]) {
        assert_eq!(ours_in(&merged, event), 1, "{event}");
    }
    let blocking = &merged["hooks"]["PermissionRequest"][0]["hooks"][0];
    assert_eq!(blocking["command"], EXE);
    assert_eq!(blocking["args"], json!(["hook", "claude"]));
    assert!(blocking["timeout"].as_u64().unwrap() >= 60 * 60);
    assert_eq!(merged["hooks"]["Stop"][1]["hooks"][0]["async"], true);
    let keys: Vec<&String> = merged.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["model", "permissions", "hooks", "statusLine"]);

    assert_eq!(fs::read_to_string(dir.path("settings.json.notchbase.bak")).unwrap(), original);
    assert!(installed_file(&file, &claude::spec(EXE)));
    assert!(!installed_file(&file, &claude::spec(MOVED_EXE)));
}

#[test]
fn idempotent_and_follows_moved_exe() {
    let dir = TempDir::new();
    let file = dir.path("settings.json");
    fs::write(&file, serde_json::to_string_pretty(&user_settings()).unwrap()).unwrap();
    install_file(&file, &claude::spec(EXE)).unwrap();
    let first = fs::read_to_string(&file).unwrap();
    install_file(&file, &claude::spec(EXE)).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), first);

    install_file(&file, &claude::spec(MOVED_EXE)).unwrap();
    let moved = read(&file);
    assert_eq!(ours_in(&moved, "PermissionRequest"), 1);
    assert_eq!(moved["hooks"]["PermissionRequest"][0]["hooks"][0]["command"], MOVED_EXE);
    assert!(!fs::read_to_string(&file).unwrap().contains("notch base"));
    assert_eq!(read(&dir.path("settings.json.notchbase.bak")), user_settings());
}

#[test]
fn uninstall_removes_only_ours() {
    let dir = TempDir::new();
    let file = dir.path("settings.json");
    fs::write(&file, serde_json::to_string_pretty(&user_settings()).unwrap()).unwrap();
    install_file(&file, &claude::spec(EXE)).unwrap();
    let mut shared_group = read(&file);
    shared_group["hooks"]["Notification"][0]["hooks"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "type": "command", "command": "notify.sh" }));
    fs::write(&file, serde_json::to_string_pretty(&shared_group).unwrap()).unwrap();

    uninstall_file(&file, claude::ours).unwrap();
    let cleaned = read(&file);
    let mut expected = user_settings();
    expected["hooks"]["Notification"] = json!([{ "hooks": [{ "type": "command", "command": "notify.sh" }] }]);
    assert_eq!(cleaned, expected);
}

#[test]
fn uninstall_drops_emptied_containers() {
    let dir = TempDir::new();
    let file = dir.path("settings.json");
    fs::write(&file, "{\n  \"model\": \"opus\"\n}\n").unwrap();
    install_file(&file, &claude::spec(EXE)).unwrap();
    uninstall_file(&file, claude::ours).unwrap();
    assert_eq!(read(&file), json!({ "model": "opus" }));
    assert!(fs::read_to_string(&file).unwrap().ends_with('\n'));
}

#[test]
fn creates_missing_or_empty_settings() {
    let dir = TempDir::new();
    let file = dir.path("settings.json");
    install_file(&file, &claude::spec(EXE)).unwrap();
    assert_eq!(ours_in(&read(&file), "PermissionRequest"), 1);
    assert!(!dir.path("settings.json.notchbase.bak").exists());

    let empty = dir.path("empty.json");
    fs::write(&empty, "\u{feff}  \n").unwrap();
    install_file(&empty, &claude::spec(EXE)).unwrap();
    assert_eq!(ours_in(&read(&empty), "Stop"), 1);
}

#[test]
fn invalid_settings_are_left_alone() {
    let dir = TempDir::new();
    for (name, body) in [
        ("broken.json", "{ \"model\": "),
        ("array.json", "[1, 2]"),
        ("hooks.json", "{ \"hooks\": [] }"),
        ("event.json", "{ \"hooks\": { \"Stop\": {} } }"),
    ] {
        let file = dir.path(name);
        fs::write(&file, body).unwrap();
        assert!(install_file(&file, &claude::spec(EXE)).is_err(), "{name}");
        assert_eq!(fs::read_to_string(&file).unwrap(), body, "{name}");
        assert!(!dir.path(&format!("{name}.notchbase.bak")).exists(), "{name}");
    }
}

#[test]
fn claude_recognizes_only_our_handlers() {
    let ours = |handler: Value| claude::ours(&handler);
    assert!(ours(
        json!({ "command": "/Applications/Notchbase.app/Contents/MacOS/notchbase", "args": ["hook", "claude"] })
    ));
    assert!(ours(json!({ "command": "\"C:\\x\\notchbase.exe\" hook claude" })));
    assert!(!ours(json!({ "command": "/usr/bin/other", "args": ["hook", "claude"] })));
    assert!(!ours(json!({ "command": "notchbase", "args": ["hook", "codex"] })));
    assert!(!ours(json!({ "command": "say done" })));
    assert!(!ours(json!({ "type": "http", "url": "http://x" })));
}

#[test]
fn codex_hooks_file_round_trip() {
    let dir = TempDir::new();
    let file = dir.path("hooks.json");
    let existing = json!({
        "description": "mine",
        "hooks": { "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "python3 policy.py" }] }] }
    });
    fs::write(&file, existing.to_string()).unwrap();
    let spec = codex::spec_for("/Applications/Notch's.app/Contents/MacOS/notchbase", false);
    install_file(&file, &spec).unwrap();
    let merged = read(&file);
    let command = merged["hooks"]["PermissionRequest"][0]["hooks"][0]["command"].as_str().unwrap().to_owned();
    assert_eq!(command, "'/Applications/Notch'\\''s.app/Contents/MacOS/notchbase' hook codex");
    assert_eq!(merged["hooks"]["PreToolUse"][0], existing["hooks"]["PreToolUse"][0]);
    assert_eq!(merged["hooks"]["PreToolUse"][1]["matcher"], "^request_user_input$");
    assert_eq!(merged["hooks"]["PostToolUse"][0]["hooks"][0]["async"], true);
    assert!(installed_file(&file, &spec));

    uninstall_file(&file, codex::ours).unwrap();
    assert_eq!(read(&file), existing);
}

#[test]
fn progress_alone_never_blocks() {
    let dir = TempDir::new();
    let file = dir.path("settings.json");
    fs::write(&file, user_settings().to_string()).unwrap();
    install_file(&file, &claude::spec(EXE)).unwrap();
    install_file(&file, &claude::spec(EXE).reaching(Hooks::Progress)).unwrap();
    let settings = read(&file);
    assert_eq!(ours_in(&settings, "PermissionRequest"), 0);
    for event in ["UserPromptSubmit", "Stop", "PostToolUse", "Notification", "SessionEnd"] {
        assert_eq!(ours_in(&settings, event), 1, "{event}");
    }
    assert!(installed_file(&file, &claude::spec(EXE).reaching(Hooks::Progress)));
    assert!(!installed_file(&file, &claude::spec(EXE).reaching(Hooks::Questions)));

    let codex = codex::spec_for(EXE, false).reaching(Hooks::Progress);
    let events: Vec<&str> = codex.groups.iter().map(|(event, _)| *event).collect();
    assert!(!events.contains(&"PermissionRequest") && !events.contains(&"PreToolUse"));
    assert!(events.contains(&"Stop") && events.contains(&"UserPromptSubmit"));
}

#[test]
fn codex_windows_command_is_cmd_quoted() {
    let spec = codex::spec_for(EXE, true);
    let (_, group) = &spec.groups[0];
    assert_eq!(group["hooks"][0]["command"], format!("\"{EXE}\" hook codex"));
    assert!(codex::ours(&group["hooks"][0]));
    assert!(!codex::ours(&json!({ "command": "python3 notchbase.py hook codex" })));
    assert!(!codex::ours(&json!({ "command": "notchbasehook codex" })));
}

#[test]
fn exe_names() {
    assert!(exe::names_us("C:\\Tools\\NOTCHBASE.EXE"));
    assert!(exe::names_us("'/opt/notchbase'"));
    assert!(!exe::names_us("/opt/notchbase-helper"));
    assert!(!exe::names_us(".exe"));
    assert_eq!(exe::shell_command("C:\\a b\\n.exe", "hook codex", true), "\"C:\\a b\\n.exe\" hook codex");
}
