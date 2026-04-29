# Solo Developer First Run

Catalyst Continuum should first be useful to an individual developer working on one repository.
Team controls, enterprise identity, and multi-project governance come later.

The first product promise is simple:

> Give Catalyst Continuum a structured brief, let agents produce a bounded delivery run, then review
> one evidence package before opening or accepting a pull request.

## What Value Should A Solo Developer Get?

As a solo developer, you can already use Codex, OpenHands, Cursor, or another agent directly. The
orchestrator is only worth using when it adds structure those tools do not consistently own by
themselves.

Catalyst Continuum focuses on these gaps:

- A repeatable brief-to-backlog-to-task-to-artifact flow instead of one-off chat sessions.
- A visible run ledger that shows what happened, which agent owned each task, and what evidence was
  produced.
- A developer handoff view that summarizes what to review before trusting the generated change.
- A portable review prompt that can be pasted into Cursor, Codex, or OpenHands so a second agent can
  review the run from the same evidence package instead of starting from an empty chat.
- Quality, policy, and repository-publication gates that sit outside any single coding agent.
- Repository-target guardrails so draft PR publication goes only to an approved repository, base
  branch, and generated branch prefix.
- One local place to inspect agent logs, artifacts, LiteLLM state, and Grafana observability without
  chasing files across terminals.

The product is not trying to replace the coding agent. It is the local control plane around the
agent.

For interactive work, keep the agent in its native mode. If Codex gives the best local UI for the
task, use Codex directly; if OpenHands is better for a longer autonomous run, use OpenHands. The
orchestrator should give both of them the same task packet, repository context, allowed tools,
quality expectations, and review evidence instead of forcing either agent through a weaker wrapper.

The first GitHub issue broker slice is now available: Catalyst Continuum can import one issue or a
bounded issue list, then package that work with an explicit PR strategy. Use the default
`per-issue` strategy when each task should become its own PR, or use `batch` when a related issue
set should stay in one branch and one PR.

## Try The Demo Flow

If you are not sure where to start, run the alpha guide first:

```bash
make start
```

It prints the shortest solo-developer path: local demo, GitHub issue preview, native-agent prompt
handoff, evidence run, review, and release-readiness checks.

Run:

```bash
make solo-demo
```

The command starts disposable local state, seeds one completed MVP delivery run, and starts the UI.
It prints a URL like:

```text
http://127.0.0.1:<port>/ui
```

Open that URL and follow this order:

1. Open `Mission Control`.
2. Select the `Developer` tab.
3. Copy the agent review prompt into Cursor, Codex, or OpenHands when you want a second-pass review
   grounded in the run evidence.
4. Review the delivery summary, checklist, quality evidence, artifacts, and agent handoff.
5. Open `Agents` to inspect agent lanes and task logs.
6. Open `Flow` to see where the run is in the brief-to-PR lifecycle.

The review prompt is the practical difference from running a coding agent directly: Catalyst
Continuum gives the reviewer the run ID, task state, artifacts, quality posture, repository guard,
agent lanes, and exact checklist in one repeatable package.

If the UI opens before any run exists, the `Recent orchestrator runs` panel stays actionable: it
links back to brief intake and can copy `make solo-demo` so the first demo run is one terminal
command away.
The top first-run playbook also exposes copy buttons for `make solo-demo` and the GitHub issue
preview command, so a new user can move from explanation to a runnable terminal step without
searching the docs.

When work starts from GitHub issues, also open `GitHub Issue Workbench` in the same UI. It shows
recent issue workflows, lets you switch between packages without a page refresh, and explains
whether the selected package is still at planning, agent execution, PR handoff, or issue sync. The
same panel exposes Codex, Cursor, and OpenHands handoff commands for the selected package, plus the
prompt preview, prompt size/status metadata, direct prompt-text copy, and the Codex app-server
command when a Codex prompt is attached. It also copies the strict preflight command and the prompt
path directly for native launchers or shell aliases that only need the file path. The browser does
not apply labels, comments, or state changes; you review the generated plan and run the copied
terminal command when you are ready.

