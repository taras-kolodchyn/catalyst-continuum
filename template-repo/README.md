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
- `deploy/compose/.env.example` for private compose overlays
- `deploy/compose/litellm-config.yaml` for private LiteLLM model alias wiring
- `deploy/github-app/app-manifest.template.json` for GitHub App registration
- `deploy/github-app/webhook.env.example` for webhook secret and installation wiring

## Notes

- These files are scaffolds, not an active integration.
- The LiteLLM scaffold keeps the OpenHands or Codex-facing model aliases stable while letting each private instance point those aliases at its own Ollama, LM Studio, vLLM, or other OpenAI-compatible local backend.
- The GitHub App assets define the minimum permissions and events expected by the
  current draft-PR flow. The upstream orchestrator now exposes a signed webhook
  intake path plus durable delivery inspection surfaces. The first safe
  brief-wired automation step is also real upstream now: the control plane can
  advance one pending webhook action and materialize the freshest matching
  repository signal into a run without bypassing the same audit trail and gate
  checks used by the lower-level commands.
- Keep secrets and private keys out of git history even in the private template repo.
