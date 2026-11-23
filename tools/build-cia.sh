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

# The CIA's version comes from the app's Cargo.toml, so the two always match. The title version
# holds 6 bits of major, 6 of minor and 4 of micro, so the patch number must stay at 15 or below:
# past that, raise the minor number and start the patch again at 0.
VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' crates/app-3ds/Cargo.toml | head -n 1)
MAJOR=${VERSION%%.*}
REST=${VERSION#*.}
MINOR=${REST%%.*}
MICRO=${REST#*.}
if [ "$MAJOR" -gt 63 ] || [ "$MINOR" -gt 63 ] || [ "$MICRO" -gt 15 ]; then
  echo "Version $VERSION does not fit a CIA. Keep the patch number at 15 or below: raise the minor number and reset the patch to 0." >&2
  exit 1
fi

# The HOME Menu sound is the app's start sound (friedspin's Cozy UI SFX, which may ship inside the
# app but not as a loose file, so it stays out of git). Without it, the banner is silent.
BANNER_SOUND=crates/app-3ds/sounds/start_randomizing.wav
[ -f "$BANNER_SOUND" ] || BANNER_SOUND="$CIA/banner.wav"
# The HOME Menu plays a mono banner sound as if it were stereo, twice as fast and high pitched, so
# the sound goes in as plain 16-bit stereo at 22050 Hz. macOS's afconvert changes the rate where it
# is there, and Python copies the one channel to both.
if command -v afconvert >/dev/null 2>&1; then
  afconvert -f WAVE -d LEI16@22050 "$BANNER_SOUND" "$OUT/banner-rate.wav"
  BANNER_SOUND="$OUT/banner-rate.wav"
fi
python3 - "$BANNER_SOUND" "$OUT/banner-sound.wav" <<'PY'
import sys, wave
with wave.open(sys.argv[1]) as source:
    channels, width, rate = source.getnchannels(), source.getsampwidth(), source.getframerate()
    frames = source.readframes(source.getnframes())
if width != 2:
    sys.exit("The banner sound must be 16-bit.")
if channels == 1:
    frames = b"".join(frames[i:i + 2] * 2 for i in range(0, len(frames), 2))
elif channels != 2:
    sys.exit("The banner sound must be mono or stereo.")
with wave.open(sys.argv[2], "wb") as out:
    out.setnchannels(2)
    out.setsampwidth(2)
    out.setframerate(rate)
    out.writeframes(frames)
PY
BANNER_SOUND="$OUT/banner-sound.wav"
"$BANNERTOOL" makebanner -i "$CIA/banner.png" -a "$BANNER_SOUND" -o "$OUT/banner.bnr"
"$MAKEROM" -f cia -o "$OUT/albw-rando.cia" -target t -exefslogo \
  -elf "$BUILT/albw-rando.elf" -rsf "$CIA/app.rsf" \
  -icon "$BUILT/albw-rando.smdh" -banner "$OUT/banner.bnr" \
  -DAPP_TITLE=ALBWRando -DAPP_PRODUCT_CODE=CTR-H-ALBW -DAPP_UNIQUE_ID=0xFA1B0 \
  -major "$MAJOR" -minor "$MINOR" -micro "$MICRO"
# makerom v0.19.0 writes 0 for the minor number whatever it is given, which left every CIA at
# 0.0.<patch>. So the title version goes into the ticket and the TMD here.
python3 - "$OUT/albw-rando.cia" "$MAJOR" "$MINOR" "$MICRO" <<'PY'
import struct, sys
path, major, minor, micro = sys.argv[1], *map(int, sys.argv[2:])
version = struct.pack(">H", major << 10 | minor << 4 | micro)
data = bytearray(open(path, "rb").read())
header, _, _, certs, ticket = struct.unpack_from("<IHHII", data)
align = lambda size: (size + 63) // 64 * 64
# Both are signed with RSA-2048 (type 0x10004): 4 bytes of type, 0x100 of signature, 0x3C of padding.
body = 4 + 0x100 + 0x3C
ticket_start = align(header) + align(certs)
for start, field in ((ticket_start, 0xA6), (ticket_start + align(ticket), 0x9C)):
    if struct.unpack_from(">I", data, start)[0] != 0x10004:
        sys.exit("The CIA's ticket or TMD is not signed the way this script expects.")
    data[start + body + field:start + body + field + 2] = version
open(path, "wb").write(data)
PY
echo "Built $OUT/albw-rando.cia, version $VERSION"
