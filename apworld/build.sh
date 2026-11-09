#!/bin/sh
# Builds the randomizer as a Python module for this computer and packs albw_3ds.apworld.
#
# Usage: apworld/build.sh [python]
#
# The module uses Python's stable ABI, so one build works on Python 3.11 and newer. It goes in
# albw3dsrandomizer/lib/<system>-<machine>/ inside the apworld. Modules built on other systems,
# such as by CI, can be put in apworld/prebuilt/<system>-<machine>/ and are packed too. The
# result is target/apworld/albw_3ds.apworld.
#
# With APWORLD_PACK_ONLY=1 it builds nothing and packs only the prebuilt modules, which is how the
# release workflow joins the modules each system built.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
python=${1:-python3}
tag=$("$python" -c 'import platform, sys; print(f"{sys.platform}-{platform.machine().lower()}")')
stage="$root/target/apworld/stage"
cargo_dir=${CARGO_TARGET_DIR:-"$root/target/apworld/cargo"}

case "$(uname -s)" in
    Darwin)
        # Python extension modules leave Python's symbols for the interpreter to provide.
        export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-undefined -C link-arg=dynamic_lookup"
        built=libalbw3dsrandomizer.dylib; module=albw3dsrandomizer.abi3.so ;;
    MINGW*|MSYS*|CYGWIN*) built=albw3dsrandomizer.dll; module=albw3dsrandomizer.pyd ;;
    *) built=libalbw3dsrandomizer.so; module=albw3dsrandomizer.abi3.so ;;
esac

if [ "${APWORLD_PACK_ONLY:-0}" != 1 ]; then
    PYO3_PYTHON="$python" cargo build --release --lib --manifest-path "$root/vendor/z17-randomizer/Cargo.toml" \
        --target-dir "$cargo_dir"
fi

rm -rf "$stage"
mkdir -p "$stage"
cp -R "$root/apworld/albw_3ds" "$stage/albw_3ds"
find "$stage" -name "__pycache__" -type d -prune -exec rm -rf {} +
if [ "${APWORLD_PACK_ONLY:-0}" != 1 ]; then
    mkdir -p "$stage/albw_3ds/albw3dsrandomizer/lib/$tag"
    cp "$cargo_dir/release/$built" "$stage/albw_3ds/albw3dsrandomizer/lib/$tag/$module"
fi
if [ -d "$root/apworld/prebuilt" ]; then
    cp -R "$root/apworld/prebuilt/." "$stage/albw_3ds/albw3dsrandomizer/lib/"
fi

mkdir -p "$root/target/apworld"
rm -f "$root/target/apworld/albw_3ds.apworld"
(cd "$stage" && "$python" -m zipfile -c "$root/target/apworld/albw_3ds.apworld" albw_3ds)
"$python" -c 'import json, sys; print(json.load(open(sys.argv[1]))["world_version"])' \
    "$root/apworld/albw_3ds/archipelago.json" > "$root/target/apworld/version.txt"
echo "Packed $root/target/apworld/albw_3ds.apworld with: $(ls "$stage/albw_3ds/albw3dsrandomizer/lib" | tr '\n' ' ')"
