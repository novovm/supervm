# Isolated SUPERVM reference

`supervm-20261002/` contains the previous implementation, physically moved from
the repository root at `ee8027131bb55d4741248a805e9bc6183235ebcb`.

- 1,413 local files were hashed before and after relocation; all SHA256 values
  matched. No source, local experiment, ledger, build output or SDK was deleted.
- The move commit preserves the 750 tracked files as identical Git blobs.
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
then reread the root plan. Do not recreate the removed root `crates/` tree.
