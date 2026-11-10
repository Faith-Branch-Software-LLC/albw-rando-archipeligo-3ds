# ALBW Randomizer 3DS

A 3DS homebrew app that randomizes The Legend of Zelda: A Link Between Worlds on the console
itself, and patches the game for Archipelago multiworlds. No computer is needed to make a seed.

Made by Faith Branch Software LLC. It runs rickfay's
[z17-randomizer](https://github.com/rickfay/z17-randomizer), from randomsalience's
[Archipelago fork](https://github.com/randomsalience/albw-archipelago).

## What you need

- A 3DS, 2DS, New 3DS or New 2DS with [Luma3DS](https://github.com/LumaTeam/Luma3DS) and
  "Enable game patching" turned on in the Luma configuration menu.
- The North American (USA) A Link Between Worlds, as a cartridge or installed from the eShop. The
  app reads the game from the console and ships no game data.
- Old 3DS models work, but a seed takes much longer to make. Keep the console plugged in.

## Install

1. Download `albw-rando-sd.zip` from the [latest release](../../releases/latest).
2. Extract it to the root of your SD card. It adds `/3ds/albw-rando.3dsx` and the
   `/3ds/albw-rando/` folder.
3. Open the app from the Homebrew Launcher.

## Make a seed

Pick a preset or change the Rules, then choose Make a Seed. There you can type a seed number or
pick a new random one, then choose Start Randomizing. The app installs the seed as a Luma game
patch, so the next time you start the game it is randomized. Spoiler logs are saved in
`/3ds/albw-rando/spoilers/` and can be read in the app under Spoiler Logs.

## Archipelago

The app uses its own apworld, "A Link Between Worlds - 3DS", based on randomsalience's
albw-archipelago with new options. Patches for the original apworld (`.apalbw`) do not work here.

1. In the app, open Archipelago. Set your Player Name and your Options.
2. Choose Share on Network, then open `http://albw.local` (or the address shown) on a computer on
   the same Wi-Fi.
3. Download your options file and send it to the person hosting the multiworld. The host also
   needs the apworld, which that page offers too; it goes in `Archipelago/custom_worlds/`.
4. When the host generates, drop your `.apalbw3ds` patch onto the same page, or copy it to
   `/3ds/albw-rando/ap/`.
5. In the app, open Patch Files, pick the patch and choose Patch the Game.
6. Connect to the multiworld from a computer with the ALBW Archipelago client and the 3DS plugin,
   as described in the apworld's setup guide. Any Archipelago server works, including private
   ones.

## Building

Needs devkitPro (`3ds-dev`), Rust from `rustup`, and `cargo-3ds`. The toolchain is pinned in
`rust-toolchain.toml`.

```
cargo 3ds build --release -p app-3ds
sh apworld/build.sh python3
```

The release workflow in `.github/workflows/release.yml` builds the app and an apworld for
Windows, macOS and Linux when a `v*` tag is pushed.

## Credits and license

See About and Credits in the app, and `crates/app-3ds/assets/README.md` for the art and fonts.
This app is not made or approved by Nintendo.

Licensed under the GNU General Public License, version 2 or later (see `LICENSE`), like the
randomizer and the apworld it builds on.
