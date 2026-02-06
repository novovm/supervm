# Copilot Instructions (SUPERVM)

## Public Language Policy
- Public-facing files must be English-only and ASCII-only.
- No Chinese characters in any public file (including documentation under repo root).
- Keep all public files UTF-8 (no BOM) and ASCII-only.

## Project Phase
- This repository is in a migration / bootstrap phase.
- Prefer correctness, clarity, and minimal changes over completeness or performance.
- Do NOT introduce placeholder files, fake code, or temporary artifacts just to satisfy tools or validations.
- Automation and CI should tolerate missing components during migration.

## Language & Encoding Rules (STRICT)
- **Source code files MUST be ASCII-only and contain no Chinese characters**, including comments.
	- Applies to: `.rs`, `.py`, `.sh`, `.ps1`, `.yml`, `.toml`, `.json`, `.js`, `.ts`, etc.
	- All source code and scripts are written for a global developer audience.
- **Public documentation MUST be English-only and ASCII-only**.
	- Applies to public `.md` files in the repo.
- Keep all files UTF-8 (no BOM).

## Local-Only Notes
- Personal notes or Chinese explanations must live in local-only paths that are ignored by git.
- Keep those notes outside public paths (e.g., under ignored `docs/` or other local-only directories).

## Mandatory Copyright & Attribution (STRICT)
- **All newly created source files MUST include a copyright and attribution header.**
- This rule applies to all source and script files:
	- `.rs`, `.py`, `.sh`, `.ps1`, `.yml`, `.toml`, `.js`, `.ts`, etc.
- Use the following standard header (adapt syntax to the language):

```text
// Copyright (c) 2026 Xonovo Technology
// All rights reserved.
// Author: Xonovo Technology
```

* For languages using `#` comments, use:

```text
# Copyright (c) 2026 Xonovo Technology
# All rights reserved.
# Author: Xonovo Technology
```

* Copilot MUST NOT omit this header when creating new files.
* Existing files should NOT be retroactively modified unless explicitly requested.

## Core Identity (High-level)

* SuperVM is a WASM-first virtual machine system.
* The L0 core is strictly isolated and must not directly import external chain logic.
* Plugins and adapters are opt-in and must respect architectural boundaries.
* Architectural correctness is more important than short-term convenience.

## Non-Negotiable Engineering Rules

* Do NOT bypass MVCC or state versioning rules.
* Do NOT import external chain code into the L0 core.
* ZERO panics in core logic; avoid `.unwrap()` in critical paths.
* Experimental or incomplete functionality MUST be feature-gated.

## Documentation & INDEX Rules

* `docs/INDEX.md` is a human-readable project map, not a full file dump.
* INDEX exists to help the project owner understand file purpose and structure.
* Code Map entries must be limited to:

	* Entry-point source files (e.g., `main.rs`, `lib.rs`, `mod.rs`)
	* Files explicitly listed in `tools/work-logger/index-descriptions.json`
* Never index runtime, cache, environment, or tool-state directories:

	* `.git`, `.venv`, `.idea`, `.vscode`, `.cache`, `node_modules`, `target`,
		`__pycache__`, `.cargo/registry`, `.cargo/git`

## Tooling & CI Behavior

* Avoid adding new tools, scripts, or workflows unless explicitly requested.
* CI must tolerate the absence of Rust code during migration.
* For Rust commands, always use `--manifest-path` when `Cargo.toml` may not be at repo root.
* Avoid `--all-features` unless explicitly asked.
* Do NOT require cargo, rustc, or build tools to be present unless the repository actually contains Rust code.

## Git Hygiene

* Treat environment-specific or local-only directories as ignored.
* Do NOT delete, rewrite, or regenerate user data, logs, or historical records without explicit approval.
* Prefer small, isolated commits with clear intent.

## When in Doubt

* Ask before assuming intent.
* Ask before generating placeholders or restructuring directories.
* Respect the current migration phase and existing design decisions.