For real daily work, start from a GitHub issue or use a task recipe instead of hand-writing a full
brief:

```bash
make github-issue-next \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  GITHUB_ISSUE_ARGS="--label bug --limit 5"
```

Use this when you want Catalyst to choose one next issue from a bounded batch and keep the rest as a
ranked queue in `.continuum/github-issue-batches/`.

```bash
make github-issue-session \
  GITHUB_ISSUE=123 \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout
```

Use this when the source of work is already a GitHub issue. It creates a normal
`.continuum/dev-sessions/` package with `brief.json`, agent prompts, `issue.md`, and
`issue-context.json`. The issue text is untrusted context, so it cannot override repository policy,
validation, sandboxing, or secrets handling.

For a related issue set that should be reviewed in one PR, import the list as a batch session:

```bash
make github-issue-session \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  GITHUB_ISSUE_PR_STRATEGY=batch \
  GITHUB_ISSUE_ARGS="--list --label bug --limit 5"
```

This creates one `.continuum/dev-sessions/` package with `issue-batch.md`,
`issue-batch-context.json`, and agent prompts that preserve the one-branch, one-PR intent.

When you want Catalyst to execute the next work package immediately, use the combined issue run:

```bash
make github-issue-run \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  GITHUB_ISSUE_ARGS="--label bug --limit 5"
```

This chooses the next issue for `per-issue`, runs the local control-plane flow, exports a local PR
candidate, and creates a dry-run GitHub issue update plan. Set `GITHUB_ISSUE_PR_STRATEGY=batch` when
the imported issues should stay in one branch and one PR.

After the workflow finishes, open `workflow-report.md` from the printed workflow output directory
first. It is the readable handoff: outcome, selected session, copy-ready Codex/Cursor/OpenHands
prompt commands, Codex app-server commands with the selected `REPO_PATH`, run evidence, PR export
branch and commit, draft PR URL when available, issue-sync status, and the next review action.

If you need to find it later, run:

```bash
make github-issue-plan-review
make github-issue-preflight
make github-issue-preflight-strict
make github-issue-review
make github-issue-next-command
make github-issue-sync-command
make github-issue-agent-prompt AGENT=codex
make github-issue-agent-prompt-command AGENT=cursor
make github-issue-agent-prompt-copy AGENT=codex
make github-issue-codex-ui
```

