# One directory, one storage budget

This is a thin integration of SQLite's public-domain quota VFS, not another
database or message store. `DiskBudget::configure(app_data_dir)` must run before
any App SQLite connection opens. All default-VFS databases beneath that root
share the same counter; unrelated paths are untouched. The root owner file
excludes a second cooperating process. Existing App single-owner rules remain.

An initial metadata inventory includes unopened databases, indexes (inside their
database files), journals, media, cache and other regular files. Symlink targets
are excluded. Explicit snapshots reconcile metadata; SQLite writes and ordinary
file reservations do not walk directories. Bytes mean logical file lengths,
including free SQLite pages and transient journals, not only useful payload.

Default is no configured ceiling. Finite choices start at 500 MiB. Policy is
stored in two fixed-size checksummed slots and fsynced: a torn newest slot falls
back to the valid previous slot, never silently to unlimited. Both slots invalid
is an error. Changing the choice needs no file growth and does not delete data.

`reserve_for_paths(paths, peak_additional_bytes)` coordinates ordinary writers
with SQLite. Paths include AtomicFile target, `.new` and `.bak`. The caller must
honor the reservation; `finish` (or RAII drop) stats only these paths before
releasing reserved bytes. Reserve zero for deletion. Outside registered roots
it is a no-op. Direct writes that bypass this API cannot be enforced by a SQLite
VFS and must not be used for mutable App files. Android system-owned files and
external document-provider exports are outside this software guarantee.

## Maintenance and shrinking

One percent of a finite quota (at least 1 MiB, at most 16 MiB) is kept inside the
total for rollback journal/WAL/SHM maintenance. Main-file growth and ordinary
reservations cannot consume this reserve. Existing-page writes, reads and small
DELETE/clock/status transactions remain possible at payload capacity. If the
user shrinks below existing data, those files remain readable and no new main
file growth is admitted; at most that bounded journal reserve may transiently
exceed the grandfathered usage. Snapshots expose both `over_limit` and
`maintenance_bytes`. Large maintenance transactions can still return SQLITE_FULL;
they must be bounded/retried or the owner must raise the quota. There is no
unbounded cleanup exemption. Current MailboxStore and MessageJournal use DELETE
journaling. WAL is counted, but applications switching to WAL must separately
validate checkpoint scratch behavior at their limit.

Capacity rejections expose SQLite FULL or `io::ErrorKind::StorageFull`; they
must not disable the account or mark a message delivered. Revision changes on
settings or persistent file reduction, not reservation release or temporary
journal churn, allowing existing work to resume without a retry spin.

## Upstream source and narrow changes

Retrieved 2026-10-21:

- https://raw.githubusercontent.com/sqlite/sqlite/master/src/test_quota.c
- https://raw.githubusercontent.com/sqlite/sqlite/master/src/test_quota.h

Original `test_quota.c` SHA-256:
`9c72c815c330639c4760da922b06eee27aa5ca86923f945ac6672bbeca395bfe`.
Original public-domain blessing remains in both files.

Upstream excludes rollback journals and counts files only after they are opened.
Local changes wrap all named SQLite files, account SHM, disable unaccounted
preallocation hints, serialize physical length changes with quota accounting,
reconcile failed writes, provide bounded maintenance headroom, and add the small
inventory/reservation/snapshot/revision bridge. The exact existing bundled
`libsqlite3-sys = 0.38.2` supplies symbols and headers; there is one SQLite library.

Tests exercise real SQLite transactions, aggregate concurrent databases,
rollback after FULL, inventory of unopened files, shared external reservations,
unlimited/finite/minimum/persistence/torn policy, shrinking without deletion,
maintenance and unaffected unrelated databases. These are local tests, not
Android deployment or production capacity acceptance.

Android/Unix SQLite resolves directory symlinks before quota matching; ordinary
file reservations also resolve existing ancestors and reject escapes from the
configured App root. Windows database access through arbitrary junction aliases
is not covered: use the configured canonical root for database opens there.
