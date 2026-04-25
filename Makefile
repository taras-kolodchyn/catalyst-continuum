SHELL := /bin/bash

ACT_ARGS ?=
ACT_JOB ?= rust
ARTIFACT_ROOT ?=
BRIEF_FILE ?=
COMPOSE := docker compose --env-file deploy/compose/.env.example -f deploy/compose/compose.yaml
CONTINUUM_ROOT ?=
DATABASE_URL ?=
DEV_SESSION_ARGS ?=
DEV_SESSION_OUTPUT_DIR ?=
DEV_RUN_ARGS ?=
DEV_RUN_KEEP_DATABASE ?=
DEV_RUN_MAX_TASK_CYCLES ?=
DEV_RUN_NO_PR_EXPORT ?=
DEV_RUN_OUTPUT_DIR ?=
DEV_LATEST_ARGS ?=
DOCTOR_ARGS ?=
GITHUB_ISSUE ?=
GITHUB_ISSUE_ARGS ?=
GITHUB_ISSUE_BATCH_OUTPUT_DIR ?=
GITHUB_ISSUE_CLAIM ?=
GITHUB_ISSUE_CLAIM_APPLY ?=
GITHUB_ISSUE_CLAIM_ARGS ?=
GITHUB_ISSUE_CREATE_DRAFT_PR ?=
GITHUB_ISSUE_DRAFT_PR_ARGS ?=
GITHUB_ISSUE_DRAFT_PR_REMOTE_URL ?=
GITHUB_ISSUE_JSON ?=
GITHUB_ISSUE_OUTPUT_ROOT ?=
GITHUB_ISSUE_PR_STRATEGY ?= per-issue
GITHUB_ISSUE_SYNC_APPLY ?=
GITHUB_ISSUE_SYNC_ARGS ?=
GITHUB_ISSUE_SYNC_OUTPUT_DIR ?=
GITHUB_ISSUE_SYNC_PR_URL ?=
GITHUB_ISSUE_SYNC_RUN_SUMMARY ?=
GITHUB_ISSUE_SYNC_STATUS ?= ready-for-review
GITHUB_ISSUE_WORKFLOW_PLAN_ONLY ?=
GITHUB_ISSUE_WORKFLOW_ARGS ?=
GITHUB_ISSUE_WORKFLOW_OUTPUT_DIR ?=
LITELLM_SMOKE_ARGS ?=
OPENHANDS_AGENT_TASK_SMOKE_ARGS ?=
OPENHANDS_AGENT_TASK_REAL_SMOKE_ARGS ?=
OPERATOR_UI_ARGS ?=
OPERATOR_UI_SMOKE_ARGS ?=
OPERATOR_UI_REPOSITORY_TARGETS_FILE ?=
PACK ?=
SOLO_DEMO_ARGS ?=
REPOSITORY ?=
REPOSITORY_ALLOW_READONLY ?=
REPOSITORY_DEFAULT_BRANCH ?=
REPOSITORY_BOOTSTRAP_ARGS ?=
REPOSITORY_JSON ?=
REPO_PATH ?=
REPOSITORY_TARGET_ID ?=
REPOSITORY_TARGETS_FILE ?= config/repository-targets.local.yaml
REPOSITORY_TARGETS_FORCE ?=
REPOSITORY_BRANCH_PREFIX ?= continuum/
RUN_ID ?=
TASK ?=
TASK_BRIEF_ARGS ?=
TASK_RECIPE ?= fix-bug
SMOKE_SCENARIO ?= all
UI_PORT ?= 8080
REPOSITORY_TARGETS_FILE_RUN_ARG = $(if $(wildcard $(REPOSITORY_TARGETS_FILE)),--repository-targets-file "$(REPOSITORY_TARGETS_FILE)")

.DEFAULT_GOAL := help

