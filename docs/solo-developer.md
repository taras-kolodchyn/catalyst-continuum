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
- Quality, policy, and repository-publication gates that sit outside any single coding agent.
- Repository-target guardrails so draft PR publication goes only to an approved repository, base
  branch, and generated branch prefix.
- One local place to inspect agent logs, artifacts, LiteLLM state, and Grafana observability without
  chasing files across terminals.

The product is not trying to replace the coding agent. It is the local control plane around the
agent.

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
3. Review the delivery summary, review checklist, quality evidence, artifacts, and agent handoff.
4. Open `Agents` to inspect agent lanes and task logs.
5. Open `Flow` to see where the run is in the brief-to-PR lifecycle.

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

## Current Limit

The solo developer path is currently optimized for understanding the value and validating the local
control plane. Live coding-agent execution still depends on a reachable LiteLLM gateway and local
coding model. Use the OpenHands live smoke only when those prerequisites are running:

```bash
OPENHANDS_REAL_AGENT_SMOKE=1 make openhands-agent-task-real-smoke
```
