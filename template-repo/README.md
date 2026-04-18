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
- `deploy/compose/.env.example` for private compose overlays
- `deploy/github-app/app-manifest.template.json` for GitHub App registration
- `deploy/github-app/webhook.env.example` for webhook secret and installation wiring

## Notes

- These files are scaffolds, not an active integration.
- The GitHub App assets define the minimum permissions and events expected by the
  current draft-PR flow, but the webhook handler itself is still a later control-plane step.
- Keep secrets and private keys out of git history even in the private template repo.
