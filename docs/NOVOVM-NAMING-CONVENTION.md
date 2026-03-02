# NOVOVM Naming Convention

## Scope

This document freezes naming rules for product, documentation, code, and public communication.

## Brand Layers

- External product brand: `NOVOVM`
- Technical abbreviation: `NVM`
- Execution engine name: `AOEM Engine`
- Internal historical codename: `SuperVM` (internal-only)

## Public Usage Rules

- Public website, whitepaper, and release docs must use `NOVOVM`.
- Technical architecture text should use:
  - `NOVOVM` for VM/protocol/runtime product identity
  - `AOEM Engine` for execution kernel identity
- `SuperVM` must not appear as the primary public brand.

## Whitepaper Naming

- Title: `NOVOVM Whitepaper`
- Subtitle: `Powered by AOEM Execution Engine`

## Repository and Crate Naming

- New crate naming baseline:
  - `novovm-node`
  - `novovm-cli`
  - `novovm-exec`
  - `novovm-prover`
- Existing `supervm-*` names are legacy and must be migrated gradually.

## Command Naming

- User-facing command baseline:
  - `novovm start`
  - `novovm status`
  - `novovm metrics`
  - `novovm version`

## Migration Compatibility

- Legacy identifiers can exist temporarily in internal paths for compatibility.
- No new external docs should introduce `SuperVM` as product name.
- All new public docs must follow this convention.
