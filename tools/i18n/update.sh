#!/bin/sh
# Regenerates po/envio.aps.pot from the QML's qsTr() strings and merges it
# into each po/*.po. With --check, changes nothing and fails if the files
# are out of date (CI). Rust strings come from crates/envio-core/src/text/.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
pot=po/envio.aps.pot
new=$(mktemp)
trap 'rm -f "$new"' EXIT

find qml \( -name '*.qml' -o -name '*.js' \) | LC_ALL=C sort | xgettext --files-from=- --output="$new" \
  --language=JavaScript --keyword=qsTr:1 --keyword=qsTr:1,2c \
  --from-code=UTF-8 --add-comments=TRANSLATORS --package-name=envio.aps

# The core's strings: tr()/tr_n() calls live only in src/text/.
ls crates/envio-core/src/text/*.rs | LC_ALL=C sort | xgettext --files-from=- --output="$new" \
  --join-existing --language=C --keyword= --keyword=tr:1 --keyword=tr_n:1,2 \
  --from-code=UTF-8 --add-comments=TRANSLATORS --package-name=envio.aps

# Older xgettext (the CI image) leaves charset=CHARSET after a join; newer
# ones write UTF-8. Pin it so every version produces the same file.
sed -i '/^"Content-Type:/s/charset=CHARSET/charset=UTF-8/' "$new"

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
