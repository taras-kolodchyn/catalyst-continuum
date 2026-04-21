Use the `catalyst-continuum` MCP tools to validate the local integration before attempting any code changes.

Launch this task with `./scripts/openhands-launch.sh --full-mcp-surface ...` so
OpenHands can see the deeper stateful MCP tool set. The default launcher
surface is intentionally narrower and only covers the bootstrap path.

Important constraints:

- Start with `list_packs` immediately.
- Treat this prompt as the full task description. Do not spend a step re-opening this task file, listing `examples/openhands/`, or exploring the current directory.
- Prefer the `catalyst-continuum` MCP tools over shell commands whenever one of the listed tools can perform the step directly.
- In OpenHands, these MCP tools are exposed as prefixed tool calls such as `catalyst-continuum_list_packs` and `catalyst-continuum_validate_brief`. Invoke them as MCP tool calls. Do not type those tool names into the terminal.
- Use the exact YAML brief embedded below as `brief_content` for both `validate_brief` and `submit_brief`. Do not paraphrase it, synthesize a new YAML document, or pass the path string as the content.
- Do not rename YAML keys. Keep `summary:` exactly as written below; do not rewrite it to keys such as `schema_summary:`.
- For mutating MCP tools such as `submit_brief`, `run_next_task`, `claim_next_agent_task`, and `complete_agent_task`, include OpenHands wrapper fields like `security_risk` and `summary` in the MCP tool-call arguments. Read-only tools such as `list_packs`, `validate_brief`, and `describe_run` can stay minimal.
- Reuse the `run_id` returned by `submit_brief` for every later run-scoped tool call. Do not call `list_runs` just to rediscover the same run unless `submit_brief` fails to return a `run_id`.
- If an MCP tool call fails, retry or correct that MCP tool call. Do not switch to the terminal to wrap, install, or emulate the same MCP action.
- Do not call the OpenHands finish action before the final artifact and quality summary. Finishing after only `list_packs`, `validate_brief`, or `describe_run` is a failed run, not a completed one.
- The task is incomplete until the final summary cites the run status, the claimed task, the prepared workspace, the completion result, the quality gate result, and the inspected artifacts. If those details are missing, keep working.
- Do not call publication or remote-review tools.
- Stop after the policy, quality, and artifact inspection summary.

Exact brief content to reuse verbatim for `brief_content`
Source path for that brief: `examples/briefs/minimal-cli-tool.yaml`

```yaml
schema_version: v0.1
brief_id: 22222222-2222-2222-2222-222222222222
title: Minimal CLI Tool
summary: >
  Build a minimal command-line proof of concept from a structured brief so the
  orchestrator can validate non-HTTP pack flows end to end.
requested_by: product@example.com
target_users:
  - internal platform engineers
goals:
  - Turn a structured brief into a draft backlog.
  - Scaffold a minimal repository layout for a Rust CLI tool.
functional_requirements:
  - id: CLI-1
    title: Summarize the generated proof of concept
    description: Emit a machine-readable summary of the generated repository.
    priority: must
  - id: CLI-2
    title: List structured requirements
    description: Emit the structured requirement catalog through CLI output.
    priority: must
constraints:
  - Keep the first CLI implementation dependency-light.
  - Use Docker as the initial runtime provider for task execution.
deliverables:
  - backlog artifact
  - scaffold plan artifact
technical_preferences:
  languages:
    - rust
  infrastructure:
    - docker compose
repository:
  host: github
  owner: smartit
  name: catalyst-continuum-cli-demo
  default_branch: main
  visibility: private
execution_preferences:
  repo_pack: cli-tool
  default_runtime_provider: docker
  sandbox_profile: restricted
  orchestrator_model: planner-default
  default_agent: openhands
  allowed_agents:
    - openhands
    - codex
policy:
  max_task_count: 8
  max_total_timeout_seconds: 180
  max_task_retry_count: 1
  allowed_task_kinds:
    - plan
    - scaffold
    - code
    - test
  allowed_runtime_providers:
    - docker
  allowed_sandbox_profiles:
    - restricted
```

Work in this order:

1. Call `list_packs` and summarize which repository packs are available.
   Exact MCP tool call example:
   Tool: `catalyst-continuum_list_packs`
   Arguments: `{}`
2. Call `validate_brief` with the exact YAML block above in `brief_content` and `examples/briefs/minimal-cli-tool.yaml` in `brief_source_path`.
   Exact MCP tool call shape:
   Tool: `catalyst-continuum_validate_brief`
   Arguments:
   ```json
   {
     "brief_source_path": "examples/briefs/minimal-cli-tool.yaml",
     "brief_content": "<paste the exact YAML block above verbatim>"
   }
   ```
3. If validation succeeds, call `submit_brief` with the same exact YAML block in `brief_content` and the same logical source path.
   Exact MCP tool call shape:
   Tool: `catalyst-continuum_submit_brief`
   Arguments:
   ```json
   {
     "security_risk": "LOW",
     "summary": "Submit the validated minimal CLI brief to create one run for the stateful MCP exercise.",
     "brief_source_path": "examples/briefs/minimal-cli-tool.yaml",
     "brief_content": "<paste the exact YAML block above verbatim>"
   }
   ```
4. Read the `run_id` returned by `submit_brief`.
5. Call `describe_run` for that `run_id` and summarize its current status, task counts, and artifacts.
   Exact MCP tool call shape:
   Tool: `catalyst-continuum_describe_run`
   Arguments:
   ```json
   {
     "run_id": "<the run_id returned by submit_brief>"
   }
   ```
6. Call `run_next_task` once for that `run_id` so the codex-owned planning task completes.
7. Call `claim_next_agent_task` with `agent=openhands` for that `run_id` and summarize the claimed task.
8. Call `prepare_agent_task_workspace` for the claimed task and summarize the returned `workspace_root` plus `source_kind`.
9. If you simulate a longer OpenHands session, call `heartbeat_agent_task` for the claimed task before completion and confirm the task stays `running` with a refreshed lease.
10. Call `complete_agent_task` for the claimed task with a short success summary and the prepared `workspace_root` so the orchestrator can persist the real task output plus an `agent_task_report`.
11. Call `describe_run` again and confirm the externally completed task now shows the persisted report linkage.
12. If the run is not terminal yet, call `run_worker_once` for that `run_id` and then `describe_run` again. Repeat until the run reaches `succeeded` or `failed`.
13. Call `evaluate_run_policy` for that `run_id`.
14. Call `evaluate_run_quality` for that `run_id`.
15. Call `describe_artifact` for the persisted `policy_report`, `quality_report`, and `agent_task_report` artifacts and summarize whether the MCP integration is working correctly end to end.

Do not use PR export, publication, or remote review tooling.
Stop after the policy/quality and artifact inspection summary and tell me whether the MCP integration is working correctly.
