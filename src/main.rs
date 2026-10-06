use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use serde_json::{Value, json};

const NOTIFIED: &str = "It runs in the background, and you are notified with its answer when it ends. Do not wait or poll for it.";

fn variable(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn sessions_directory() -> Option<PathBuf> {
    variable("JOB_HOOK_SESSIONS_DIR")
        .or_else(|| variable("CLAUDE_CONFIG_DIR").map(|base| base.join("sessions")))
        .or_else(|| variable("HOME").map(|home| home.join(".claude/sessions")))
}

fn session_name(session_id: &str) -> Option<String> {
    std::fs::read_dir(sessions_directory()?)
        .ok()?
        .flatten()
        .find_map(|entry| {
            let text = std::fs::read_to_string(entry.path()).ok()?;
            let value: Value = serde_json::from_str(&text).ok()?;
            (value.get("sessionId")?.as_str()? == session_id)
                .then(|| value.get("name")?.as_str().map(str::to_owned))
                .flatten()
        })
}

fn question(event: &Value) -> Option<Value> {
    if event.get("tool_name").and_then(Value::as_str) != Some("Bash") {
        return None;
    }
    let input = event.get("tool_input")?;
    let command = input.get("command").and_then(Value::as_str)?;
    let mut question = json!({
        "command": command,
        "detached": input.get("run_in_background").and_then(Value::as_bool) == Some(true),
    });
    if let Some(cwd) = event.get("cwd").and_then(Value::as_str) {
        question["cwd"] = json!(cwd);
    }
    if let Some(session) = event.get("session_id").and_then(Value::as_str) {
        question["caller"] = json!(session);
        if let Some(name) = session_name(session) {
            question["caller_name"] = json!(name);
        }
    }
    Some(question)
}

fn ask(question: &Value) -> Result<Value, String> {
    let program = std::env::var_os("JOB_HOOK_PROGRAM")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "job".into());
    let mut child = Command::new(&program)
        .arg("hook")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{} cannot be started: {error}", program.to_string_lossy()))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(question.to_string().as_bytes());
    }
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "it ended with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let answer: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("its answer is not JSON: {error}"))?;
    match answer.get("decision").and_then(Value::as_str) {
        Some("allow" | "deny" | "rewrite") => Ok(answer),
        _ => Err("its answer has no decision this adapter knows".to_owned()),
    }
}

fn context(text: &str) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "additionalContext": text,
        }
    })
}

fn translate(event: &Value, answer: &Value) -> Option<Value> {
    let reason = answer.get("reason").and_then(Value::as_str);
    match answer["decision"].as_str()? {
        "deny" => Some(json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason.unwrap_or("job: refused"),
            }
        })),
        "rewrite" => {
            let mut updated = event["tool_input"].clone();
            updated["command"] = answer.get("command")?.clone();
            let detach = answer.get("detach").and_then(Value::as_bool) == Some(true);
            if detach {
                updated["run_in_background"] = Value::Bool(true);
            }
            let told = match (reason, detach) {
                (Some(reason), true) => format!("{reason} {NOTIFIED}"),
                (Some(reason), false) => reason.to_owned(),
                (None, true) => NOTIFIED.to_owned(),
                (None, false) => String::new(),
            };
            let mut output = json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "updatedInput": updated,
                }
            });
            if !told.is_empty() {
                output["hookSpecificOutput"]["additionalContext"] = json!(told);
            }
            Some(output)
        }
        _ => reason.map(context),
    }
}

fn main() -> ExitCode {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let Ok(event) = serde_json::from_str::<Value>(&input) else {
        return ExitCode::SUCCESS;
    };
    let Some(question) = question(&event) else {
        return ExitCode::SUCCESS;
    };
    let output = match ask(&question) {
        Ok(answer) => translate(&event, &answer),
        Err(error) => Some(context(&format!(
            "job-hook-claude: `job hook` gave no usable answer ({error}), so this command ran unchanged, outside the job service and unchecked by the command policy."
        ))),
    };
    if let Some(output) = output {
        println!("{output}");
    }
    ExitCode::SUCCESS
}
