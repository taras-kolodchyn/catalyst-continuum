# Developer Workflows

Catalyst Continuum should be useful before a team adopts it. The solo-developer workflow is:

1. Turn a daily task into a structured brief.
2. Run the brief through the orchestrator.
3. Let agents execute bounded tasks.
4. Generate a developer handoff package.
5. Review the result in GitHub, Cursor, Codex, OpenHands, or another review surface.

The product value is not that Catalyst writes code instead of a coding agent. The value is that it
keeps the work repeatable, reviewable, and tied to evidence.

If you are evaluating the alpha and want the shortest path first, run:

```bash
make start
```

That prints the recommended demo, GitHub issue preview, native-agent handoff, evidence run, review,
and release-readiness commands without requiring you to read the full workflow catalog.

Catalyst Continuum should not make developers give up the native Codex, Cursor, or OpenHands
experience. Those tools already provide strong interactive coding UX, host-access modes, and
sandbox modes. The orchestrator adds value when it acts as the repository task broker: pick or
receive work from GitHub, package the context, expose the next safe action, preserve evidence, and
enforce quality and publication gates after the agent does the coding.

## Use Native Agent UX

Use the coding agent directly when you are inside an interactive coding loop:

- Let Codex, Cursor, or OpenHands own the editing experience.
- Choose host-full-access or sandboxed execution in the agent that already implements it well.
- Use Catalyst outputs as the task packet, prompt, policy reference, artifact index, and review
  checklist.

Use the orchestrator when the question is bigger than one chat session:

- Which GitHub issue or repository signal should be worked next?
- What context and validation contract should every agent receive?
- What did the agent change, and which artifact proves it?
- Did policy, quality, and repository-target gates pass before PR publication?
- What should a reviewer inspect before trusting the generated change?

## Create A Session From GitHub Issues

Use GitHub issue sessions when the work already exists as GitHub issues and you want Catalyst
Continuum to broker the context without replacing your coding agent's native UI:

```bash
make github-issue-session \
  GITHUB_ISSUE=123 \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout
```

The command uses `gh issue view` and writes one portable developer-session package per issue under
`.continuum/dev-sessions/` by default. Each package contains the normal session files plus:

- `issue.md` for a readable issue snapshot.
- `issue-context.json` for the structured GitHub issue payload.
- `manifest.json` fields that record the source repository, issue number, labels, selected recipe,
  and trust note.
- Codex, Cursor, and OpenHands prompts with the same issue context appended.

Issue title, body, labels, and comments are untrusted repository context. They can describe the
requested change, but they must not override repository policy, `AGENTS.md`, validation commands,
sandboxing, publication gates, or secrets handling.

To rank a small batch and create only the next recommended session, use `github-issue-next`:

```bash
make github-issue-next \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  GITHUB_ISSUE_ARGS="--label bug --limit 5"
```

That command defaults to `gh issue list`, ranks the imported issues, creates one session for the
top-ranked issue, and writes a batch plan under `.continuum/github-issue-batches/`:

- `issue-batch.json` is the machine-readable ranked queue.
- `issue-batch.md` is the human-readable "why this issue next" view.
- `recommended_next_issue_number` and `recommended_next_session_dir` are printed to stdout.

To create sessions for every imported issue instead, pass the list mode through
`GITHUB_ISSUE_ARGS`:

```bash
make github-issue-session \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  GITHUB_ISSUE_ARGS="--list --label bug --limit 5"
```

Choose the PR packaging strategy explicitly when importing a batch:

```bash
make github-issue-session \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  GITHUB_ISSUE_PR_STRATEGY=per-issue \
  GITHUB_ISSUE_ARGS="--list --label bug --limit 5"
```

`per-issue` is the default. It creates one session per issue, and each session is expected to become
its own branch and PR.

```bash
make github-issue-session \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  GITHUB_ISSUE_PR_STRATEGY=batch \
  GITHUB_ISSUE_ARGS="--list --label bug --limit 5"
```

