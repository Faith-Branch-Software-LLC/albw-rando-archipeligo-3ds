#!/bin/sh
# Packs the built 3DS app as a CIA, for installing to the HOME Menu with FBI or similar.
# Run `cargo 3ds build --release -p app-3ds` first. Needs makerom (github.com/3DSGuy/Project_CTR)
# and bannertool (github.com/Epicpkmn11/bannertool) in tools/bin (gitignored), on PATH, or set
# MAKEROM and BANNERTOOL.
set -e

cd "$(dirname "$0")/.."
PATH="$PWD/tools/bin:$PATH"
MAKEROM=${MAKEROM:-makerom}
BANNERTOOL=${BANNERTOOL:-bannertool}
BUILT=target/armv6k-nintendo-3ds/release
CIA=crates/app-3ds/cia
OUT=target/cia
mkdir -p "$OUT"

"$BANNERTOOL" makebanner -i "$CIA/banner.png" -a "$CIA/banner.wav" -o "$OUT/banner.bnr"
"$MAKEROM" -f cia -o "$OUT/albw-rando.cia" -target t -exefslogo \
  -elf "$BUILT/albw-rando.elf" -rsf "$CIA/app.rsf" \
  -icon "$BUILT/albw-rando.smdh" -banner "$OUT/banner.bnr" \
  -DAPP_TITLE=ALBWRando -DAPP_PRODUCT_CODE=CTR-H-ALBW -DAPP_UNIQUE_ID=0xFA1B0 \
  -major 0 -minor 1 -micro 0
echo "Built $OUT/albw-rando.cia"
