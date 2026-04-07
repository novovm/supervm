# NOVOVM migration script pool

This directory is a frozen migration/history asset pool.

Rules:

- It is not the current mainline strategy-core migration target.
- Files under this directory are not part of the normal Rust policy-core cleanup batch.
- Only reactivate or modify a script here when it is explicitly reintroduced into the current production runtime path.
- Mainline cleanup should prioritize `scripts/*.ps1` root shells and unified Rust policy entrypoints.