`batch` creates one aggregate session for the imported issue set. Use it when the issues are small,
related, and should be reviewed as one pull request. The batch session contains:

- `brief.json` for the one control-plane run.
- `issue-batch.md` for the readable issue set.
- `issue-batch-context.json` for the structured payloads.
- Codex, Cursor, and OpenHands prompts that tell the agent to keep the work in one branch and one
  PR unless validation shows the batch is unsafe to combine.

`github-issue-next` always creates only the top recommended session. If you set
`GITHUB_ISSUE_PR_STRATEGY=batch` together with `github-issue-next`, the batch plan records the
requested strategy, but the effective session strategy stays `per-issue` because only one issue is
handed to the agent.

For CI-safe or offline testing, import a fixture instead of calling GitHub:

```bash
make github-issue-session \
  GITHUB_ISSUE_JSON=/path/to/issue.json \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout
```

After the issue session exists, keep using the same daily workflow:

```bash
make dev-latest
make dev-run-latest-session
make dev-review
```

This is the first practical broker value above raw Codex/Cursor/OpenHands: Catalyst normalizes the
GitHub issue into one task packet, chooses a recipe from labels when `--recipe auto` is used, and
keeps the path into quality gates, local PR export, and review evidence consistent.

## Run One GitHub Issue Work Package

Use `github-issue-run` when you want one command to take a bounded GitHub issue source through the
local control-plane path and prepare the issue update plan:

```bash
make github-issue-run \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  GITHUB_ISSUE_ARGS="--label bug --limit 5"
```

With the default `per-issue` strategy, Catalyst lists or imports the issue set, ranks it, creates a
session only for the top issue, runs that session, exports a local PR candidate, and writes
`workflow-summary.json` under `.continuum/github-issue-workflows/`. The summary points to the
session, `run-summary.json`, PR export evidence, and GitHub issue sync plan.

The same output directory also contains `workflow-report.md`, which is the developer-facing report.
It summarizes the workflow outcome, selected GitHub issue titles/URLs/labels when recorded,
session, copy-ready Codex/Cursor/OpenHands prompt commands, run evidence, PR export branch/commit,
Codex app-server commands with the selected `REPO_PATH`, draft PR URL when present, issue-sync plan,
and the next human review step. It also embeds ready-to-run review commands for reopening the
report, inspecting JSON artifacts, reading the generated issue-sync plan/comment, starting Codex from
the generated prompt, and asking Catalyst for the reviewed issue-sync apply command. Use it as the
first artifact to read after a run succeeds or fails.

To rediscover that report later:

```bash
make github-issue-latest
make github-issue-plan-review
make github-issue-preflight
make github-issue-preflight-strict
make github-issue-review
make github-issue-next-command
make github-issue-sync-command
make github-issue-agent-prompt AGENT=codex
make github-issue-agent-prompt-path AGENT=openhands
make github-issue-agent-prompt-command AGENT=cursor
make github-issue-agent-prompt-copy AGENT=codex
make github-issue-codex-ui
```

