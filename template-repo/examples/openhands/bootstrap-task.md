Use the `catalyst-continuum` MCP tools to prove the shortest local OpenHands integration path before
attempting deeper workflow validation.

Important constraints:

- Start with `list_packs` immediately.
- Treat this prompt as the full task description. Do not re-open this task file, inspect
  `examples/openhands/`, or explore the repository unless a listed MCP step fails.
- Prefer the `catalyst-continuum` MCP tools over shell commands whenever one of the listed tools can
  perform the step directly.
- In OpenHands, these MCP tools are exposed as prefixed tool calls such as
  `catalyst-continuum_list_packs` and `catalyst-continuum_validate_brief`. Invoke them as MCP tool
  calls. Do not type those tool names into the terminal.
- Use the exact YAML brief embedded below as `brief_content` for both `validate_brief` and
  `submit_brief`. Do not paraphrase it, synthesize a new YAML document, or pass the path string as
  the content.
- Do not rename YAML keys. Keep `summary:` exactly as written below; do not rewrite it to keys such
  as `schema_summary:`.
- Keep the `execution_preferences` flow-style mapping on one line exactly as shown below. Do not
  reformat it into an indented block.
- For mutating MCP tools such as `submit_brief`, include OpenHands wrapper fields like
  `security_risk` and `summary` in the MCP tool-call arguments. Read-only tools such as
  `list_packs`, `validate_brief`, `describe_run`, and `describe_run_guide` can stay minimal.
- Reuse the `run_id` returned by `submit_brief` for the follow-up run inspection. Do not call
  `list_runs` just to rediscover the run unless `submit_brief` fails to return a `run_id`.
- If an MCP tool call fails, retry or correct that MCP tool call. Do not switch to the terminal to
  wrap, install, or emulate the same MCP action.
- This task is incomplete until `describe_run` and `describe_run_guide` succeed. Do not call the
  OpenHands finish action before step 6. Finishing after only `list_packs`, `validate_brief`, or
  `submit_brief` is a failed run, not a completed one.
- Your final message must cite the `describe_run` result by naming the run `status`, the task
  counts, the assigned agents, and the persisted artifacts. It must also cite the
  `describe_run_guide` next action. If those details are missing, keep working.
- Stop after the run inspection summary. Do not run workers, claim agent tasks, prepare workspaces,
  or use publication tooling.

Exact brief content to reuse verbatim for `brief_content`
Source path for that brief: `examples/briefs/openhands-bootstrap-cli.yaml`

```yaml
schema_version: v0.1
brief_id: 44444444-4444-4444-4444-444444444444
title: OpenHands Bootstrap CLI
summary: "Validate the shortest OpenHands MCP bootstrap path with a deterministic CLI brief."
goals:
  - "Confirm that OpenHands can validate and submit a brief through MCP."
functional_requirements:
  - id: BOOT-1
    title: "Materialize one inspectable run"
    description: "Create a run that can be inspected through MCP after submission."
constraints:
  - "Keep the bootstrap brief short and deterministic."
execution_preferences: { repo_pack: cli-tool, default_runtime_provider: docker, sandbox_profile: restricted, allowed_agents: [openhands, codex] }
```

Work in this order:

1. Call `list_packs` and summarize which repository packs are available.
   Exact MCP tool call example:
   Tool: `catalyst-continuum_list_packs`
   Arguments: `{}`
2. Call `validate_brief` with the exact YAML block above in `brief_content` and
   `examples/briefs/openhands-bootstrap-cli.yaml` in `brief_source_path`.
   Exact MCP tool call shape:
   Tool: `catalyst-continuum_validate_brief`
   Arguments:
   ```json
   {
     "brief_source_path": "examples/briefs/openhands-bootstrap-cli.yaml",
     "brief_content": "<paste the exact YAML block above verbatim>"
   }
   ```
3. If validation succeeds, call `submit_brief` with the same exact YAML block in `brief_content` and
   the same logical source path.
   Exact MCP tool call shape:
   Tool: `catalyst-continuum_submit_brief`
   Arguments:
   ```json
   {
     "security_risk": "LOW",
     "summary": "Submit the validated bootstrap brief to materialize one inspectable run.",
     "brief_source_path": "examples/briefs/openhands-bootstrap-cli.yaml",
     "brief_content": "<paste the exact YAML block above verbatim>"
   }
   ```
4. Read the `run_id` returned by `submit_brief`.
5. Call `describe_run` for that `run_id` and summarize its status, task counts, assigned agents, and
   persisted artifacts.
   Exact MCP tool call shape:
   Tool: `catalyst-continuum_describe_run`
   Arguments:
   ```json
   {
     "run_id": "<the run_id returned by submit_brief>"
   }
   ```
6. Call `describe_run_guide` for that `run_id` and summarize the recommended next safe action.
   Exact MCP tool call shape:
   Tool: `catalyst-continuum_describe_run_guide`
   Arguments:
   ```json
   {
     "run_id": "<the run_id returned by submit_brief>"
   }
   ```
7. Stop and tell me whether the OpenHands MCP bootstrap path is working.
   A successful final answer must explicitly reference the `describe_run` and `describe_run_guide`
   outputs, not just the `submit_brief` output.
