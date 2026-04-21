Use the `catalyst-continuum` MCP tools to prove the shortest local OpenHands integration path before attempting deeper workflow validation.

Important constraints:

- Start with `list_packs` immediately.
- Treat this prompt as the full task description. Do not re-open this task file, inspect `examples/openhands/`, or explore the repository unless a listed MCP step fails.
- Prefer the `catalyst-continuum` MCP tools over shell commands whenever one of the listed tools can perform the step directly.
- In OpenHands, these MCP tools are exposed as prefixed tool calls such as `catalyst-continuum_list_packs` and `catalyst-continuum_validate_brief`. Invoke them as MCP tool calls. Do not type those tool names into the terminal.
- Use the exact YAML brief embedded below as `brief_content` for both `validate_brief` and `submit_brief`. Do not paraphrase it, synthesize a new YAML document, or pass the path string as the content.
- Stop after the run inspection summary. Do not run workers, claim agent tasks, prepare workspaces, or use publication tooling.

Exact brief content to reuse verbatim for `brief_content`
Source path for that brief: `examples/briefs/openhands-bootstrap-cli.yaml`

```yaml
schema_version: v0.1
brief_id: 44444444-4444-4444-4444-444444444444
title: OpenHands Bootstrap CLI
summary: >
  Validate the shortest OpenHands MCP bootstrap path with a deterministic CLI
  brief.
goals:
  - Confirm that OpenHands can validate and submit a brief through MCP.
functional_requirements:
  - id: BOOT-1
    title: Materialize one inspectable run
    description: Create a run that can be inspected through MCP after submission.
constraints:
  - Keep the bootstrap brief short and deterministic.
execution_preferences:
  repo_pack: cli-tool
  default_runtime_provider: docker
  sandbox_profile: restricted
  allowed_agents:
    - openhands
    - codex
```

Work in this order:

1. Call `list_packs` and summarize which repository packs are available.
2. Call `validate_brief` with the exact YAML block above in `brief_content` and `examples/briefs/openhands-bootstrap-cli.yaml` in `brief_source_path`.
3. If validation succeeds, call `submit_brief` with the same exact YAML block in `brief_content` and the same logical source path.
4. Call `list_runs` and identify the run created from that brief.
5. Call `describe_run` for the newest run and summarize its status, task counts, assigned agents, and persisted artifacts.
6. Stop and tell me whether the OpenHands MCP bootstrap path is working.
