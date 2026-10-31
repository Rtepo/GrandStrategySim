# Daemon v2 Bootstrap — READ THIS FIRST

You are a supervised worker daemon (agent-N) managed by
`.devin/scripts/daemon_supervisor.sh`. You may have been restarted after a
crash, OOM kill, or model rate-limit cooldown. **Do not assume any prior
session memory survives** — everything you need is on disk:

1. **Your task spec**: `.devin/tasks/specs/<TASK-ID>.md` (path in the
   supervisor context file `context.md` beside this file).
2. **The ledger**: `.devin/tasks/backlog.json` — find your task by `id`;
   `required_rust_files`, `required_ui_files`, and `ui_ast_grep_checks` are
   the completion contract.
3. **Your prior work**: `git -C <worktree> status` and
   `git -C <worktree> diff` show uncommitted progress; check
   `.devin/wip/<TASK-ID>.patch` for an unsaved diff snapshot if the
   worktree is clean.
4. **Guardrails**: `AGENTS.md` — branch isolation, banned git commands,
   surgical edits only. Never work on `main`.

## Completion protocol
- Work until the spec's acceptance criteria hold.
- Run `bash .devin/scripts/task_ctl.sh verify <TASK-ID>` — a task is only
  DONE when the full-stack gate passes (backend diffs **and** React UI
  `.tsx` modifications **and** UI grep checks). Backend-only work is
  REJECTED by the gate.
- Post failures blockers via `bash .devin/scripts/task_ctl.sh block <TASK-ID> "reason"`.
