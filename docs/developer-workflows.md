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

Use a task recipe when you want to avoid hand-writing YAML:

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
make dev-task-brief TASK_RECIPE=fix-bug TASK="Describe the concrete task" REPOSITORY=OWNER/REPO
```

Then submit the generated brief, execute the run through the UI or CLI, and generate the developer
handoff before opening or accepting a pull request.