Use `make github-issue-latest` when you want the recent workflow list instead of printing the report
body. It shows the selected issue refs, session manifest, PR handoff, and issue-sync posture. For a
specific workflow output directory, pass `GITHUB_ISSUE_WORKFLOW_DIR=/path/to/workflow-output`. Use
`make github-issue-plan-review` after a plan-only preview when you want the selected issue package,
session manifest, context files, prompt commands, planned GitHub mutations, publication policy,
read-only preflight checks, setup commands, Codex app-server commands, and next run command in one
focused view. Use `make github-issue-preflight` when you want Catalyst to run the read-only checks
but leave
repository-target bootstrap as an explicit setup step. Use `make github-issue-preflight-strict`
before handing the planned issue package to an agent so a dirty local checkout fails fast. For real
runs, also pass `GITHUB_ISSUE_REQUIRE_CLEAN_CHECKOUT=1` so the workflow blocks before issue claim or
agent execution if the target checkout is dirty. Use
`make github-issue-next-command` when you only want the next shell command, especially after a
plan-only preview. Use `make github-issue-sync-command` after reviewing an unapplied issue-sync plan
when you want the exact apply command without rebuilding it by hand.
The `GitHub Issue Workbench` panel in the Operator UI shows the same workflow history, selected
issues, labels, recipes, readiness steps, native-agent handoff commands, and copy buttons for those
commands, terminal review, strict preflight, workflow/session evidence paths, prompt text, and
prompt paths. Recent workflow cards include posture badges for preflight, generated agent prompts,
draft PR evidence, issue sync, and plan-only packages, so you can choose the right package without
opening every artifact first. The top value snapshot explains what Catalyst adds for a solo
developer before the command details: fixed issue scope, native-agent handoff, and preserved PR or
issue-sync evidence in one workflow bundle. It also shows the developer operating path: select the issue package,
run strict preflight, continue in Codex, Cursor, or OpenHands, then review PR evidence and sync the
issue. It can copy one compact issue-workflow evidence packet with the selected workflow paths,
review commands, preflight command, issue-sync command, issue-sync comment text, draft PR evidence.
It also exposes one ordered runbook that combines review, strict preflight, native-agent handoff,
next workflow command, issue sync, evidence paths, and the safety boundary into a single clipboard
packet for notes or agent handoff. The same runbook is available as a top-level shortcut on the
selected package so you do not have to scroll before handing the issue to another tool.
The selected package also highlights one recommended next action, preferring the Codex UI bridge
when it is available and falling back to plan review, strict preflight, or the next safe workflow
command so the next click stays obvious across both plan-only and executed packages.
When no issue workflow exists yet, the same panel shows the first `make github-issue-plan` command
with a copy button so the empty state is still actionable.
It also copies a smaller issue-context packet with issue refs, labels, URLs, session manifest, and
native-agent prompt commands when you only need to move the selected source work into Codex, Cursor,
or OpenHands. When you need to tell GitHub, Slack, or your notes where the workflow stands, copy the
short status update instead of pasting the full evidence packet. The same panel opens the recorded
source issue or draft PR directly when that evidence exists. This is easier than manually opening
files under `.continuum/github-issue-workflows/` or searching GitHub for the selected issue again.
Use `make github-issue-agent-prompt AGENT=codex`, `AGENT=cursor`, or `AGENT=openhands` when you want
to paste the generated workflow prompt directly into the agent's native UI. Use
`make github-issue-agent-prompt-path AGENT=codex` when a script only needs the file path. Use
`make github-issue-agent-prompt-command AGENT=codex` when you want to show or reuse the exact command
without opening the workflow report. Use `make github-issue-agent-prompt-copy AGENT=codex` when you
want Catalyst to copy the prompt straight to your clipboard.

If Codex is your selected agent and you want Catalyst to start the generated prompt through Codex
app-server instead of copying it to the clipboard, run `make github-issue-codex-ui`. By default this
starts a standalone `codex app-server` process and writes evidence under
`.continuum/codex-app-server-runs/`. When a Codex Desktop or IDE app-server control socket is
already running, use `make github-issue-codex-ui CODEX_APP_SERVER_MODE=proxy` for the best chance of
attaching the generated turn to that live Codex UI.

If you want to preview the selected issue and GitHub mutation posture before starting agents, run:

```bash
make github-issue-plan \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  GITHUB_ISSUE_ARGS="--label bug --limit 5"
```

This writes `workflow-plan.md` with the selected issue, agent handoff prompts, copy-ready prompt
commands, session context files, PR strategy, draft PR intent, issue-sync status, and the next
command to run the real workflow.

If you want the source issue to show that Catalyst accepted it before the local run starts, enable
the claim plan:

```bash
make github-issue-run \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  GITHUB_ISSUE=123 \
  GITHUB_ISSUE_CLAIM=1 \
  GITHUB_ISSUE_REQUIRE_CLEAN_CHECKOUT=1
```

This is still dry-run by default. Add `GITHUB_ISSUE_CLAIM_APPLY=1` only when you want the
`continuum:in-progress` comment and label applied before execution.

When you are ready for live GitHub handoff, keep repository-target enforcement on and add the draft
PR step:

```bash
make github-issue-run \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  REPOSITORY_TARGET_ID=primary \
  GITHUB_ISSUE=123 \
  GITHUB_ISSUE_CREATE_DRAFT_PR=1 \
  GITHUB_ISSUE_REQUIRE_CLEAN_CHECKOUT=1
```

The flow opens or reuses the draft PR through the orchestrator, then passes that PR URL into the
issue update plan. Issue comments and labels still stay dry-run unless you also set
`GITHUB_ISSUE_SYNC_APPLY=1`.