`github-issue-latest` lists recent workflow output directories, selected issue refs, handoff
posture, and the recommended next action.
`github-issue-plan-review` prints the latest or selected plan package with selected issue refs,
session manifest, brief, context files, prompt commands, planned GitHub mutations, publication
policy, read-only preflight checks, setup commands, Codex app-server commands, and the exact next
command. Use it after `github-issue-plan` before handing work to a native agent or running the
workflow for real.
`github-issue-preflight` executes only those read-only checks: local checkout status, remotes, and
GitHub repository access. It prints repository-target bootstrap commands as setup guidance but does
not run them automatically.
`github-issue-preflight-strict` adds a clean-checkout gate on top of the same read-only checks. Use
it before handing a planned issue package to Codex, Cursor, or OpenHands so uncommitted local work
does not get mixed into the generated branch or PR evidence. For the actual execution command, pass
`GITHUB_ISSUE_REQUIRE_CLEAN_CHECKOUT=1`; the workflow will stop before issue claim or agent run if
the target checkout is dirty.
`github-issue-review` prints the latest or selected `workflow-report.md`; pass
`GITHUB_ISSUE_WORKFLOW_DIR=/path/to/workflow-output` when you want a specific run.
`github-issue-next-command` prints only the recommended command so scripts can continue from the
latest plan or report without parsing the full human-readable output.
`github-issue-sync-command` prints only the apply command for the latest unapplied issue-sync plan,
after you have reviewed the generated plan and comment.
The Operator UI mirrors this discovery path in `GitHub Issue Workbench`: it reads the latest
`.continuum/github-issue-workflows/` evidence, lets you select a recent workflow in place, shows
the issue package with labels and recipes, and provides copy buttons for native-agent handoff,
Codex app-server launch, the next review command, and the issue-sync apply command. It is
intentionally read-only, so GitHub still changes only when you run the copied command from a
terminal.
`github-issue-agent-prompt` prints the generated Codex, Cursor, or OpenHands prompt for the latest
or selected workflow. `github-issue-agent-prompt-path` prints only the prompt path, which is better
for shell aliases and native agent launchers. `github-issue-agent-prompt-command` prints the
copy-ready command for the selected agent so a UI or wrapper can show the exact continuation action.
`github-issue-agent-prompt-copy` copies the selected prompt directly to the system clipboard when
`pbcopy`, `wl-copy`, `xclip`, or `xsel` is available.
`github-issue-codex-ui` sends the latest or selected workflow `codex-prompt.md` through Codex
app-server and persists thread/turn evidence under `.continuum/codex-app-server-runs/`. Use
`CODEX_APP_SERVER_MODE=proxy` only when a running Codex Desktop or IDE app-server control socket is
available and you want the best chance of attaching that turn to the live Codex UI.

If you want to inspect that choice before running agents, use the plan-only wrapper:

```bash
make github-issue-plan \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  GITHUB_ISSUE_ARGS="--label bug --limit 5"
```

This creates the selected developer session and writes `workflow-plan.json` plus
`workflow-plan.md`. The plan shows the selected issue(s), PR strategy, claim/apply posture, draft PR
intent, issue-sync status, repository-target settings, and the next command to run the real
workflow without `--plan-only`. When the source issue payload includes title, URL, state, labels,
rank, or selected recipe, those details are included in both the JSON plan and the readable markdown
preview. The plan also lists the generated Codex, Cursor, and OpenHands prompts plus the session
runbook, issue context files, and copy-ready prompt commands so the developer can inspect the
handoff packet before execution. It also includes read-only preflight checks for local checkout
inspection, GitHub repository access, and separate repository-target setup when draft PR publication
needs policy config. Run `make github-issue-plan-review` for a concise terminal view of
the same package without opening the generated files manually.

For a related batch that should stay in one PR:

```bash
make github-issue-run \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  GITHUB_ISSUE_PR_STRATEGY=batch \
  GITHUB_ISSUE_ARGS="--label bug --limit 5"
```

`batch` creates one aggregate session and one local run for the imported issue set. The issue sync
plan then targets every issue in the batch with the same branch, commit, PR URL, labels, and audit
comment.

When you want to mark the issue work package as accepted before the local run starts, enable the
claim step:

```bash
make github-issue-run \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  GITHUB_ISSUE=123 \
  GITHUB_ISSUE_CLAIM=1 \
  GITHUB_ISSUE_REQUIRE_CLEAN_CHECKOUT=1
```

This writes an `in-progress` issue claim plan into the workflow output. Add
`GITHUB_ISSUE_CLAIM_APPLY=1` only when you want Catalyst to comment and label the source issue(s)
through `gh` before execution begins.

By default, issue sync is still a dry run. Add `GITHUB_ISSUE_SYNC_APPLY=1` only after reviewing the
generated `github-issue-sync-plan.json` and `comment.md`.

