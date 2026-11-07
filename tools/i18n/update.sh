#!/bin/sh
# Regenerates po/envio.aps.pot from the QML's qsTr() strings and merges it
# into each po/*.po. With --check, changes nothing and fails if the files
# are out of date (CI). Rust strings join in when the core has some (Plan 6).
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
pot=po/envio.aps.pot
new=$(mktemp)
trap 'rm -f "$new"' EXIT

find qml -name '*.qml' | LC_ALL=C sort | xgettext --files-from=- --output="$new" \
  --language=JavaScript --keyword=qsTr:1 --keyword=qsTr:1,2c \
  --from-code=UTF-8 --add-comments=i18n --package-name=envio.aps

# The creation date changes every run; compare without it.
strip() { grep -v '^"POT-Creation-Date:' "$1" || true; }
if [ "${1:-}" = --check ]; then
  if [ "$(strip "$new")" != "$(strip "$pot")" ]; then
    echo "po/envio.aps.pot is out of date: run cargo make pot" >&2
    exit 1
  fi
  exit 0
fi
cp "$new" "$pot"
for po in po/*.po; do
  [ -e "$po" ] || continue
  msgmerge --quiet --update --backup=none "$po" "$pot"
done
