#!/bin/sh
# Checks the QoL plugin's logic map against the randomizer's own logic. Builds two seeds (one
# with shuffled cracks) on this computer, writes their map files and a made-up playthrough, then
# runs the plugin's C logic over them. Needs no game files.
set -e
cd "$(dirname "$0")/../.."
OUT=target/overlay-test
mkdir -p "$OUT"
cargo build --release -p fill-bench
cc -O2 -Wall -o "$OUT/map_test" overlay/tests/map_test.c overlay/map_logic.c
for seed in 3 11; do
  SEED=$seed SETTINGS='{"cracksanity":"AnyWorldPairs","cracks":"Open"}' ./target/release/fill-bench mapcases "$OUT/seed-$seed" </dev/null
  "$OUT/map_test" "$OUT/seed-$seed/albw-map.bin" "$OUT/seed-$seed"/case-*.txt
done
SEED=5 ./target/release/fill-bench mapcases "$OUT/plain" </dev/null
"$OUT/map_test" "$OUT/plain/albw-map.bin" "$OUT/plain"/case-*.txt
echo "Logic map test passed."
