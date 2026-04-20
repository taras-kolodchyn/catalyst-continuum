# Private Instance Template

This directory is the seed for the future private deployment template repository.

It exists so the public upstream can define the expected layout, config surfaces,
and onboarding flow without mixing tenant-specific secrets or infrastructure
choices into the open-source root.

## Intended Use

1. Copy `template-repo/` into a separate private repository.
2. Keep application code, orchestrator logic, and shared packs upstream.
3. Store instance-specific config, runtime-provider settings, secrets wiring,
   and GitHub App registration material in the private repo.

## Included Scaffolds

- `.env.example` for instance-level orchestrator variables
- `.github/CODEOWNERS` for private review ownership
- `config/runtime-providers.yaml` for Docker, Proxmox, and Kubernetes placeholders
- `config/mcp-servers.yaml` for the instance-level external MCP allowlist
- `config/ai-gateway.yaml` for the LiteLLM AI edge-gateway contract
- `deploy/compose/.env.example` for private compose overlays
- `deploy/compose/litellm-config.yaml` for private LiteLLM model alias wiring
- `deploy/github-app/app-manifest.template.json` for GitHub App registration
- `deploy/github-app/webhook.env.example` for webhook secret and installation wiring

## Notes

- These files are scaffolds, not an active integration.
- The LiteLLM scaffold keeps the OpenHands or Codex-facing model aliases stable while letting each private instance point those aliases at its preferred host-managed backend.
- The AI gateway scaffold keeps the LiteLLM ownership boundary explicit: model-facing and retrieval-facing concerns belong in the gateway config, while run policy, runtime control, and publication state still belong in the orchestrator.
- The runtime-provider scaffold is now operational for Docker execution upstream: `providers.docker.network_mode` is passed through to `docker run`, and `providers.docker.rootless: true` requests a best-effort non-root user mapping based on the mounted workspace or artifact path owner.
- The upstream repository defaults to native `mlx-lm` on macOS Apple Silicon and Ollama everywhere else, but the private template can override that through `LITELLM_DEFAULT_MODEL` and the backend-specific env vars in `deploy/compose/.env.example`.
- The upstream macOS-native example uses a coding-tuned MLX model and enables Redis-backed LiteLLM cache entries by default, so private instances inherit a code-oriented local validation path instead of a general chat model.
- The private compose scaffold now also carries `LITELLM_DATABASE_NAME`, because the upstream local stack expects LiteLLM to persist its Prisma-backed proxy state in a dedicated database on the shared Postgres server.
- The private compose scaffold also carries the upstream LiteLLM OTel env knobs, because the upstream local stack expects the gateway to emit official LiteLLM traces and semantic log events into the shared collector baseline instead of running without gateway-level observability.
- The GitHub App assets define the minimum permissions and events expected by the
  current draft-PR flow. The upstream orchestrator now exposes a signed webhook
  intake path plus durable delivery inspection surfaces. The first safe
  brief-wired automation step is also real upstream now: the control plane can
  advance one pending webhook action and materialize the freshest matching
  repository signal into a run without bypassing the same audit trail and gate
  checks used by the lower-level commands.
- Keep secrets and private keys out of git history even in the private template repo.
