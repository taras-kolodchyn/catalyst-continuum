# catalyst-continuum
Open‑source AI SDLC toolkit that turns a product brief into a working proof‑of‑concept with code, tests, CI/CD and infrastructure. Catalyst Continuum orchestrates AI agents, disposable sandboxes, Model Context Protocol tools and Repo Packs to build draft pull requests from your requirements.

Planning context for Codex and other agents lives in [docs/summary.md](docs/summary.md).

Current CLI flow can promote a completed run into a draft GitHub pull request with a single command:

```bash
catalyst-continuum-orchestrator create-draft-pr \
  --database-url "$CATALYST_DATABASE_URL" \
  --run-id "<RUN_ID>" \
  --remote-url "https://github.com/<OWNER>/<REPO>.git"
```

## CI

GitHub Actions currently validates the bootstrap repository with:

- `cargo fmt --all --check`
- `cargo build --workspace --locked`
- `cargo test --workspace --locked`
- `docker compose --env-file deploy/compose/.env.example -f deploy/compose/compose.yaml config`

Local runs through `act` use the default image pinned in [`.actrc`](.actrc).

```bash
act pull_request -W .github/workflows/ci.yml -j rust
act pull_request -W .github/workflows/ci.yml -j compose
```
