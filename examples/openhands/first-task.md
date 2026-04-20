Use the `catalyst-continuum` MCP tools to validate the local integration before attempting any code changes.

Work in this order:

1. Call `list_packs` and summarize which repository packs are available.
2. Call `validate_brief` with the contents of `examples/briefs/minimal-cli-tool.yaml`.
3. If validation succeeds, call `submit_brief`.
4. Call `list_runs` and identify the run created from that brief.
5. Call `describe_run` for the newest run and summarize its current status, task counts, and artifacts.
6. Call `run_next_task` once for that run so the codex-owned planning task completes.
7. Call `claim_next_agent_task` with `agent=openhands` for that run and summarize the claimed task.
8. If you simulate a longer OpenHands session, call `heartbeat_agent_task` for the claimed task before completion and confirm the task stays `running` with a refreshed lease.
9. Call `complete_agent_task` for the claimed task with a short success summary so the orchestrator persists an `agent_task_report`.
10. Call `describe_run` again and confirm the externally completed task now shows the persisted report linkage.
11. If the run is not terminal yet, call `run_worker_once` for that run and then `describe_run` again. Repeat until the run reaches `succeeded` or `failed`.
12. Call `evaluate_run_policy` for the run.
13. Call `evaluate_run_quality` for the run.
14. Call `describe_artifact` for the persisted `policy_report`, `quality_report`, and `agent_task_report` artifacts and summarize whether the MCP integration is working correctly end to end.

Do not open a GitHub PR.
Do not call `export_pr_candidate`, `publish_pr_export`, or `open_github_pr`.
Stop after the policy/quality and artifact inspection summary and tell me whether the MCP integration is working correctly.
