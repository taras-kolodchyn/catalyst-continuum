# Developer Workflows

Catalyst Continuum should be useful before a team adopts it. The solo-developer workflow is:

1. Turn a daily task into a structured brief.
2. Run the brief through the orchestrator.
3. Let agents execute bounded tasks.
4. Generate a developer handoff package.
5. Review the result in GitHub, Cursor, Codex, OpenHands, or another review surface.

The product value is not that Catalyst writes code instead of a coding agent. The value is that it
keeps the work repeatable, reviewable, and tied to evidence.

## Create A Brief From A Daily Task

Use a developer session when you want a practical starting point for Codex, Cursor, OpenHands, or
another coding agent:

```bash
make dev-session \
  TASK_RECIPE=fix-bug \
  TASK="Fix the flaky login retry test" \
  REPOSITORY=OWNER/REPO
```

The command writes a portable package under `.continuum/dev-sessions/`:

- `brief.json` is the structured Catalyst Continuum brief.
- `codex-prompt.md` is a prompt shaped for Codex.
- `cursor-prompt.md` is a prompt shaped for Cursor.
- `openhands-prompt.md` is a prompt shaped for OpenHands.
- `manifest.json` records the recipe, repository, prompt files, and validation commands.
- `README.md` explains the session order for the human developer.

This is the first reason to use Catalyst before a full orchestrator run: every agent starts from the
same task, validation contract, and evidence discipline instead of a different chat summary.

If you only need the brief file, use a task recipe directly:

```bash
make dev-task-brief \
  TASK_RECIPE=fix-bug \
  TASK="Fix the flaky login retry test" \
  REPOSITORY=OWNER/REPO
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
  REPOSITORY=OWNER/REPO
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
`run-summary.json`. If `CATALYST_DATABASE_URL` is not set, it starts a disposable Postgres container
and removes it when the command exits.

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
make dev-session TASK_RECIPE=fix-bug TASK="Describe the concrete task" REPOSITORY=OWNER/REPO
make dev-task-brief TASK_RECIPE=fix-bug TASK="Describe the concrete task" REPOSITORY=OWNER/REPO
make dev-run TASK_RECIPE=fix-bug TASK="Describe the concrete task" REPOSITORY=OWNER/REPO
```

Use the generated agent prompt when you want immediate Codex, Cursor, or OpenHands help. Use
`dev-run` when you want durable orchestration evidence, local Docker execution, quality gates, a
developer handoff, and a local PR export in one command before opening or accepting a pull request.
