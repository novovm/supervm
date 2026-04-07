# NOVOVM Phase 2-C daemon full Rust shell tasklist

## Closure note (2026-04-07)

- `novovmctl daemon` now owns the mainline daemon shell path.
- `scripts/novovm-prod-daemon.ps1` is a strict compat shell that forwards into `novovmctl daemon`.
- Minimum validation closure passed for:
  - `daemon --dry-run`
  - `daemon --build-before-run --dry-run`
  - `daemon --use-node-watch-mode --lean-io --dry-run`
- Validation confirmed:
  - dry-run no lifecycle state mutation
  - build-before-run enters the Rust daemon path
  - watch/spool env preparation is reflected in audit output
  - unified terminal/json/jsonl output remains intact
- Phase 2-C is closed for minimum mainline Rust-shell validation.