.PHONY: help
help: ## Show available Make targets.
	@awk 'BEGIN {FS = ":.*##"; printf "Catalyst Continuum targets:\n\n"} /^[a-zA-Z0-9_.-]+:.*##/ {printf "  %-38s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

.PHONY: check
check: doctor versions markdown-links repository-targets-smoke dev-artifacts-smoke dev-session-smoke github-issue-session-smoke github-issue-sync-smoke github-issue-workflow-smoke template-repo-check lint-shell lint-ui-assets rust ## Run the standard fast local validation set.

.PHONY: doctor
doctor: ## Run fast local readiness checks for tools, config, and optional live services.
	./scripts/doctor.sh $(DOCTOR_ARGS)

.PHONY: ci
ci: versions markdown-links lint-shell lint-ui-assets repository-targets-smoke template-repo-check rust compose eval smoke ## Run the broad local validation set.

.PHONY: ci-full
ci-full: ci sbom ## Run broad local validation plus SBOM generation.

.PHONY: release-check
release-check: doctor ci solo-demo-check ui-smoke ui-smoke-repository-policy ui-smoke-repository-policy-blocked ## Run the v0.1 release-baseline validation gate.

.PHONY: versions
versions: ## Check pinned versions, workflow pins, and image refs.
	./scripts/check-versions.sh

.PHONY: markdown-links
markdown-links: ## Check local Markdown links and reject absolute local paths.
	./scripts/check-markdown-links.sh

.PHONY: lint-shell
lint-shell: ## Lint shell scripts with shellcheck or the pinned shellcheck image.
	./scripts/lint-shell.sh

.PHONY: lint-ui-assets
lint-ui-assets: ## Lint static operator UI assets for JS syntax and CSS token drift.
	./scripts/lint-operator-ui-assets.sh

.PHONY: rust
rust: ## Run Rust fmt, clippy, build, tests, and MCP smoke checks.
	./scripts/ci-rust.sh

.PHONY: fmt
fmt: ## Check Rust formatting.
	cargo fmt --all --check

.PHONY: clippy
clippy: ## Run Rust clippy with repository warnings policy.
	cargo clippy --workspace --all-targets -- -D warnings

.PHONY: build
build: ## Build the full Rust workspace with locked dependencies.
	cargo build --workspace --locked

.PHONY: test
test: ## Run the full Rust test suite with locked dependencies.
	cargo test --workspace --locked

.PHONY: compose
compose: compose-config compose-runtime compose-observability ## Validate compose config, runtime contract, and observability wiring.

.PHONY: compose-config
compose-config: ## Validate Docker Compose configuration.
	./scripts/ci-compose.sh

.PHONY: compose-runtime
compose-runtime: ## Validate container runtime layout through orchestrator and worker containers.
	./scripts/compose-runtime-check.sh

.PHONY: compose-observability
compose-observability: ## Validate observability stack readiness without rebuilding images.
	./scripts/compose-observability-smoke.sh --no-build

.PHONY: compose-up
compose-up: ## Start the full local compose stack with build.
	$(COMPOSE) up --build

.PHONY: compose-down
compose-down: ## Stop the full local compose stack without deleting volumes.
	$(COMPOSE) down

.PHONY: ui
ui: ## Start the host-run operator UI; override UI_PORT and OPERATOR_UI_ARGS as needed.
	./scripts/run-operator-ui.sh --http-port "$(UI_PORT)" $(if $(OPERATOR_UI_REPOSITORY_TARGETS_FILE),--repository-targets-file "$(OPERATOR_UI_REPOSITORY_TARGETS_FILE)") $(OPERATOR_UI_ARGS)

.PHONY: solo-demo
solo-demo: ## Start a seeded solo-developer demo UI session.
	./scripts/solo-demo.sh $(SOLO_DEMO_ARGS)

.PHONY: solo-demo-check
solo-demo-check: ## Verify the seeded solo-developer demo starts and exposes run state.
	./scripts/solo-demo.sh --check-only $(SOLO_DEMO_ARGS)

