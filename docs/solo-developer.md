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
bounded issue list, produce one work package per issue, and let the developer's preferred agent
complete each package step by step until the result is ready for review.

## Try The Demo Flow

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
make dev-review
make run-guide RUN_ID=<RUN_ID>
```

This shows the latest brief, agent prompt package, run summary, review markdown, local PR export,
and the recommended next safe action without manually browsing `.continuum/`.

Use `make dev-next` when you only want the next action, or `make dev-next-command` when another
script needs the recommended command.

Use `make dev-review` after `make dev-run` when you want the latest review package, agent review
prompt, local PR export paths, and suggested local diff commands in one focused view.

Use `make run-guide RUN_ID=<RUN_ID>` when you already know the run and want the orchestrator-owned
stage, blocker, and next safe action. This is the same guidance exposed to HTTP and MCP clients.

After the run has execution and quality evidence, persist a durable review package:

```bash
make developer-handoff RUN_ID=<RUN_ID>
```

In the operator UI, the same action is available from the selected run under `Run controls` as
`Generate developer handoff`.

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
