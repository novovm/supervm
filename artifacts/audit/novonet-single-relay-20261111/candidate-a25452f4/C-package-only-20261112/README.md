# C package received and verified only, 2026-11-12

Latest direct user instruction is copy/verify only, temporarily NO UPDATE.
This narrower instruction governs this receipt even though CD-UPDATE.md offers
a version-specific updater. No updater/package command, node restart, observer,
object send, extraction or D operation was performed here.

Copied the original E: ZIP into persistent C-local storage:
`C:\Users\leadb\AppData\Local\NOVOVM\candidates\novonet-observation-a25452f4-20261112\novonet-observation-a25452f4-windows-x86_64.zip`.
Future use of this copy does not require reconnecting E:.

Source and copy both match150573543 bytes and SHA256
fa198636fa33021ea542850e56f1df17140a5174bfb93fca1ade47f599f14978.
Read-only archive inspection verified77 release-manifest members,77 top checksum
entries,80 total files, manifest/source pins, inherited checksum-file pin and
safe unique paths. No package executable or library was loaded.

The initial supplemental coverage assertion incorrectly expected the top
checksum to exclude only itself; actual manifest design additionally excludes
release-manifest.json and the inherited checksum file. Both are independently
pinned and were verified. This checker assumption was corrected without package
changes or a content-hash failure. Original verification/copy records accompany
this receipt; the ZIP stays local and is not uploaded to Git.

Status: C_A254_PACKAGE_VERIFIED_ONLY. This is not deployment or real-network
double-relay acceptance. Current node/database/identity were not operated.
