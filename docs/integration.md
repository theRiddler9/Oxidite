# Integration Checkpoints

Track merge status, blockers, and cross-module contract issues here.

## Current phase

**Phase 0 — Scaffold, docs, contracts** (in progress)

## Blockers

None yet.

## Merge checklist (per feature branch)

1. Rebase on latest `main`
2. `cargo fmt --check`
3. `cargo clippy -- -D warnings`
4. `cargo test`
5. Open PR; merge with a **merge commit** (no squash)
6. Delete the short-lived branch after merge

## Contract change protocol

1. Open `contract/<topic>` branch
2. Update `docs/contracts/shared-contracts.md` and matching stubs
3. Merge to `main` before any dependent feature branch continues

## Phase tags

| Tag | Meaning |
|---|---|
| `phase-1-pty` | Headless PTY passthrough works |
| `phase-2-parser` | Parser → Handler stream verified |
| `phase-3-grid` | Grid snapshots match fixtures |
| `phase-4-window` | Window shows live shell output |
| `phase-5-input` | Shell fully usable (incl. vim) |
| `phase-6-polish` | Rendering looks like a real terminal |
| `phase-7-config` | Themes + hot reload |
| `phase-8-release` | Golden demo + release ready |

## Notes

- Record blockers here instead of silently working around a contract.
- Idle CPU must stay near zero once the window exists (no busy redraw loop).
