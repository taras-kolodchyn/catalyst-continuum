# Developer Workflows

Catalyst Continuum should be useful before a team adopts it. The solo-developer workflow is:

1. Turn a daily task into a structured brief.
2. Run the brief through the orchestrator.
3. Let agents execute bounded tasks.
4. Generate a developer handoff package.
5. Review the result in GitHub, Cursor, Codex, OpenHands, or another review surface.

The product value is not that Catalyst writes code instead of a coding agent. The value is that it
keeps the work repeatable, reviewable, and tied to evidence.

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
  validation commands.
- `README.md` explains the session order for the human developer.

This is the first reason to use Catalyst before a full orchestrator run: every agent starts from the
same task, validation contract, and evidence discipline instead of a different chat summary.

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
make dev-review
```

It prints the latest briefs, sessions, and runs with the paths that matter most:

- A recommended next action based on the newest artifact.
- The Codex/Cursor/OpenHands prompt paths for a session.
- The review markdown and agent review prompt for a run.
- The local PR export repository, branch, commit, manifest, and combined patch when export exists.
- A short next-action hint for each artifact type.

For focused output, use `dev-next` when you only need the recommended action. For automation or
shell integration, use JSON or command-only output:

```bash
make dev-latest DEV_LATEST_ARGS="--json --limit 1"
make dev-next-command
```

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
```

Use the generated agent prompt when you want immediate Codex, Cursor, or OpenHands help. Use
`dev-run` when you want durable orchestration evidence, local Docker execution, quality gates, a
developer handoff, and a local PR export in one command before opening or accepting a pull request.
