#!/bin/sh
# Builds the 3DS app and CIA on this computer and adds them to a GitHub release.
#
# The menu sounds (friedspin's Cozy UI SFX) may ship inside the app but not as loose files, so they
# stay out of git and GitHub Actions cannot build them in. Release builds come from here instead.
#
# 1. Commit, tag and push: `git tag v0.2.14 && git push origin v0.2.14`.
# 2. Wait for the Release workflow to finish. It makes a draft release with the apworld and QR code.
# 3. Run `sh tools/release-3ds.sh` (or `sh tools/release-3ds.sh v0.2.14`) on the tagged commit.
#
# Needs devkitPro, cargo-3ds, makerom and bannertool (see build-cia.sh), and gh signed in.
set -e

cd "$(dirname "$0")/.."
export DEVKITPRO=${DEVKITPRO:-/opt/devkitpro}
export DEVKITARM=${DEVKITARM:-$DEVKITPRO/devkitARM}
export PATH="$HOME/.cargo/bin:$DEVKITPRO/tools/bin:$DEVKITARM/bin:$PATH"

VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' crates/app-3ds/Cargo.toml | head -n 1)
TAG=${1:-v$VERSION}
if [ "$TAG" != "v$VERSION" ]; then
  echo "The tag $TAG does not match the app version $VERSION in crates/app-3ds/Cargo.toml." >&2
  exit 1
fi

# The release's source code is the tagged commit, so the build must come from exactly that.
if ! git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
  echo "There is no tag $TAG. Tag and push it first." >&2
  exit 1
fi
if [ "$(git rev-parse HEAD)" != "$(git rev-parse "$TAG^{commit}")" ]; then
  echo "HEAD is not the commit tagged $TAG. Check it out first." >&2
  exit 1
fi
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "There are uncommitted changes. Commit or stash them first." >&2
  exit 1
fi

for sound in back change_hover choose page_change select start_randomizing; do
  if [ ! -f "crates/app-3ds/sounds/$sound.wav" ]; then
    echo "crates/app-3ds/sounds/$sound.wav is missing, so the build would have no sounds." >&2
    exit 1
  fi
done

OUT=target/release-3ds
rm -rf "$OUT"
mkdir -p "$OUT"

# The apworld comes from the release the workflow made, so the zip matches what hosts download.
if ! gh release download "$TAG" -p albw_3ds.apworld -p apworld-version.txt -D "$OUT"; then
  echo "Could not download the apworld from release $TAG. Wait for the Release workflow to finish." >&2
  exit 1
fi

# Tagged builds link the albw.local page to this release's apworld.
ALBW_RELEASE_TAG=$TAG cargo 3ds build --release -p app-3ds
sh tools/build-cia.sh

cp target/armv6k-nintendo-3ds/release/albw-rando.3dsx target/cia/albw-rando.cia "$OUT/"
mkdir -p "$OUT/sd/3ds/albw-rando/apworld" "$OUT/sd/3ds/albw-rando/ap"
cp "$OUT/albw-rando.3dsx" "$OUT/sd/3ds/"
cp "$OUT/albw_3ds.apworld" "$OUT/sd/3ds/albw-rando/apworld/"
cp "$OUT/apworld-version.txt" "$OUT/sd/3ds/albw-rando/apworld/version.txt"
touch "$OUT/sd/3ds/albw-rando/ap/.keep"
(cd "$OUT/sd" && zip -qr ../albw-rando-sd.zip .)

gh release upload "$TAG" --clobber \
  "$OUT/albw-rando.cia" "$OUT/albw-rando.3dsx" "$OUT/albw-rando-sd.zip"
gh release edit "$TAG" --draft=false --latest
echo "Published release $TAG with albw-rando.cia, albw-rando.3dsx and albw-rando-sd.zip."
