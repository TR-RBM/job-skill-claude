---
name: job
description: Run builds, test suites, renders, scripts and anything long or heavy through this host's job service, which reserves memory and cores so that parallel sessions cannot freeze the machine, stops a job that overruns with one line saying why, and answers with only the errors and failed tests. Use it for cargo build, test, clippy, tools/check, make, python scripts and renders, and instead of sleep followed by cat to wait for a result. Invoking it turns on a hook that routes such commands through the service by itself for the rest of the session.
hooks:
  PreToolUse:
    - matcher: "Bash"
      hooks:
        - type: command
          command: "job-hook-claude"
---

# job

From now on heavy Bash commands in this session go through the job service and run in the background. Your tool call still shows the command as you wrote it, because the hook rewrites it after you send it, and it returns at once with a background task id. When the Job ends you are notified; the answer's first line begins `[job] job N`, which is how you and the person at the terminal see that it went through the service. Do not wait or poll for it. Cheap commands (ls, cat, grep, git, sv and the like, and for-loops over them) run in the foreground as before.

## What comes back

One header line: `[job] job N:`, exit status, run time, peak memory, how long it was queued and for what. A line saying why, if the Job was stopped. Then the output: whole when it is at most 35 lines; otherwise only what matters: compile errors as `file:line:col: error[CODE]: message`, failed tests as `FAILED name at file:line:col: message`, a totals line, the last lines, and how to ask the log for the rest. Read it; do not rerun the command to see more.

## Three rules when you type job yourself

- Arguments after `--` are executed literally, also when there is only one. `job run -- 'make && make check'` looks for a program of that name. For shell syntax write `job run --shell bash -- 'make && make check'` or `job run -- sh -c '...'`.
- An interrupted `job run` does not stop waiting: the first SIGINT or SIGTERM is forwarded to the Job. A harness that interrupts `job run` only to get its prompt back must pass `--on-interrupt detach`, which returns 75 and leaves the Job running; `--on-interrupt cancel` cancels the Job. `job submit` leaves no client to interrupt.
- A command the hook routes carries this session's ID as its label, which `job list` shows. A `job run` or `job submit` you type yourself carries the label only if you pass it: `--session "$CLAUDE_CODE_SESSION_ID"`.

Plain `job run` passes the command's output through. `--summary` asks for the short answer described above; the hook adds it for you.

## cd and export

A routed command runs in the background, and a background call cannot move your shell. So `cd X && cargo test` is refused with the two calls to make instead: `cd X` on its own, then `cargo test`. A `cd` in the middle of a routed line moves only the Job, never your shell: write later commands with absolute paths, or `cd` on its own first. `job run --dir DIR` names the working directory of one Job.

## Declaring what a Job needs

    job run --mem 8G --cores 4 --summary -- cargo build --release

`--cores` and `--mem` reserve; `--time 30m` stops a Job that runs longer. Like every routed command, it runs in the background and you are notified with its answer; a `job run` or `job wait` you type never holds the session.

## Wait, never sleep

- `job submit --shell bash -- '<command>'` prints the ID at once; `job wait --summary ID` then gives the summary. Plain `job wait ID` is quiet and returns the Job's exit status.
- `job show ID` says where a Job stands; `job list` shows what is held, queued and running, and `job list --all` the completed ones too.
- Never `sleep N; cat …` to wait for a result.

## Ask the log instead of reading it

    job log ID errors          lines with error words, numbered
    job log ID grep TEXT       lines containing TEXT
    job log ID lines 120..160  a range
    job log ID tail 50         the end

`job log ID full` prints everything; use it last. `job logs ID --stream stderr` prints one stream.

## Queues

A new Queue has no ceiling: its Jobs run side by side. Say `--max-running 1` where they must run one after another.

    job queue create serial --max-running 1
    job queue create fetch --max-running 2 --net none
    job run -q serial --shell bash --summary -- '<command>'
    job queue show serial                  its settings and how many Jobs wait and run
    job queue pause serial                 hold new starts; job queue resume serial
    job queue cancel --recursive serial    cancel its Jobs, waiting and running
    job queue remove serial                remove it and the records of its completed Jobs
    job queue set fetch --bandwidth 10Mbit --job-bandwidth 5Mbit
    job run --net none --shell bash --summary -- '<command>'       no network for one Job
    job run --net socks5://HOST:PORT --shell bash --summary -- '…'  only through a proxy
    job run --on USER@HOST --shell bash --summary -- '…'           on another host's job service
    job host                               what this host offers and whether limits are enforced

A Job's own option wins over its Queue's default.

## When a Job was stopped

The line names the resource and the value: memory, processes, bytes written, time. If the Job really needs more, declare it (`--mem`, `--pids`, `--write-budget`, `--time`) and run again.

## Also

- `job run --confine --shell bash --summary -- '<command>'` lets the command write only in its own tree, its repository's .git, /tmp and the cargo caches; add `--allow-write PATH` where it must write elsewhere.
- `job cancel ID` cancels a Job; `job retry ID` runs a completed one again as a new attempt.
- A line saying `limits watched, not enforced` means the service has no delegated cgroup; Jobs are still queued and watched.
- A command the command policy forbids is refused with the rule and what to do instead; do that instead.
- If the service is down, the hook lets commands run directly and tells you so.

Every command and option: `job help`, `job COMMAND --help`, and `docs/reference/syntax.md` in job's source tree. How the hook is set up: the README of the repository this skill comes from.