To continue the same flow all the way to a GitHub draft PR, enable the explicit publication step:

```bash
make github-issue-run \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout \
  REPOSITORY_TARGET_ID=primary \
  GITHUB_ISSUE=123 \
  GITHUB_ISSUE_CREATE_DRAFT_PR=1 \
  GITHUB_ISSUE_REQUIRE_CLEAN_CHECKOUT=1
```

This does not bypass repository policy. The wrapper calls the same orchestrator `create-draft-pr`
command used by the CLI, HTTP, MCP, and UI surfaces, then copies the created PR URL into the issue
sync plan. If the run used a disposable database, the workflow keeps it only long enough for draft
PR publication to read the stored run, quality, and PR candidate state, then cleans it up unless you
explicitly requested `--keep-database`. If draft PR publication fails after the local run produced
evidence, the workflow prepares a `failed` issue sync plan with the draft PR output path before
returning the publication failure code.

## Sync Run Evidence Back To GitHub Issues

After a GitHub issue-derived run has produced review evidence, sync the result back to the source
issue:

```bash
make github-issue-sync \
  GITHUB_ISSUE_SYNC_RUN_SUMMARY=.continuum/dev-runs/<run>/run-summary.json \
  GITHUB_ISSUE_SYNC_PR_URL=https://github.com/OWNER/REPO/pull/123
```

The default is a dry run. It writes:

- `github-issue-sync-plan.json` with the exact issue numbers, labels, state transition, and `gh`
  commands that would run.
- `comment.md` with the GitHub issue comment body.

Apply the update only after you review the plan. If the plan came from `github-issue-run`, ask
Catalyst to print the exact apply command:

```bash
make github-issue-sync-command
```

Or run the apply command manually:

```bash
make github-issue-sync \
  GITHUB_ISSUE_SYNC_RUN_SUMMARY=.continuum/dev-runs/<run>/run-summary.json \
  GITHUB_ISSUE_SYNC_PR_URL=https://github.com/OWNER/REPO/pull/123 \
  GITHUB_ISSUE_SYNC_APPLY=1
```

In `in-progress`, Catalyst comments that the issue work package has been accepted for local
execution, applies `continuum:in-progress`, and leaves the issue open. In `failed`, Catalyst records
the failed run attempt, applies `continuum:failed`, and leaves the issue open with local failure
evidence for the developer to inspect. In `ready-for-review`, Catalyst comments on the issue,
applies labels such as `continuum:ready-for-review`, records the branch/commit/PR URL, and leaves
the issue open for normal review.

When the work is accepted and the issue should move to done, run:

```bash
make github-issue-sync \
  GITHUB_ISSUE_SYNC_STATUS=done \
  GITHUB_ISSUE_SYNC_PR_URL=https://github.com/OWNER/REPO/pull/123 \
  GITHUB_ISSUE_SYNC_APPLY=1
```

`done` adds `continuum:done`, comments with the final evidence, and closes the source issue(s) as
completed. For a batch session, the same comment, labels, branch, and PR URL are applied to every
issue in the batch so each issue carries the same audit trail.

## Create A Brief From A Daily Task

Use a developer session when you want a practical starting point for Codex, Cursor, OpenHands, or
another coding agent:

```bash
make dev-session \
  TASK_RECIPE=fix-bug \
  TASK="Fix the flaky login retry test" \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout
```

The command writes a portable package under `.continuum/dev-sessions/`:

- `brief.json` is the structured Catalyst Continuum brief.
- `codex-prompt.md` is a prompt shaped for Codex.
- `cursor-prompt.md` is a prompt shaped for Cursor.
- `openhands-prompt.md` is a prompt shaped for OpenHands.
- `manifest.json` records the recipe, repository, git branch, dirty-file count, prompt files, and
  validation commands. It also records the prompt contract derived from `brief.json`, including the
  repo pack, runtime provider, sandbox profile, allowed agents, allowed task kinds, and acceptance
  evidence counts.