.PHONY: dev-demo
dev-demo: solo-demo ## Alias for solo-demo.

.PHONY: developer-handoff
developer-handoff: ## Generate a review.md + agent prompt package for RUN_ID=<uuid>.
	@test -n "$(RUN_ID)" || { echo "RUN_ID is required"; exit 2; }
	cargo run --quiet --package catalyst-continuum-orchestrator -- generate-developer-handoff --run-id "$(RUN_ID)" --pretty $(if $(DATABASE_URL),--database-url "$(DATABASE_URL)") $(if $(ARTIFACT_ROOT),--artifact-root "$(ARTIFACT_ROOT)")

.PHONY: run-guide
run-guide: ## Show the orchestrator-owned next safe action for RUN_ID=<uuid>.
	@test -n "$(RUN_ID)" || { echo "RUN_ID is required"; exit 2; }
	cargo run --quiet --package catalyst-continuum-orchestrator -- describe-run-guide --run-id "$(RUN_ID)" $(if $(DATABASE_URL),--database-url "$(DATABASE_URL)")

.PHONY: dev-task-brief
dev-task-brief: ## Create a structured brief from TASK="..." and TASK_RECIPE=fix-bug.
	@test -n "$(TASK)" || { echo "TASK is required"; exit 2; }
	./scripts/create-dev-task-brief.sh --task "$(TASK)" --recipe "$(TASK_RECIPE)" $(if $(REPOSITORY),--repository "$(REPOSITORY)") $(if $(REPOSITORY_DEFAULT_BRANCH),--default-branch "$(REPOSITORY_DEFAULT_BRANCH)") $(if $(REPO_PATH),--repo-path "$(REPO_PATH)") $(if $(PACK),--pack "$(PACK)") $(TASK_BRIEF_ARGS)

.PHONY: dev-session
dev-session: ## Create brief + Codex/Cursor/OpenHands prompts for TASK="...".
	@test -n "$(TASK)" || { echo "TASK is required"; exit 2; }
	./scripts/create-dev-session.sh --task "$(TASK)" --recipe "$(TASK_RECIPE)" $(if $(REPOSITORY),--repository "$(REPOSITORY)") $(if $(REPOSITORY_DEFAULT_BRANCH),--default-branch "$(REPOSITORY_DEFAULT_BRANCH)") $(if $(REPO_PATH),--repo-path "$(REPO_PATH)") $(if $(PACK),--pack "$(PACK)") $(if $(DEV_SESSION_OUTPUT_DIR),--output-dir "$(DEV_SESSION_OUTPUT_DIR)") $(DEV_SESSION_ARGS)

.PHONY: github-issue-session
github-issue-session: ## Create Codex/Cursor/OpenHands packages from GITHUB_ISSUE or GITHUB_ISSUE_JSON.
	./scripts/create-github-issue-session.sh --pr-strategy "$(GITHUB_ISSUE_PR_STRATEGY)" $(if $(GITHUB_ISSUE),--issue "$(GITHUB_ISSUE)") $(if $(GITHUB_ISSUE_JSON),--issue-json "$(GITHUB_ISSUE_JSON)") $(if $(REPOSITORY),--repository "$(REPOSITORY)") $(if $(REPOSITORY_DEFAULT_BRANCH),--default-branch "$(REPOSITORY_DEFAULT_BRANCH)") $(if $(REPO_PATH),--repo-path "$(REPO_PATH)") $(if $(PACK),--pack "$(PACK)") $(if $(GITHUB_ISSUE_OUTPUT_ROOT),--output-root "$(GITHUB_ISSUE_OUTPUT_ROOT)") $(GITHUB_ISSUE_ARGS)

