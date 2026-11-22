#!/bin/sh
# update-po.sh: collects Arkha's texts to translate from the code into
# po/arkha.pot (gettext's xgettext; texts in tr(), trn() and n_()), then
# brings every po/<language>.po up to date with it (msgmerge): new texts
# are added untranslated, changed ones marked fuzzy for the translator to
# check, and dropped ones removed (no obsolete entries are kept: git has
# them). New source files count before they are committed.
#
# A new translation: msginit -i po/arkha.pot -o po/<code>.po -l <code>, add
# an "X-Language-Name: <the language's name in itself>\n" line to its
# header (Preferences lists it by that), translate, and build: build.rs
# compiles it.
set -e
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
# shellcheck disable=SC2046  # the source files, split: no spaces in their names
xgettext --language=Rust --from-code=UTF-8 \
    --keyword=tr --keyword=trn:1,2 --keyword=n_ \
    --add-comments=TRANSLATORS: --sort-by-file \
    --package-name=Arkha --package-version="$version" --msgid-bugs-address= \
    --output=po/arkha.pot $(git ls-files --cached --others --exclude-standard 'src/*.rs' 'src/**/*.rs' | sort -u)
for po in po/*.po; do
    [ -e "$po" ] || continue
    # One line per text, as the files are written, for editing by hand.
    msgmerge --quiet --update --backup=none --no-wrap "$po" po/arkha.pot
    msgattrib --no-obsolete --no-wrap --output-file="$po" "$po"
    printf '%s: ' "$po"; msgfmt --check --statistics -o /dev/null "$po"
done