- `README.md` explains the session order for the human developer.

This is the first reason to use Catalyst before a full orchestrator run: every agent starts from the
same task, validation contract, and evidence discipline instead of a different chat summary.
The generated prompts are intentionally self-contained: they include the brief title, goals,
functional requirements, acceptance criteria, constraints, deliverables, policy limits, validation
commands, and final delivery-report expectations so a native agent session does not depend on a
separate chat summary.

If you only need the brief file, use a task recipe directly:

```bash
make dev-task-brief \
  TASK_RECIPE=fix-bug \
  TASK="Fix the flaky login retry test" \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout
```

The command writes a validated brief under `.continuum/dev-briefs/`. The output is JSON, which is
valid YAML and can be submitted through the normal `submit-brief` path.

Available recipes are stored in [config/task-recipes.json](../config/task-recipes.json):

- `fix-bug` for focused bug fixes with tests.
- `add-feature` for bounded feature work.
- `add-tests` for test coverage improvements.
- `security-hardening` for security-focused changes.
- `docs-update` for documentation work.

You can override the default pack:

```bash
make dev-task-brief \
  TASK_RECIPE=add-tests \
  TASK="Add coverage for repository-target validation" \
  REPOSITORY=OWNER/REPO \
  PACK=cli-tool
```

## Submit The Brief

When local Postgres is available, submit directly:

```bash
./scripts/create-dev-task-brief.sh \
  --recipe fix-bug \
  --task "Fix the flaky login retry test" \
  --repository OWNER/REPO \
  --submit
```

This still uses the same brief validation, planning, pack selection, policy checks, and artifact
lineage as a hand-written brief.

## Run A Local Orchestration

When you want the control plane to do useful work immediately, use `dev-run`:

```bash
make dev-run \
  TASK_RECIPE=fix-bug \
  TASK="Fix the flaky login retry test" \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout
```

This runs the current v0.1 flow end to end:

- Create and validate `brief.json`.
- Submit the brief into Postgres-backed run state.
- Evaluate the run policy.
- Execute runnable tasks through the Docker runtime provider.
- Evaluate quality gates.
- Export a local PR candidate artifact without pushing to GitHub.
- Generate the `developer_handoff` review package with the local export evidence included when the
  export gate passes.

The command writes everything under `.continuum/dev-runs/` by default, including
`run-summary.json`. The summary includes the local `pr_export` branch name, commit SHA, manifest
path, generated repository path, and combined patch path when export succeeds. If
`CATALYST_DATABASE_URL` is not set, it starts a disposable Postgres container and removes it when
the command exits.

If you already created a developer session, run the exact same brief instead of retyping the task:

```bash
make dev-run-brief \
  BRIEF_FILE=.continuum/dev-sessions/<session>/brief.json
```

This copies the existing brief into the new run directory, validates it, submits it, and records the
original path as `brief_source_path` in `run-summary.json`. Use this path when you want Codex,
Cursor, OpenHands, and the orchestrator run to stay anchored to one shared input.

If the session was just created and you do not want to copy a path, run the newest session directly:

```bash
make dev-run-latest-session
```

This resolves the newest `.continuum/dev-sessions/*/brief.json` by session manifest timestamp and
then uses the same `brief_source_path` tracking as `dev-run-brief`.

Keep the disposable database only when you want to inspect the completed run in the operator UI:

```bash
make dev-run \
  TASK_RECIPE=fix-bug \
  TASK="Fix the flaky login retry test" \
  REPOSITORY=OWNER/REPO \
  DEV_RUN_KEEP_DATABASE=1
```

The command prints the exact `make ui` command for that kept database. Clean it up later with:

```bash
make cleanup
```

Use `DEV_RUN_NO_PR_EXPORT=1` when you only want the run evidence and handoff package.

## Find The Latest Output