.PHONY: github-issue-next
github-issue-next: ## Rank a GitHub issue batch and create only the next recommended session.
	./scripts/create-github-issue-session.sh --next-only --pr-strategy "$(GITHUB_ISSUE_PR_STRATEGY)" $(if $(GITHUB_ISSUE),--issue "$(GITHUB_ISSUE)",$(if $(GITHUB_ISSUE_JSON),--issue-json "$(GITHUB_ISSUE_JSON)",--list)) $(if $(REPOSITORY),--repository "$(REPOSITORY)") $(if $(REPOSITORY_DEFAULT_BRANCH),--default-branch "$(REPOSITORY_DEFAULT_BRANCH)") $(if $(REPO_PATH),--repo-path "$(REPO_PATH)") $(if $(PACK),--pack "$(PACK)") $(if $(GITHUB_ISSUE_OUTPUT_ROOT),--output-root "$(GITHUB_ISSUE_OUTPUT_ROOT)") $(if $(GITHUB_ISSUE_BATCH_OUTPUT_DIR),--batch-output-dir "$(GITHUB_ISSUE_BATCH_OUTPUT_DIR)") $(GITHUB_ISSUE_ARGS)

.PHONY: github-issue-sync
github-issue-sync: ## Comment, label, and optionally close GitHub issues from the latest or selected run summary.
	./scripts/sync-github-issue-status.sh --status "$(GITHUB_ISSUE_SYNC_STATUS)" $(if $(GITHUB_ISSUE_SYNC_RUN_SUMMARY),--run-summary "$(GITHUB_ISSUE_SYNC_RUN_SUMMARY)") $(if $(CONTINUUM_ROOT),--continuum-root "$(CONTINUUM_ROOT)") $(if $(GITHUB_ISSUE_SYNC_OUTPUT_DIR),--output-dir "$(GITHUB_ISSUE_SYNC_OUTPUT_DIR)") $(if $(GITHUB_ISSUE_SYNC_PR_URL),--pr-url "$(GITHUB_ISSUE_SYNC_PR_URL)") $(if $(GITHUB_ISSUE_SYNC_APPLY),--apply) $(GITHUB_ISSUE_SYNC_ARGS)

.PHONY: github-issue-run
github-issue-run: ## Take one GitHub issue work package through session, local run, PR export, and issue sync plan.
	./scripts/run-github-issue-workflow.sh --pr-strategy "$(GITHUB_ISSUE_PR_STRATEGY)" $(if $(GITHUB_ISSUE_WORKFLOW_PLAN_ONLY),--plan-only) $(if $(GITHUB_ISSUE),--issue "$(GITHUB_ISSUE)",$(if $(GITHUB_ISSUE_JSON),--issue-json "$(GITHUB_ISSUE_JSON)",--list)) $(if $(REPOSITORY),--repository "$(REPOSITORY)") $(if $(REPOSITORY_DEFAULT_BRANCH),--default-branch "$(REPOSITORY_DEFAULT_BRANCH)") $(if $(REPO_PATH),--repo-path "$(REPO_PATH)") $(if $(PACK),--pack "$(PACK)") $(if $(GITHUB_ISSUE_OUTPUT_ROOT),--session-output-root "$(GITHUB_ISSUE_OUTPUT_ROOT)") $(if $(GITHUB_ISSUE_BATCH_OUTPUT_DIR),--batch-output-dir "$(GITHUB_ISSUE_BATCH_OUTPUT_DIR)") $(if $(GITHUB_ISSUE_WORKFLOW_OUTPUT_DIR),--workflow-output-dir "$(GITHUB_ISSUE_WORKFLOW_OUTPUT_DIR)") $(if $(DEV_RUN_OUTPUT_DIR),--run-output-dir "$(DEV_RUN_OUTPUT_DIR)") $(if $(DEV_RUN_MAX_TASK_CYCLES),--max-task-cycles "$(DEV_RUN_MAX_TASK_CYCLES)") $(if $(DATABASE_URL),--database-url "$(DATABASE_URL)") $(if $(ARTIFACT_ROOT),--artifact-root "$(ARTIFACT_ROOT)") $(if $(REPOSITORY_TARGET_ID),--repository-target-id "$(REPOSITORY_TARGET_ID)") $(REPOSITORY_TARGETS_FILE_RUN_ARG) $(if $(DEV_RUN_KEEP_DATABASE),--keep-database) $(if $(DEV_RUN_NO_PR_EXPORT),--no-pr-export) $(if $(GITHUB_ISSUE_CLAIM),--claim-issues) $(if $(GITHUB_ISSUE_CLAIM_APPLY),--apply-issue-claim) $(GITHUB_ISSUE_CLAIM_ARGS) $(if $(GITHUB_ISSUE_CREATE_DRAFT_PR),--create-draft-pr) $(if $(GITHUB_ISSUE_DRAFT_PR_REMOTE_URL),--draft-pr-remote-url "$(GITHUB_ISSUE_DRAFT_PR_REMOTE_URL)") $(GITHUB_ISSUE_DRAFT_PR_ARGS) --issue-sync-status "$(GITHUB_ISSUE_SYNC_STATUS)" $(if $(GITHUB_ISSUE_SYNC_PR_URL),--pr-url "$(GITHUB_ISSUE_SYNC_PR_URL)") $(if $(GITHUB_ISSUE_SYNC_OUTPUT_DIR),--sync-output-dir "$(GITHUB_ISSUE_SYNC_OUTPUT_DIR)") $(if $(GITHUB_ISSUE_SYNC_APPLY),--apply-issue-sync) $(GITHUB_ISSUE_ARGS) $(DEV_RUN_ARGS) $(GITHUB_ISSUE_WORKFLOW_ARGS)

