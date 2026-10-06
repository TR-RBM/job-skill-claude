use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

struct Bench {
    base: PathBuf,
}

impl Bench {
    fn new(name: &str) -> Bench {
        let base =
            std::env::temp_dir().join(format!("job-hook-claude-{}-{name}", std::process::id()));
        std::fs::create_dir_all(base.join("sessions")).unwrap();
        Bench { base }
    }

    fn stub(&self, body: &str) -> PathBuf {
        let path = self.base.join("job");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{0}/arguments'\ncat > '{0}/question'\n{body}\n",
                self.base.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn answering(&self, answer: &Value) -> PathBuf {
        std::fs::write(self.base.join("answer"), answer.to_string()).unwrap();
        self.stub(&format!("cat '{}/answer'", self.base.display()))
    }

    fn run(&self, program: &Path, payload: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_job-hook-claude"))
            .env("JOB_HOOK_PROGRAM", program)
            .env("JOB_HOOK_SESSIONS_DIR", self.base.join("sessions"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn question(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.base.join("question")).unwrap()).unwrap()
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn payload(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/payloads/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn printed(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{error}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn a_shell_command_is_asked_as_one_neutral_question() {
    let bench = Bench::new("question");
    let job = bench.answering(&json!({"decision": "allow"}));
    let output = bench.run(&job, &payload("bash-build"));
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert_eq!(
        std::fs::read_to_string(bench.base.join("arguments")).unwrap(),
        "hook\n"
    );
    assert_eq!(
        bench.question(),
        json!({
            "command": "cargo test",
            "cwd": "/home/user/project",
            "caller": "3f6c1c1e-8a55-4a3e-9d0e-5d0b0c1f7a10",
            "detached": false
        })
    );
}

#[test]
fn the_name_of_the_session_is_passed_as_the_name_of_the_caller() {
    let bench = Bench::new("name");
    std::fs::write(
        bench.base.join("sessions/100.json"),
        json!({"sessionId": "another", "name": "other"}).to_string(),
    )
    .unwrap();
    std::fs::write(
        bench.base.join("sessions/200.json"),
        json!({"sessionId": "3f6c1c1e-8a55-4a3e-9d0e-5d0b0c1f7a10", "name": "builder"}).to_string(),
    )
    .unwrap();
    let job = bench.answering(&json!({"decision": "allow"}));
    bench.run(&job, &payload("bash-build"));
    assert_eq!(bench.question()["caller_name"], "builder");
}

#[test]
fn a_command_already_in_the_background_is_asked_as_detached() {
    let bench = Bench::new("detached");
    let job = bench.answering(&json!({"decision": "allow"}));
    let output = bench.run(&job, &payload("bash-background"));
    assert!(output.stdout.is_empty());
    assert_eq!(bench.question()["detached"], true);
    assert_eq!(bench.question()["command"], "job wait 17");
}

#[test]
fn a_denial_becomes_a_denied_permission_with_the_reason() {
    let bench = Bench::new("deny");
    let job = bench.answering(&json!({
        "decision": "deny",
        "rule": "empty-variable",
        "reason": "job: denied by rule empty-variable: a variable. Instead: the full path."
    }));
    let output = printed(&bench.run(&job, &payload("bash-remove")));
    assert_eq!(
        output,
        json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": "job: denied by rule empty-variable: a variable. Instead: the full path."
            }
        })
    );
}

#[test]
fn a_rewrite_replaces_the_command_runs_in_the_background_and_keeps_every_other_field() {
    let bench = Bench::new("rewrite");
    let job = bench.answering(&json!({
        "decision": "rewrite",
        "command": "job run --session 'x' --budget none --shell bash --summary -- 'cargo test'",
        "detach": true,
        "reason": "job: routed to the job service because it runs cargo."
    }));
    let output = printed(&bench.run(&job, &payload("bash-build")));
    let specific = &output["hookSpecificOutput"];
    assert_eq!(specific["hookEventName"], "PreToolUse");
    assert!(specific.get("permissionDecision").is_none());
    assert_eq!(
        specific["updatedInput"],
        json!({
            "command": "job run --session 'x' --budget none --shell bash --summary -- 'cargo test'",
            "description": "Run the tests",
            "timeout": 300000,
            "run_in_background": true
        })
    );
    let told = specific["additionalContext"].as_str().unwrap();
    assert!(told.starts_with("job: routed to the job service because it runs cargo."));
    assert!(told.contains("you are notified with its answer when it ends"));
}

#[test]
fn an_allowed_command_with_a_reason_passes_the_reason_on() {
    let bench = Bench::new("note");
    let job = bench.answering(&json!({
        "decision": "allow",
        "reason": "job: the service does not answer."
    }));
    let output = printed(&bench.run(&job, &payload("bash-build")));
    assert_eq!(
        output,
        json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "additionalContext": "job: the service does not answer."
            }
        })
    );
}

#[test]
fn another_tool_and_unreadable_input_are_passed_without_asking() {
    let bench = Bench::new("other");
    let job = bench.answering(&json!({"decision": "deny", "reason": "never asked"}));
    for input in [payload("read-file"), "not json".to_owned(), String::new()] {
        let output = bench.run(&job, &input);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stdout.is_empty());
    }
    assert!(!bench.base.join("question").exists());
}

#[test]
fn a_job_that_gives_no_usable_answer_is_reported_and_the_command_passes() {
    let bench = Bench::new("broken");
    for program in [
        bench.stub("echo 'job: hook: bad input' >&2; exit 125"),
        bench.stub("echo 'not json'"),
        bench.stub("echo '{\"decision\": \"perhaps\"}'"),
        bench.base.join("absent"),
    ] {
        let output = printed(&bench.run(&program, &payload("bash-build")));
        let specific = &output["hookSpecificOutput"];
        assert!(specific.get("permissionDecision").is_none());
        assert!(specific.get("updatedInput").is_none());
        let told = specific["additionalContext"].as_str().unwrap();
        assert!(told.contains("`job hook` gave no usable answer"), "{told}");
        assert!(told.contains("unchecked by the command policy"), "{told}");
    }
}
