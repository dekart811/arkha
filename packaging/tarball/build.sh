#!/bin/sh
# build.sh: builds the prebuilt program for a release page, which the AUR's
# arkha-bin installs: target/tarball/arkha-<version>-<arch>.tar.gz with a
# folder arkha-<version>-<arch>/ holding arkha (it runs from there too, with
# its translations), arkha.desktop, arkha.svg, the AppStream metainfo
# (io.github.dekart811.Arkha.metainfo.xml), locale/, manual/, LICENSE and
# README.md. <version> is Cargo.toml's. Needs what building Arkha needs (see
# BUILDING.md).
set -e
cd "$(dirname "$0")/../.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
arch=$(uname -m)
cargo build --release --locked

name=arkha-$version-$arch
out=target/tarball
rm -rf "$out"
mkdir -p "$out/$name"
install -m755 target/release/arkha "$out/$name/arkha"
install -m644 arkha.desktop LICENSE README.md "$out/$name/"
install -m644 data/arkha.svg "$out/$name/arkha.svg"
install -m644 data/io.github.dekart811.Arkha.metainfo.xml "$out/$name/"
# The translations build.rs made from po/*.po.
for mo in target/release/locale/*/LC_MESSAGES/arkha.mo; do
    [ -e "$mo" ] && install -Dm644 "$mo" "$out/$name/${mo#target/release/}"
done
# Never a tarball without them (as the first 1.0.0 one was): what's
# there instead, to see why.
if [ ! -d "$out/$name/locale" ]; then
    echo "build.sh: no translations in target/release/locale" >&2
    echo "msgfmt: $(command -v msgfmt || echo none) $(msgfmt --version 2>/dev/null | head -n1)" >&2
    ls -laR target/release/locale >&2 || true
    for f in target/release/build/arkha-*/out/languages.rs; do echo "$f:" >&2; cat "$f" >&2; done
    exit 1
fi
# The manual's pages build.rs made from docs/manual (Arkha finds them
# next to it, as the translations).
cp -r target/release/manual "$out/$name/manual"
[ -f "$out/$name/manual/en/index.html" ] || { echo "build.sh: no manual in target/release/manual" >&2; exit 1; }
chmod -R u=rwX,go=rX "$out/$name/manual"
# The same archive for the same files: sorted, with plain owners and times.
tar -C "$out" --sort=name --owner=0 --group=0 --numeric-owner \
    --mtime="@$(git log -1 --format=%ct 2>/dev/null || date +%s)" \
    -czf "$out/$name.tar.gz" "$name"
ls "$out/$name.tar.gz"