.PHONY: github-issue-plan
github-issue-plan: ## Preview selected GitHub issue work package and planned workflow without running it.
	$(MAKE) github-issue-run GITHUB_ISSUE_WORKFLOW_PLAN_ONLY=1

.PHONY: dev-session-smoke
dev-session-smoke: ## Validate developer session package generation.
	./scripts/dev-session-smoke.sh

.PHONY: github-issue-session-smoke
github-issue-session-smoke: ## Validate GitHub issue session package generation without live GitHub.
	./scripts/github-issue-session-smoke.sh

.PHONY: github-issue-sync-smoke
github-issue-sync-smoke: ## Validate dry-run GitHub issue status sync plans without live GitHub mutation.
	./scripts/github-issue-sync-smoke.sh

.PHONY: github-issue-workflow-smoke
github-issue-workflow-smoke: ## Validate the GitHub issue run wrapper without live GitHub mutation.
	./scripts/github-issue-workflow-smoke.sh

.PHONY: dev-run
dev-run: ## Run TASK="..." through brief, worker execution, quality, handoff, and local PR export.
	@test -n "$(TASK)" || { echo "TASK is required"; exit 2; }
	./scripts/run-dev-task.sh --task "$(TASK)" --recipe "$(TASK_RECIPE)" $(if $(REPOSITORY),--repository "$(REPOSITORY)") $(if $(REPOSITORY_DEFAULT_BRANCH),--default-branch "$(REPOSITORY_DEFAULT_BRANCH)") $(if $(REPO_PATH),--repo-path "$(REPO_PATH)") $(if $(PACK),--pack "$(PACK)") $(if $(DATABASE_URL),--database-url "$(DATABASE_URL)") $(if $(ARTIFACT_ROOT),--artifact-root "$(ARTIFACT_ROOT)") $(if $(DEV_RUN_OUTPUT_DIR),--output-dir "$(DEV_RUN_OUTPUT_DIR)") $(if $(DEV_RUN_MAX_TASK_CYCLES),--max-task-cycles "$(DEV_RUN_MAX_TASK_CYCLES)") $(if $(REPOSITORY_TARGET_ID),--repository-target-id "$(REPOSITORY_TARGET_ID)") $(REPOSITORY_TARGETS_FILE_RUN_ARG) $(if $(DEV_RUN_KEEP_DATABASE),--keep-database) $(if $(DEV_RUN_NO_PR_EXPORT),--no-pr-export) $(DEV_RUN_ARGS)

