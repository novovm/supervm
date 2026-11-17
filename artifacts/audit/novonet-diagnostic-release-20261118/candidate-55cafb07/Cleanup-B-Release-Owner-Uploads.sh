#!/usr/bin/env bash
set -euo pipefail
command -v fuser >/dev/null
files=(
/home/ecs-user/novovm-common-db667aa4-20261117/novovm-common-db667aa4-aoem570-linux-x86_64.tar.gz
/home/ecs-user/novovm-common-5b656189-20261117/novovm-common-5b656189-aoem570-linux-x86_64.tar.gz
)
pins=(21b697c9e74a41914d3b0e3fd628f3e1b7b38c0c27f47968701626cd9d13dcb7 73c79dd02796a22441b40527c97b3cda25bae9667007871dc8451e545a54bc02)
for i in 0 1; do
 archive=${files[$i]}
 test -f "$archive" && test ! -L "$archive"
 test "$(realpath -e "$archive")" = "$archive"
 test "$(stat -c %h "$archive")" = 1
 if fuser -s "$archive"; then echo 'Archive still open' >&2; exit 1; else code=$?; test "$code" = 1; fi
 test "$(sha256sum "$archive" | cut -d' ' -f1)" = "${pins[$i]}"
 stat -c '%s %n' "$archive"
done
df -B1 /
for archive in "${files[@]}"; do rm -- "$archive"; done
echo REMOVED_TWO_MATCHING_UPLOAD_ARCHIVES_ONLY_INSTALLED_RELEASES_UNTOUCHED
df -B1 /