After a few sessions or runs, use `dev-latest` instead of hunting through `.continuum/` manually:

```bash
make dev-latest
make dev-next
make dev-session-review
make dev-agent-prompt AGENT=codex
make dev-agent-prompt-command AGENT=cursor
make dev-agent-prompt-copy AGENT=codex
make dev-codex-ui
make dev-review
```

It prints the latest briefs, sessions, and runs with the paths that matter most:

- A recommended next action based on the newest artifact.
- The Codex/Cursor/OpenHands prompt paths for a session.
- The focused session handoff view with checkout state, brief, runbook, prompts, validation
  commands, and the matching `dev-run-brief` command.
- Copy-ready `make dev-agent-prompt ... AGENT=...` commands for continuing in the agent UI.
- Clipboard-ready prompt handoff through `make dev-agent-prompt-copy AGENT=...`.
- Codex app-server handoff through `make dev-codex-ui` with the selected session `REPO_PATH` when
  Codex should start from the generated Catalyst prompt without manual copy/paste.
- The review markdown, agent review prompt, and generated Codex app-server command for a run
  `developer_handoff` artifact.
- The local PR export repository, branch, commit, manifest, and combined patch when export exists.
- A short next-action hint for each artifact type.

For focused output, use `dev-next` when you only need the recommended action. For automation or
shell integration, use JSON or command-only output:

```bash
make dev-latest DEV_LATEST_ARGS="--json --limit 1"
make dev-next-command
make dev-session-review
make dev-session-review DEV_SESSION_DIR=.continuum/dev-sessions/<session>
make dev-agent-prompt-path AGENT=openhands
make dev-agent-prompt-command AGENT=cursor
make dev-agent-prompt-copy AGENT=codex
make dev-codex-ui CODEX_APP_SERVER_MODE=proxy
```

Use `dev-session-review` before handing work to a native agent or before converting a session into a
run. It is the shortest readable view of the selected package and avoids opening `manifest.json`,
`brief.json`, `README.md`, and three prompt files manually.

`dev-codex-ui` and `github-issue-codex-ui` are bridge commands, not a new agent runtime. The
orchestrator still owns task packets, policy, evidence, and publication gates; Codex owns the
interactive coding UX, sandbox behavior, and any human approval UI.

When a run reaches the review stage, `generate-developer-handoff` writes a stable
`agent-review-prompt.md` plus `codex_app_server_commands` into the handoff manifest and artifact
metadata. The operator UI Developer tab renders those commands next to the review prompt so the
handoff can continue in Codex from the same evidence package. The UI copy action also includes the
key artifact paths in the prompt, so a second agent can open the quality report, PR handoff, runtime
logs, and agent reports without reconstructing them from the artifact table.
The adjacent `Evidence packet` copy action provides the same prioritized artifact path list without
the full prompt when you want a compact terminal or agent handoff.
The adjacent `Live run brief` copy action provides a short Markdown status note with the current
stage, next safe action, evidence groups, agent lanes, and next terminal command. Use it for GitHub
issue updates, personal notes, or a quick Codex/Cursor/OpenHands continuation where the full review
prompt would be too heavy.
The adjacent `GitHub update` copy action is shaped as an issue or PR status comment. It includes
delivery evidence, branch/commit/remote/PR state when available, repository guard status, reviewer
starting points, and a reminder that human approval remains in GitHub.
The `Next terminal command` copy action provides a run-scoped `curl` command against the currently
open local UI service, so you can continue the orchestrator-recommended action outside the browser
without exposing database credentials. Matching run-action `make` targets are available when your
shell already has the same database environment.

After a local run, use `dev-review` when you want the review surface without the full artifact
index. It prints the latest run summary, `review.md`, reusable agent review prompt, local PR export
repository, branch, commit, manifest, combined patch, and safe local inspection commands.

## Ask The Orchestrator What Is Safe Next

When a run exists but the next control-plane action is not obvious, use the run guide instead of
manually reconstructing state from task rows and artifacts:

