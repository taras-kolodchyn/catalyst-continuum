Use the `catalyst-continuum` MCP tools to validate the local integration before attempting any code changes.

Work in this order:

1. Call `list_packs` and summarize which repository packs are available.
2. Call `validate_brief` with the contents of `examples/briefs/minimal-cli-tool.yaml`.
3. If validation succeeds, call `submit_brief`.
4. Call `list_runs` and identify the run created from that brief.
5. Call `describe_run` for the newest run and summarize its current status, task counts, and artifacts.

Do not open a GitHub PR.
Do not call `publish_pr_export` or `open_github_pr`.
Stop after the run inspection summary and tell me whether the MCP integration is working correctly.