.PHONY: dev-run-brief
dev-run-brief: ## Run an existing BRIEF_FILE=... through quality, handoff, and local PR export.
	@test -n "$(BRIEF_FILE)" || { echo "BRIEF_FILE is required"; exit 2; }
	./scripts/run-dev-task.sh --brief-file "$(BRIEF_FILE)" $(if $(DATABASE_URL),--database-url "$(DATABASE_URL)") $(if $(ARTIFACT_ROOT),--artifact-root "$(ARTIFACT_ROOT)") $(if $(DEV_RUN_OUTPUT_DIR),--output-dir "$(DEV_RUN_OUTPUT_DIR)") $(if $(DEV_RUN_MAX_TASK_CYCLES),--max-task-cycles "$(DEV_RUN_MAX_TASK_CYCLES)") $(if $(REPOSITORY_TARGET_ID),--repository-target-id "$(REPOSITORY_TARGET_ID)") $(REPOSITORY_TARGETS_FILE_RUN_ARG) $(if $(DEV_RUN_KEEP_DATABASE),--keep-database) $(if $(DEV_RUN_NO_PR_EXPORT),--no-pr-export) $(DEV_RUN_ARGS)

.PHONY: dev-run-latest-session
dev-run-latest-session: ## Run the newest .continuum/dev-sessions/*/brief.json through the local flow.
	./scripts/run-dev-task.sh --latest-session $(if $(CONTINUUM_ROOT),--continuum-root "$(CONTINUUM_ROOT)") $(if $(DATABASE_URL),--database-url "$(DATABASE_URL)") $(if $(ARTIFACT_ROOT),--artifact-root "$(ARTIFACT_ROOT)") $(if $(DEV_RUN_OUTPUT_DIR),--output-dir "$(DEV_RUN_OUTPUT_DIR)") $(if $(DEV_RUN_MAX_TASK_CYCLES),--max-task-cycles "$(DEV_RUN_MAX_TASK_CYCLES)") $(if $(REPOSITORY_TARGET_ID),--repository-target-id "$(REPOSITORY_TARGET_ID)") $(REPOSITORY_TARGETS_FILE_RUN_ARG) $(if $(DEV_RUN_KEEP_DATABASE),--keep-database) $(if $(DEV_RUN_NO_PR_EXPORT),--no-pr-export) $(DEV_RUN_ARGS)

.PHONY: dev-run-smoke
dev-run-smoke: ## Validate the solo-developer local orchestration run entrypoint.
	./scripts/dev-run-smoke.sh

.PHONY: dev-latest
dev-latest: ## Show the latest solo-developer briefs, sessions, runs, and next actions.
	./scripts/show-dev-artifacts.sh $(DEV_LATEST_ARGS)

.PHONY: dev-next
dev-next: ## Show only the recommended next solo-developer action.
	./scripts/show-dev-artifacts.sh --next $(DEV_LATEST_ARGS)

.PHONY: dev-next-command
dev-next-command: ## Print only the recommended next solo-developer command.
	./scripts/show-dev-artifacts.sh --next-command $(DEV_LATEST_ARGS)

.PHONY: dev-review
dev-review: ## Show the latest local run review package and PR export inspection commands.
	./scripts/show-dev-artifacts.sh --review $(DEV_LATEST_ARGS)

.PHONY: dev-artifacts-smoke
dev-artifacts-smoke: ## Validate developer artifact discovery output.
	./scripts/dev-artifacts-smoke.sh

.PHONY: ui-smoke
ui-smoke: ## Run browser-level operator UI smoke against seeded MVP run data.
	./scripts/operator-ui-smoke.sh $(OPERATOR_UI_SMOKE_ARGS)

