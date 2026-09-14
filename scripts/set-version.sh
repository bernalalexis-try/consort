#!/usr/bin/env bash
# Write a version into the seven files that carry one.
#
# Seven, and none of them reads another, which is what this exists to stop being
# a checklist somebody keeps half of. It is called by the release workflow and
# covered by set-version.test.sh, which uses the repository's own files as its
# fixtures so that reformatting one of them fails a test rather than a release.
#
#     scripts/set-version.sh 0.6.0 [root]
#
# `root` is the tree to write into and defaults to this repository. Only the
# test passes it.
set -euo pipefail

version=${1:-}
root=${2:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}

if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Usage: set-version.sh <major.minor.patch> [root]" >&2
  echo "Got: '$version'" >&2
  exit 1
fi

# sed writes, and then what it was supposed to write has to be there. A pattern
# that matches nothing exits 0 and leaves the file as it was, which is how a
# release gets built with the previous version in its filename and nothing in
# the log to say so.
rewrite() {
  local file=$1 expression=$2 expected=$3
  sed -i "$expression" "$root/$file"
  if ! grep -qF "$expected" "$root/$file"; then
    echo "$file has no line for the version to go on. Has it been reformatted?" >&2
    echo "Expected to find: $expected" >&2
    exit 1
  fi
}

# The workspace version. Line-anchored, because `rust-version` is two lines
# below it and a dependency's own version is indented inside its table.
rewrite Cargo.toml \
  "s/^version = \".*\"$/version = \"$version\"/" \
  "version = \"$version\""

rewrite app/package.json \
  "s/^  \"version\": \".*\",$/  \"version\": \"$version\",/" \
  "  \"version\": \"$version\","

rewrite app/src-tauri/tauri.conf.json \
  "s/^  \"version\": \".*\",$/  \"version\": \"$version\",/" \
  "  \"version\": \"$version\","

# makepkg overwrites this from the checkout, so the number here is only a
# placeholder. It still has to be the right one: it is what a build from a
# tarball reports.
rewrite packaging/aur/PKGBUILD \
  "s/^pkgver=.*$/pkgver=$version.r0.g0000000/" \
  "pkgver=$version.r0.g0000000"

# Not a placeholder. The tagged-release PKGBUILD builds `#tag=v$pkgver`, so
# this line is the only thing saying which commit gets packaged, and the Arch
# release workflow refuses to run when it disagrees with the tag.
rewrite packaging/arch/PKGBUILD \
  "s/^pkgver=.*$/pkgver=$version/" \
  "pkgver=$version"

# The one built-artefact filename the install instructions still spell out. The
# rest name `<version>`, because what a release page carries is fetched rather
# than found at a path.
rewrite README.md \
  "s#/rpm/Consort-[0-9][^-]*-1.x86_64.rpm#/rpm/Consort-${version}-1.x86_64.rpm#" \
  "/rpm/Consort-${version}-1.x86_64.rpm"

# Cargo.lock's four consort-* entries, which cargo would otherwise rewrite on
# the next build. ci.yml runs `cargo tree --locked`, so a lockfile still naming
# the previous version fails the release commit's own CI.
#
# `n` moves to the line after the name, which is where cargo puts the version
# and is the only line here that may move: every other `version =` belongs to a
# dependency, and a dependency list names these four without a version at all.
# Doing it here rather than with `cargo metadata` is what keeps this a file
# rewrite, needing no toolchain, no network and no warm registry cache.
members=$(grep -c '^name = "consort-' "$root/Cargo.lock")
sed -i "/^name = \"consort-/{n;s/^version = \".*\"$/version = \"$version\"/}" "$root/Cargo.lock"
written=$(awk -v want="version = \"$version\"" \
  '/^name = "consort-/ { getline; if ($0 == want) n++ } END { print n + 0 }' \
  "$root/Cargo.lock")
if [ "$written" -ne "$members" ]; then
  echo "Cargo.lock: $written of $members consort entries took the version." >&2
  exit 1
fi

echo "Wrote $version into seven files."
