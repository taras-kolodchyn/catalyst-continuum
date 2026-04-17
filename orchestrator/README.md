# Orchestrator

This crate is the Rust control plane for Catalyst Continuum.

Its responsibilities will grow into:

- brief ingestion
- run creation and state transitions
- task planning and scheduling
- budget and policy enforcement
- runtime provider coordination
- artifact tracking

For now, the crate provides a minimal CLI skeleton, typed brief parsing, persisted `run` creation, and initial backlog artifact generation.
