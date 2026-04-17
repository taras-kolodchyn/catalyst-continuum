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

Available repository packs can be discovered through the CLI or HTTP API:

```bash
catalyst-continuum-orchestrator list-packs --json
catalyst-continuum-orchestrator describe-pack --pack-id container-service --json
catalyst-continuum-orchestrator validate-brief --file examples/briefs/minimal-cli-tool.yaml --json
curl http://127.0.0.1:8080/packs
curl http://127.0.0.1:8080/packs/container-service
curl -X POST --data-binary @examples/briefs/minimal-cli-tool.yaml http://127.0.0.1:8080/briefs/validate
curl -X POST --data-binary @examples/briefs/minimal-cli-tool.yaml http://127.0.0.1:8080/briefs/submit
curl -X POST http://127.0.0.1:8080/runs/<RUN_ID>/tasks/next
curl -X POST http://127.0.0.1:8080/runs/<RUN_ID>/worker/once
curl -X POST http://127.0.0.1:8080/runs/<RUN_ID>/export-pr-candidate
curl -X POST http://127.0.0.1:8080/runs/<RUN_ID>/draft-pr
curl http://127.0.0.1:8080/runs
curl http://127.0.0.1:8080/runs/<RUN_ID>
```

## CI

GitHub Actions runs one workflow, [`.github/workflows/ci.yml`](.github/workflows/ci.yml), with six required checks:

- `versions`: validates version pins from [`versions.env`](versions.env) against the workflow, Dockerfile, Compose env file, pack image refs, and [`.actrc`](.actrc)
- `shell`: runs ShellCheck across every script under [`scripts/`](scripts)
- `sbom`: builds the orchestrator image, generates an SPDX SBOM, uploads the SBOM artifact, and creates a GitHub/Sigstore provenance attestation for that uploaded artifact
- `rust`: runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace --locked`, and `cargo test --workspace --locked`
- `compose`: validates `deploy/compose/compose.yaml` with the pinned `.env.example`
- `smoke`: exercises the bootstrap flow end to end for both the `container-service` and `cli-tool` packs: `submit-brief -> worker -> export-pr-candidate -> publish-pr-export`

Local runs through `act` use the runner image and container architecture pinned in [`.actrc`](.actrc), with the canonical values tracked in [`versions.env`](versions.env). GitHub-only publication steps such as artifact upload and attestation are skipped under `act`, because local runs do not expose GitHub runtime tokens, OIDC tokens, or the attestations API. The underlying build and SBOM generation steps still run locally.
Pinned version policy and update automation are documented in [VERSIONS.md](VERSIONS.md).
GitHub Actions are pinned to commit SHAs instead of floating tags.
Docker base and runtime images are pinned by tag and digest.

The core checks can be run directly without GitHub Actions:

```bash
./scripts/check-versions.sh
./scripts/lint-shell.sh
./scripts/generate-sbom.sh
./scripts/ci-rust.sh
./scripts/ci-compose.sh
./scripts/ci-smoke.sh
```

`./scripts/smoke-mvp.sh` still runs a single end-to-end smoke pass and accepts `SMOKE_BRIEF_FILE` to target a specific brief, for example `examples/briefs/minimal-container-service.yaml` or `examples/briefs/minimal-cli-tool.yaml`.

To reproduce the workflow structure locally through `act`:

```bash
./scripts/ci-act.sh -l
./scripts/ci-act.sh -j versions
./scripts/ci-act.sh -j shell
./scripts/ci-act.sh -j sbom
./scripts/ci-act.sh -j rust
./scripts/ci-act.sh -j compose
./scripts/ci-act.sh -j smoke
./scripts/ci-act.sh -n
```

To verify a downloaded SBOM artifact against its GitHub attestation:

```bash
gh run download <RUN_ID> -n orchestrator-sbom -D /tmp/orchestrator-sbom
./scripts/verify-github-attestation.sh /tmp/orchestrator-sbom/orchestrator-image.spdx.json
```
