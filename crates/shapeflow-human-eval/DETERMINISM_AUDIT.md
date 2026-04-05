# Human-Eval Determinism Audit

## Claim Domain

Determinism claim applies to sample generation for equal inputs:

- `seed`
- `difficulty`
- `modality`
- `scene_index`
- unchanged effective config

It does **not** apply to session bootstrap randomness (`rand::random`, `Uuid::now_v7`) or DB wall-clock timestamps.

## Proof-First Status

This repository has strong Verus foundations in `shapeflow-core/proofs` for:

- seed schedule determinism and stream separation,
- config-hash invariants,
- scene accounting invariants,
- target validity invariants,
- canonical ordering/tie-break invariants.

These are prerequisites, but not yet a full end-to-end theorem for the human-eval tuple path.

## Remaining Formal Gaps

The end-to-end composition from

`(seed, difficulty, modality, scene_index) -> native payload / MCP payload`

still needs explicit theorem closure across runtime composition code:

- `build_session_config` (`src/flow.rs`)
- `build_scene_for_index` (`src/flow.rs`)
- `build_plan_item_from_config` (`src/flow.rs`)
- `build_ai_native_sample` (`src/stimulus.rs`)
- `build_mcp_tool_result` (`src/server.rs`)

## Policy Applied

- Removed long-running tuple replay sweeps from unit tests.
- Verification posture is proof-first; runtime replay is treated as secondary regression evidence, not as a proof artifact.