After the run produces a local PR candidate or draft PR, sync the evidence back to the source issue:

```bash
make github-issue-sync \
  GITHUB_ISSUE_SYNC_PR_URL=https://github.com/OWNER/REPO/pull/123
```

This first writes a dry-run plan. When the comment, labels, branch, commit, and PR URL look right,
add `GITHUB_ISSUE_SYNC_APPLY=1`. Use `GITHUB_ISSUE_SYNC_STATUS=done` only when the issue should be
closed as completed.

If `make github-issue-run` fails before review-ready evidence is produced, or if draft PR
publication fails after local evidence is ready, Catalyst prepares a `failed` issue sync plan by
default unless `--skip-issue-sync` is set. That gives a solo developer a clear source-issue
breadcrumb with the failed run or publication output path, instead of requiring them to hunt through
local terminal history.

If the task is not yet tracked as an issue, create a session directly:

```bash
make dev-session \
  TASK_RECIPE=fix-bug \
  TASK="Fix the flaky login retry test" \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout
```

That creates a `.continuum/dev-sessions/` package with `brief.json`, `codex-prompt.md`,
`cursor-prompt.md`, `openhands-prompt.md`, a runbook, detected validation commands, and git context
for the selected checkout. Use it when you want immediate value from an existing coding agent before
deciding whether to submit the brief into the orchestrator.

Each generated prompt includes the brief-derived contract directly: goals, functional
requirements, acceptance criteria, constraints, deliverables, policy limits, validation commands,
and the expected final delivery report. That makes the prompt useful in the native Codex, Cursor,
or OpenHands UI without relying on a reconstructed chat summary.

If you only want the structured brief:

```bash
make dev-task-brief \
  TASK_RECIPE=fix-bug \
  TASK="Fix the flaky login retry test" \
  REPOSITORY=OWNER/REPO
```

When you want Catalyst to run the full local control-plane loop, use:

```bash
make dev-run \
  TASK_RECIPE=fix-bug \
  TASK="Fix the flaky login retry test" \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout
```

That command creates the brief, submits it, executes Docker-backed tasks, evaluates policy and
quality, exports a local PR candidate, and generates the handoff package without pushing anything to
GitHub. The printed `pr_export_repository_path`, `pr_export_branch_name`, and
`pr_export_manifest_path` tell you exactly where the local reviewable export was written.

If you already created the session package and want the orchestrator to use that exact input, run:

```bash
make dev-run-brief BRIEF_FILE=.continuum/dev-sessions/<session>/brief.json
```

That path keeps the generated agent prompts and the full control-plane run attached to the same
`brief.json`.

If you just created the session and want the shortest command, run:

```bash
make dev-run-latest-session
```

That resolves the newest session brief and submits it through the same run path.

To find the most recent output later, run:

```bash
make dev-latest
make dev-next
make dev-session-review
make dev-agent-prompt AGENT=codex
make dev-agent-prompt-command AGENT=cursor
make dev-agent-prompt-copy AGENT=codex
make dev-codex-ui
make dev-review
make run-guide RUN_ID=<RUN_ID>
```

This shows the latest brief, agent prompt package, run summary, review markdown, local PR export,
and the recommended next safe action without manually browsing `.continuum/`.

Use `make dev-next` when you only want the next action, or `make dev-next-command` when another
script needs the recommended command.

Use `make dev-session-review` before handing a generated package to Codex, Cursor, or OpenHands. It
prints the selected session manifest, checkout state, brief, runbook, prompt paths, prompt commands,
Codex app-server commands with the selected session `REPO_PATH`, validation commands, and the exact
`make dev-run-brief ...` command for keeping the later run tied to the same input.

Use `make dev-agent-prompt AGENT=codex`, `AGENT=cursor`, or `AGENT=openhands` when you want the
latest generated session prompt without opening `.continuum/dev-sessions/` manually. Use
`make dev-agent-prompt-path AGENT=codex` for native launchers that need only the file path, or
`make dev-agent-prompt-command AGENT=codex` when a wrapper or UI should display the exact command.
Use `make dev-agent-prompt-copy AGENT=codex` when you want to paste the prompt into a native agent
UI without printing the full prompt in the terminal.