.PHONY: ui-smoke-repository-policy
ui-smoke-repository-policy: ## Run operator UI smoke with a matching repository-target allowlist.
	./scripts/operator-ui-smoke.sh --repository-targets-file config/repository-targets.example.yaml $(OPERATOR_UI_SMOKE_ARGS)

.PHONY: ui-smoke-repository-policy-blocked
ui-smoke-repository-policy-blocked: ## Run operator UI smoke proving remote publication is blocked by repository policy.
	./scripts/operator-ui-smoke.sh --repository-targets-file config/repository-targets.unmatched.example.yaml --expect-remote-publication-blocked $(OPERATOR_UI_SMOKE_ARGS)

.PHONY: cleanup
cleanup: ## Stop repo-local helper sessions and disposable smoke/UI databases.
	./scripts/cleanup-local-dev.sh

.PHONY: github-repo-preflight
github-repo-preflight: ## Check gh access and permissions for REPOSITORY=owner/repo before real PR publication.
	./scripts/github-repo-preflight.sh $(if $(REPOSITORY),"$(REPOSITORY)") $(if $(REPOSITORY_DEFAULT_BRANCH),--default-branch "$(REPOSITORY_DEFAULT_BRANCH)") $(if $(REPOSITORY_ALLOW_READONLY),--allow-readonly) $(if $(REPOSITORY_JSON),--repo-json "$(REPOSITORY_JSON)")

.PHONY: repository-targets-init
repository-targets-init: ## Generate a local repository-target allowlist for REPOSITORY=owner/repo.
	./scripts/init-repository-targets.sh $(if $(REPOSITORY),"$(REPOSITORY)") --output "$(REPOSITORY_TARGETS_FILE)" --branch-prefix "$(REPOSITORY_BRANCH_PREFIX)" $(if $(REPOSITORY_TARGET_ID),--target-id "$(REPOSITORY_TARGET_ID)") $(if $(REPOSITORY_TARGETS_FORCE),--force) $(if $(REPOSITORY_ALLOW_READONLY),--allow-readonly) $(if $(REPOSITORY_JSON),--repo-json "$(REPOSITORY_JSON)")

.PHONY: repository-targets-bootstrap
repository-targets-bootstrap: ## Bootstrap a local repository-target allowlist plus doctor/preflight in one step.
	./scripts/bootstrap-repository-target.sh $(if $(REPOSITORY),"$(REPOSITORY)") --output "$(REPOSITORY_TARGETS_FILE)" --branch-prefix "$(REPOSITORY_BRANCH_PREFIX)" $(if $(REPOSITORY_TARGET_ID),--target-id "$(REPOSITORY_TARGET_ID)") $(if $(REPOSITORY_TARGETS_FORCE),--force) $(if $(REPOSITORY_ALLOW_READONLY),--allow-readonly) $(if $(REPOSITORY_JSON),--repo-json "$(REPOSITORY_JSON)") $(REPOSITORY_BOOTSTRAP_ARGS)

.PHONY: repository-targets-smoke
repository-targets-smoke: ## Run offline smoke coverage for repository-target bootstrap helpers.
	./scripts/repository-target-bootstrap-smoke.sh

.PHONY: template-repo-check
template-repo-check: ## Validate private template scaffold env/config alignment.
	./scripts/check-template-repo.sh

.PHONY: smoke
smoke: ## Run CI smoke tests; override SMOKE_SCENARIO for one scenario.
	CI_SMOKE_SCENARIO="$(SMOKE_SCENARIO)" ./scripts/ci-smoke.sh

.PHONY: smoke-container
smoke-container: ## Run the container-service MVP smoke scenario.
	CI_SMOKE_SCENARIO=mvp-container-service ./scripts/ci-smoke.sh

