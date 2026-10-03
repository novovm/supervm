# Isolated SUPERVM reference

The 2026-10-03 audit confirmed that this isolation exceeded the user's intended
scope. Restore the product architecture under the
[recovery roadmap](../docs/NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md), including
novovm-node and supporting modules. This archive is preserved recovery material,
not a declaration that every original module should be rewritten or discarded.
The current update changes documentation only; no product paths are restored yet.

`supervm-20261002/` contains the previous implementation, physically moved from
the repository root at `ee8027131bb55d4741248a805e9bc6183235ebcb`.

- The original relocation report recorded 1,413 matching local file hashes.
  The 2026-10-03 audit did not repeat that historical untracked-file comparison
  and does not use it as proof that every local file was preserved.
- The 2026-10-03 Git audit verified all 750 tracked archived files as identical
  to the pre-move blobs at main@eaf4f37.
  The 26 modified and 12 untracked experimental files remain local, unstaged,
  at their corresponding paths inside the archive. They are not accepted code.
- The original Cargo workspace/lockfile, source, vendor code, proof projects,
  scripts, configuration and workflows stay together. Archived workflows are
  **not active GitHub Actions workflows**. Old test claims keep their old scope.
- AOEM stays in the root `aoem/`; historical documents stay in `docs/` and
  `docs_CN/`. Old document links may refer to the pre-move layout; prepend this
  archive path when looking up old source. The Git baseline remains definitive.
- Root builds use the replacement workspace only. This directory is a reference,
  not an alternative running chain or default fallback. Do not run old scripts
  against a live service. Old runtime path discovery needs explicit AOEM paths;
  relocation has not revalidated its integration tests.

No branches were created. B should protect its uncommitted work before pulling,
then reread the recovery roadmap. The former blanket ban on restoring root
`crates/` is superseded; do not equate restoration with enabling faulty hot paths.
