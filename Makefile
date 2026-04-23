SHELL := /bin/bash

ACT_ARGS ?=
ACT_JOB ?= rust
COMPOSE := docker compose --env-file deploy/compose/.env.example -f deploy/compose/compose.yaml
LITELLM_SMOKE_ARGS ?=
OPERATOR_UI_ARGS ?=
SMOKE_SCENARIO ?= all
UI_PORT ?= 8080

.DEFAULT_GOAL := help

.PHONY: help
help: ## Show available Make targets.
	@awk 'BEGIN {FS = ":.*##"; printf "Catalyst Continuum targets:\n\n"} /^[a-zA-Z0-9_.-]+:.*##/ {printf "  %-28s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

.PHONY: check
check: versions lint-shell rust ## Run the standard fast local validation set.

.PHONY: ci
ci: versions lint-shell rust compose eval smoke ## Run the broad local validation set.

.PHONY: ci-full
ci-full: ci sbom ## Run broad local validation plus SBOM generation.

.PHONY: versions
versions: ## Check pinned versions, workflow pins, and image refs.
	./scripts/check-versions.sh

.PHONY: lint-shell
lint-shell: ## Lint shell scripts with shellcheck or the pinned shellcheck image.
	./scripts/lint-shell.sh

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
	./scripts/run-operator-ui.sh --http-port "$(UI_PORT)" $(OPERATOR_UI_ARGS)

.PHONY: cleanup
cleanup: ## Stop repo-local helper sessions and disposable smoke/UI databases.
	./scripts/cleanup-local-dev.sh

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
	./scripts/openhands-run-agent-task-smoke.sh

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

.PHONY: act-rust-dry
act-rust-dry: ## Dry-run the Rust GitHub Actions job shape through act.
	./scripts/ci-act.sh -j rust -n $(ACT_ARGS)

.PHONY: act-smoke
act-smoke: ## Run smoke GitHub Actions matrix slices through act.
	./scripts/ci-act.sh -j smoke $(ACT_ARGS)

.PHONY: act-smoke-dry
act-smoke-dry: ## Dry-run the smoke GitHub Actions job shape through act.
	./scripts/ci-act.sh -j smoke -n $(ACT_ARGS)
