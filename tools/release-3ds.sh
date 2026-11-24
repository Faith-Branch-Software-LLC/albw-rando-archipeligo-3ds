#!/bin/sh
# Makes a whole release from this Mac with one command: sh tools/release-3ds.sh
#
# 1. Commit everything for the release, with the new version in crates/app-3ds/Cargo.toml.
# 2. Run this script on that commit. It:
#    - tags the commit v<version> (unless the tag is already there) and pushes the branch and tag;
#    - builds the macOS randomizer modules (Intel, and Apple Silicon by cross-compiling), the 3DS
#      app and the CIA here, while the Release workflow on GitHub builds the Linux and Windows
#      modules and the FBI QR code;
#    - waits for that workflow, running failed jobs again up to twice;
#    - packs the apworld with every module, makes the SD card zip, and publishes the release.
#
# The menu sounds (friedspin's Cozy UI SFX) may ship inside the app but not as loose files, so they
# stay out of git and the app is built here, not on GitHub.
#
# Needs devkitPro, cargo-3ds, makerom and bannertool (see build-cia.sh), the Rust target
# aarch64-apple-darwin (rustup target add aarch64-apple-darwin), and gh signed in.
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
gh auth status >/dev/null 2>&1 || { echo "gh is not signed in. Run: gh auth login" >&2; exit 1; }
rustup target list --installed | grep -q '^aarch64-apple-darwin$' ||
  { echo "Run first: rustup target add aarch64-apple-darwin" >&2; exit 1; }

COMMIT=$(git rev-parse HEAD)
if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
  if [ "$(git rev-parse "$TAG^{commit}")" != "$COMMIT" ]; then
    echo "The tag $TAG is on another commit. Check that commit out, or move the tag:" >&2
    echo "  git tag -d $TAG && git push origin --delete $TAG" >&2
    exit 1
  fi
else
  git tag -a "$TAG" -m "$VERSION"
fi
# An annotated tag's commit is listed as $TAG^{}; a lightweight tag's is listed as the tag itself.
REMOTE_TAG=$(git ls-remote origin "refs/tags/$TAG^{}" | cut -f1)
[ -n "$REMOTE_TAG" ] || REMOTE_TAG=$(git ls-remote origin "refs/tags/$TAG" | cut -f1)
if [ -n "$REMOTE_TAG" ] && [ "$REMOTE_TAG" != "$COMMIT" ]; then
  echo "GitHub has $TAG on another commit. Delete it there first: git push origin --delete $TAG" >&2
  exit 1
fi
git push origin HEAD
[ -n "$REMOTE_TAG" ] || git push origin "$TAG"

OUT=target/release-3ds
rm -rf "$OUT"
mkdir -p "$OUT/modules"

# The macOS modules. Python's stable ABI needs no Python to build against.
echo "Building the macOS modules."
PYO3_NO_PYTHON=1 APWORLD_TAG=darwin-x86_64 APWORLD_TARGET=x86_64-apple-darwin sh apworld/build.sh python3
cp -R target/apworld/stage/albw_3ds/albw3dsrandomizer/lib/darwin-x86_64 "$OUT/modules/"
PYO3_NO_PYTHON=1 APWORLD_TAG=darwin-arm64 APWORLD_TARGET=aarch64-apple-darwin sh apworld/build.sh python3
cp -R target/apworld/stage/albw_3ds/albw3dsrandomizer/lib/darwin-arm64 "$OUT/modules/"

# Tagged builds link the albw.local page to this release's apworld.
echo "Building the 3DS app and CIA."
ALBW_RELEASE_TAG=$TAG cargo 3ds build --release -p app-3ds
sh tools/build-cia.sh

# The workflow run for this commit's tag. It may take a moment to appear after the push.
echo "Waiting for the Release workflow on GitHub."
RUN=
for _ in $(seq 60); do
  RUN=$(gh run list --workflow release.yml --event push --branch "$TAG" --json databaseId,headSha \
    -q "[.[] | select(.headSha == \"$COMMIT\")][0].databaseId // empty")
  [ -n "$RUN" ] && break
  sleep 10
done
[ -n "$RUN" ] || { echo "No Release workflow run for $TAG appeared on GitHub." >&2; exit 1; }
for try in 1 2 3; do
  if gh run watch "$RUN" --exit-status --interval 30 >/dev/null; then
    break
  fi
  if [ "$try" = 3 ]; then
    echo "The Release workflow failed three times. See: gh run view $RUN" >&2
    exit 1
  fi
  echo "The Release workflow failed. Running its failed jobs again."
  gh run rerun "$RUN" --failed
  sleep 20
done
gh run download "$RUN" -D "$OUT/ci"
for folder in "$OUT"/ci/module-*; do
  cp -R "$folder" "$OUT/modules/${folder##*/module-}"
done
cp "$OUT/ci/qr-code/albw-rando-qr.png" "$OUT/"

# The apworld with every system's module.
APWORLD_PACK_ONLY=1 APWORLD_PREBUILT="$OUT/modules" sh apworld/build.sh python3
for system in linux-x86_64 linux-aarch64 darwin-x86_64 darwin-arm64 win32-amd64 win32-arm64; do
  if [ ! -d "target/apworld/stage/albw_3ds/albw3dsrandomizer/lib/$system" ]; then
    echo "The apworld has no module for $system." >&2
    exit 1
  fi
done
cp target/apworld/albw_3ds.apworld "$OUT/"
cp target/apworld/version.txt "$OUT/apworld-version.txt"

cp target/armv6k-nintendo-3ds/release/albw-rando.3dsx target/cia/albw-rando.cia "$OUT/"
mkdir -p "$OUT/sd/3ds/albw-rando/apworld" "$OUT/sd/3ds/albw-rando/ap"
cp "$OUT/albw-rando.3dsx" "$OUT/sd/3ds/"
cp "$OUT/albw_3ds.apworld" "$OUT/sd/3ds/albw-rando/apworld/"
cp "$OUT/apworld-version.txt" "$OUT/sd/3ds/albw-rando/apworld/version.txt"
touch "$OUT/sd/3ds/albw-rando/ap/.keep"
(cd "$OUT/sd" && zip -qr ../albw-rando-sd.zip .)

FILES="$OUT/albw-rando.cia $OUT/albw-rando.3dsx $OUT/albw-rando-sd.zip $OUT/albw_3ds.apworld $OUT/apworld-version.txt $OUT/albw-rando-qr.png"
if gh release view "$TAG" >/dev/null 2>&1; then
  # shellcheck disable=SC2086
  gh release upload "$TAG" --clobber $FILES
  gh release edit "$TAG" --draft=false --latest
else
  REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)
  sed -e "s|REPO|$REPO|g" -e "s|TAG|$TAG|g" tools/release-notes.md > "$OUT/notes.md"
  # shellcheck disable=SC2086
  gh release create "$TAG" --title "$TAG" --notes-file "$OUT/notes.md" --latest $FILES
fi
echo "Published release $TAG."