Use `make dev-codex-ui` when you want Catalyst to start the latest or selected `codex-prompt.md`
through Codex app-server and persist thread/turn evidence. This gives a smoother Codex handoff than
clipboard copy while keeping Catalyst's prompt, repo path, sandbox policy, and app-server events in
one inspectable run directory. The bridge does not silently approve interactive Codex prompts; keep
human approvals inside Codex itself.

Use `make dev-review` after `make dev-run` when you want the latest review package, agent review
prompt, local PR export paths, and suggested local diff commands in one focused view.

Use `make run-guide RUN_ID=<RUN_ID>` when you already know the run and want the orchestrator-owned
stage, blocker, and next safe action. This is the same guidance exposed to HTTP and MCP clients.

After the run has execution and quality evidence, persist a durable review package:

```bash
make developer-handoff RUN_ID=<RUN_ID>
```

In the operator UI, the same action is available from the selected run under `Run controls` as
`Generate developer handoff`. After it exists, the UI Developer tab also shows the ready-to-run
portable review prompt, a copy button for sending that prompt to Cursor, Codex, OpenHands, or
another review agent, key artifact paths for that reviewer to inspect, and ready-to-run Codex
app-server commands for the persisted review prompt. The Codex commands include the default
standalone mode and the optional proxy mode for an already-running Codex Desktop or IDE app-server.
If you only need file references, use the Developer tab `Evidence packet` copy action instead of the
full prompt. If you need a short status note for yourself, GitHub, or another agent, use `Live run
brief`; it copies the current stage, next safe action, evidence groups, agent lanes, and next
terminal command as Markdown. If you need to update a source issue or PR conversation, use `GitHub
update`; it copies delivery evidence, branch/commit/PR state, repository guard status, and the
human-review boundary as a concise Markdown comment. If you want to leave the browser and continue
the same run in a terminal, use `Next terminal command`; it copies a run-scoped command against the
currently open local UI service.

See [Developer Workflows](developer-workflows.md) for the available recipes and the real-repository
path.

Stop the demo with `Ctrl-C`. If a session is interrupted, run:

```bash
make cleanup
```

## Validate The Demo Entrypoint

For a non-interactive check that starts the same demo, verifies seeded run state, and exits:

```bash
make solo-demo-check
```

Use a fixed port only when you need it:

```bash
make solo-demo SOLO_DEMO_ARGS="--http-port 8080"
```

Use a different seeded scenario when reviewing another pack:

```bash
make solo-demo SOLO_DEMO_ARGS="--scenario mvp-worker-service"
```

## Next Step: Real Repository

After the demo makes sense, configure a repository target before publishing to GitHub:

```bash
make repository-targets-bootstrap \
  REPOSITORY=<OWNER>/<REPO> \
  REPOSITORY_TARGET_ID=<TARGET_ID>
```

Then run the UI with that allowlist:

```bash
CATALYST_REPOSITORY_TARGETS_FILE="$PWD/config/repository-targets.local.yaml" make ui
```

Repository-target enforcement is the boundary between a local demo and a real draft-PR workflow.

For UI inspection of a real local run, keep the disposable run database:

```bash
make dev-run \
  TASK_RECIPE=fix-bug \
  TASK="Fix the flaky login retry test" \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  DEV_RUN_KEEP_DATABASE=1
```

The command prints the exact `make ui` command for the kept database. Run `make cleanup` when you are
done.

## Current Limit

The solo developer path is currently optimized for understanding the value and validating the local
control plane. Live coding-agent execution still depends on a reachable LiteLLM gateway and local
coding model. Use the OpenHands live smoke only when those prerequisites are running:

```bash
OPENHANDS_REAL_AGENT_SMOKE=1 make openhands-agent-task-real-smoke
```