```bash
make run-guide RUN_ID=<RUN_ID>
```

The same shared guide is exposed on every orchestrator surface:

- CLI: `describe-run-guide`.
- HTTP: `GET /runs/{run_id}/guide`.
- MCP: `describe_run_guide`.

The guide is read-only. It returns the current lifecycle stage, progress summary, blocker detail,
recommended next action, suggested CLI command, and matching MCP tool call when one exists.

## Mix Local Workers And External Agents

When a run is meant to delegate work to Codex, OpenHands, or another external executor, do not let a
generic local worker consume that agent-owned queue. Use the assignment-respecting mode:

```bash
cargo run --manifest-path orchestrator/Cargo.toml -- run-next-task \
  --database-url "$CATALYST_DATABASE_URL" \
  --run-id <RUN_ID> \
  --respect-agent-assignments
```

The same guard is available on `worker` and the MCP `run_next_task` / `run_worker_once` tools as
`respect_agent_assignments`. In that mode, the local Docker executor only runs unassigned tasks and
leaves `assigned_agent` tasks for `claim_next_agent_task`, `prepare_agent_task_workspace`,
`heartbeat_agent_task`, and `complete_agent_task`.

## Generate A Developer Handoff

After a run has produced execution and quality evidence, create a durable handoff package:

```bash
make developer-handoff RUN_ID=<RUN_ID>
```

You can do the same from the operator UI:

1. Open `/ui`.
2. Select the run from the ledger.
3. Open `Run controls`.
4. Click `Generate developer handoff`.

The command persists a `developer_handoff` artifact containing:

- `review.md` for human review.
- `agent-review-prompt.md` for Cursor, Codex, OpenHands, or another review agent.
- `manifest.json` with task counts, artifact groups, recent events, and review checklist state.

The operator UI Developer tab renders the review prompt with a copy action so a solo developer can
hand the same evidence package to another agent without manually selecting the prompt text. The
copied prompt includes the key artifact paths that reviewer should inspect first. Use `Evidence
packet` in the same tab when you only need the artifact path list. Use `Next terminal command` when
you want a local-service command for the selected run guide action.

The same command layer is exposed as:

- CLI: `generate-developer-handoff`.
- HTTP: `POST /runs/{run_id}/developer-handoff`.
- UI: `Generate developer handoff`.

This is the difference from directly prompting a coding agent: the next reviewer receives a stable
evidence package instead of a reconstructed chat summary.

## What To Use Today

For a real repository, start with:

```bash
make repository-targets-bootstrap REPOSITORY=OWNER/REPO REPOSITORY_TARGET_ID=local-dev
make github-issue-next REPOSITORY=OWNER/REPO REPO_PATH=/path/to/local/checkout GITHUB_ISSUE_ARGS="--label bug --limit 5"
make github-issue-session GITHUB_ISSUE=123 REPOSITORY=OWNER/REPO REPO_PATH=/path/to/local/checkout
make dev-session TASK_RECIPE=fix-bug TASK="Describe the concrete task" REPOSITORY=OWNER/REPO REPO_PATH=/path/to/local/checkout
make dev-task-brief TASK_RECIPE=fix-bug TASK="Describe the concrete task" REPOSITORY=OWNER/REPO REPO_PATH=/path/to/local/checkout
make dev-run TASK_RECIPE=fix-bug TASK="Describe the concrete task" REPOSITORY=OWNER/REPO REPO_PATH=/path/to/local/checkout
make dev-run-brief BRIEF_FILE=.continuum/dev-sessions/<session>/brief.json
make dev-run-latest-session
make dev-latest
make dev-session-review
make dev-codex-ui
```

Use the generated agent prompt when you want immediate Codex, Cursor, or OpenHands help. Use
`dev-run` when you want durable orchestration evidence, local Docker execution, quality gates, a
developer handoff, and a local PR export in one command before opening or accepting a pull request.
