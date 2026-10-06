# job skill for Claude Code

This repository connects Claude Code to `job`, a Linux execution and scheduling service. It holds three things:

- a skill, `skill/job/SKILL.md`, that explains `job run`, `job wait` and `job log` to the agent;
- an adapter, the program `job-hook-claude`, that Claude Code calls as a PreToolUse hook before it runs a shell command. The adapter asks `job hook` what to do with the command and translates the answer back;
- a default command policy, `policy/policy.json`, that you may install as your own.

job itself knows nothing about Claude Code. Everything that is specific to it is here.

## What you need

- Linux, and stable Rust 1.98 or later to build the adapter.
- The `job` program on `PATH`, with a running service, in a version whose `job hook` answers a JSON question. Check it:

  ```sh
  echo '{"command": "ls"}' | job hook
  ```

  This must print `{"decision":"allow"}`. A `job` that prints nothing, or a usage text, is too old for this adapter.

## Install

```sh
make install
```

This builds the adapter and installs, for the current user:

| File | Where | Change it with |
|---|---|---|
| `job-hook-claude` | `~/.local/bin` | `PREFIX` |
| `SKILL.md` | `~/.claude/skills/job` | `CLAUDE_HOME` |

`~/.local/bin` must be on the `PATH` Claude Code runs with. The skill's header registers `job-hook-claude` as a PreToolUse hook for `Bash`, for the rest of a session that invokes the skill; a session that never invokes it is not affected. To have the hook in every session, put it into `~/.claude/settings.json` instead:

```json
{
  "hooks": {
    "PreToolUse": [
      {"matcher": "Bash", "hooks": [{"type": "command", "command": "job-hook-claude"}]}
    ]
  }
}
```

`make uninstall` removes the two files. A session that already loaded the hook keeps it until it ends.

## The command policy

job refuses nothing by itself. Which commands the hook refuses is written in your command policy, the file `policy.json` in job's configuration directory, by default `~/.config/job/policy.json`. It is yours: job reads it at every call of the hook, so an edit takes effect at once, and `job policy --show` validates it and lists the rules in force. The format is described in job's documentation, on its page about automation and in `job.conf(5)`.

`make install` tells you whether you have such a file. If you have none,

```sh
make install-policy
```

installs the default from `policy/policy.json`. It never replaces a file that exists. The default refuses a recursive `rm` of the root, a home or a system directory, destructive commands whose path holds an unexpanded variable, `find` from the root directory, `git` with `--no-verify`, and a download piped into a shell. It is written for no particular host; remove what you do not want and add what you do.

## What the adapter does

Claude Code hands `job-hook-claude` one JSON object on standard input. For any tool but `Bash`, and for input it cannot read, the adapter prints nothing, which leaves the call as it is. For a shell command it runs `job hook` with this question:

| Field | From |
|---|---|
| `command` | `tool_input.command` |
| `cwd` | `cwd` |
| `caller` | `session_id` |
| `caller_name` | the `name` of the session whose `sessionId` is `session_id`, read from the files in `~/.claude/sessions`; left out when there is none |
| `detached` | `tool_input.run_in_background` |

and turns the answer into Claude Code's hook output:

| Answer of `job hook` | Output |
|---|---|
| `allow` without a reason | nothing |
| `allow` with a reason | the reason as `additionalContext` |
| `deny` | `permissionDecision` `deny` with the reason |
| `rewrite` | `updatedInput`: the tool input with `command` replaced and, when the answer says `detach`, `run_in_background` set; the reason as `additionalContext` |

If `job hook` cannot be started, ends with an error or prints something else, the command runs unchanged and the agent is told that it ran outside the service and unchecked by the command policy. The adapter is not a security boundary: whoever controls the agent's configuration can remove the hook.

Three environment variables change where the adapter looks, mainly for tests: `JOB_HOOK_PROGRAM` (default `job`), `JOB_HOOK_SESSIONS_DIR`, and `CLAUDE_CONFIG_DIR`, whose `sessions` directory is used when the second is not set.

## Test

```sh
make check
```

runs `cargo fmt --check`, `cargo clippy` with warnings as errors, and `cargo test`. The tests start the built adapter with a stand-in for `job` and the hook payloads in `tests/payloads/`. Those payloads are written by hand after the documented shape of a PreToolUse call; none was captured from a running session.

## Licence

Apache-2.0, see `LICENSE`.
