#!/bin/sh
# build-version.sh: Cargo.toml's version with the build, as the packages
# made from this checkout carry it (written to VERSION, which version.sh and
# tarball/build.sh read): "1.0.0-dev+r142.g6a7d287" (".dirty" with
# uncommitted changes), or just "1.0.0" for a release (the commit tagged
# v1.0.0). Without git (or its history) only Cargo.toml's version.
cd "$(dirname "$0")/.." || exit 1
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
if ! count=$(git rev-list --count HEAD 2>/dev/null) || ! commit=$(git rev-parse --short HEAD 2>/dev/null); then
    printf '%s\n' "$version"
    exit 0
fi
# The PKGBUILD's version line is always changed by makepkg: not counted.
dirty=
[ -n "$(git status --porcelain --untracked-files=no -- . ':!packaging/arch/PKGBUILD')" ] && dirty=.dirty
if [ -n "$dirty" ] || ! git describe --exact-match --tags --match "v$version" >/dev/null 2>&1; then
    version="$version+r$count.g$commit$dirty"
fi
printf '%s\n' "$version"