.PHONY: smoke-cli
smoke-cli: ## Run the CLI-tool MVP smoke scenario.
	CI_SMOKE_SCENARIO=mvp-cli-tool ./scripts/ci-smoke.sh

.PHONY: smoke-worker
smoke-worker: ## Run the worker-service MVP smoke scenario.
	CI_SMOKE_SCENARIO=mvp-worker-service ./scripts/ci-smoke.sh

.PHONY: smoke-mcp-stateful
smoke-mcp-stateful: ## Run the stateful MCP CLI-tool smoke scenario.
	CI_SMOKE_SCENARIO=mcp-stateful-cli-tool ./scripts/ci-smoke.sh

.PHONY: eval
eval: ## Run the baseline evaluation suite.
	./scripts/eval-baseline.sh

.PHONY: sbom
sbom: ## Generate the orchestrator image SBOM.
	./scripts/generate-sbom.sh

.PHONY: mcp-smoke
mcp-smoke: ## Run stateless MCP handshake and tool discovery smoke.
	./scripts/mcp-smoke.sh

.PHONY: mcp-reference-smoke
mcp-reference-smoke: ## Run upstream MCP Everything reference interoperability smoke.
	./scripts/mcp-reference-smoke.sh

.PHONY: mcp-stateful-smoke
mcp-stateful-smoke: ## Run stateful MCP/OpenHands-facing smoke.
	./scripts/mcp-stateful-smoke.sh

.PHONY: openhands-launch-smoke
openhands-launch-smoke: ## Validate pinned OpenHands launch-profile rendering.
	./scripts/openhands-launch-smoke.sh

.PHONY: openhands-agent-task-smoke
openhands-agent-task-smoke: ## Validate OpenHands executor-wrapper task handoff behavior.
	./scripts/openhands-run-agent-task-smoke.sh $(OPENHANDS_AGENT_TASK_SMOKE_ARGS)

.PHONY: openhands-agent-task-real-smoke
openhands-agent-task-real-smoke: ## Run opt-in live OpenHands executor smoke against LiteLLM/local model.
	OPENHANDS_REAL_AGENT_SMOKE=1 ./scripts/openhands-run-agent-task-smoke.sh --real-agent $(OPENHANDS_AGENT_TASK_REAL_SMOKE_ARGS)

.PHONY: litellm-smoke
litellm-smoke: ## Validate the local LiteLLM gateway contract.
	./scripts/litellm-local-smoke.sh $(LITELLM_SMOKE_ARGS)

.PHONY: act
act: ## Run the local GitHub Actions workflow shape through act.
	./scripts/ci-act.sh $(ACT_ARGS)

.PHONY: act-job
act-job: ## Run one act job; override ACT_JOB and ACT_ARGS as needed.
	./scripts/ci-act.sh -j "$(ACT_JOB)" $(ACT_ARGS)

.PHONY: act-rust
act-rust: ## Run the Rust GitHub Actions job through act.
	./scripts/ci-act.sh -j rust $(ACT_ARGS)

.PHONY: act-shell
act-shell: ## Run the shell GitHub Actions job through act.
	./scripts/ci-act.sh -j shell $(ACT_ARGS)

.PHONY: act-shell-dry
act-shell-dry: ## Dry-run the shell GitHub Actions job shape through act.
	./scripts/ci-act.sh -j shell -n $(ACT_ARGS)

.PHONY: act-rust-dry
act-rust-dry: ## Dry-run the Rust GitHub Actions job shape through act.
	./scripts/ci-act.sh -j rust -n $(ACT_ARGS)

.PHONY: act-smoke
act-smoke: ## Run smoke GitHub Actions matrix slices through act.
	./scripts/ci-act.sh -j smoke $(ACT_ARGS)

.PHONY: act-smoke-dry
act-smoke-dry: ## Dry-run the smoke GitHub Actions job shape through act.
	./scripts/ci-act.sh -j smoke -n $(ACT_ARGS)
