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

### With FBI

In FBI, choose Remote Install, then Scan QR Code, and scan this code. It installs the latest
release's `albw-rando.cia` to the HOME Menu.

![QR code for the latest albw-rando.cia](https://github.com/Faith-Branch-Software-LLC/albw-rando-archipeligo-3ds/releases/latest/download/albw-rando-qr.png)

### By hand

1. Download `albw-rando-sd.zip` from the [latest release](../../releases/latest).
2. Extract it to the root of your SD card. It adds `/3ds/albw-rando.3dsx` and the
   `/3ds/albw-rando/` folder.
3. Open the app from the Homebrew Launcher.

## Make a seed

Pick a preset or change the Rules, then choose Make a Seed. There you can type a seed number or
pick a new random one, then choose Start Randomizing. The app installs the seed as a Luma game
patch, so the next time you start the game it is randomized. Spoiler logs are saved in
`/3ds/albw-rando/spoilers/` and can be read in the app under Spoiler Logs.

Seeds are kept in `/3ds/albw-rando/seeds/`. The active one is moved to
`/luma/titles/00040000000EC300/`. Under Your Seeds, pick another seed or play unrandomized. A
mod that was already in that Luma folder moves to the seeds folder, so you can switch back to it.
If ModMoon is installed, the app asks once whether ModMoon or the app should switch seeds, and
App Settings can change that later. With ModMoon, each seed goes into a new ModMoon slot.
All seeds share the game's save files, so start a new file for a new seed.

Give a seed a name under Seed Name, and Your Seeds and ModMoon show that name.

## Speed and QoL

The Speed and QoL page sets up a plugin that runs beside the game. Start the game from that page
to use it, or turn on **Load Every Time** so Luma loads it whenever the game starts, from the HOME
Menu or ModMoon too (Luma's plugin loader must stay on, in the Rosalina menu). With Load Every
Time on, the plugin does nothing in games that are not this app's seeds or Archipelago patches. A
plugin already in Luma's folder for the game is kept in `/3ds/albw-rando/other-plugin.3gx` and put
back when Load Every Time is turned off. In Archipelago games the plugin also connects the game to
the Archipelago client (see below).

- **Speedrun Timer** shows in the top left corner while a save is being played. Time stops
  whenever the game stops showing new frames, so loading screens are left out.
- **ZL Button** and **ZR Button** can open the logic map or pause and start the timer. On an Old
  3DS, hold SELECT and press L for ZL or R for ZR.
  Holding the timer button for two seconds resets it.
- **Logic Map** draws over the bottom screen, on the game's own map: green checks are in logic,
  yellow ones need glitches beyond the seed's logic, red ones need more items, grey ones are
  done. Tap a marker to list its checks by name; tap anywhere else to hide the list. Zoom In
  shows a quarter of the world, and while zoomed, tapping the map moves the view. Press the
  button again to close the map, or hold it to see the other world. The top screen lists each
  dungeon's checks. With shuffled cracks, each crack you have gone through shows a line to where
  it leads. The game keeps running while the map is open, and taps also reach the game. Seeds
  made before version 0.3.2 have no map.

The map's pictures are made from your own game files when you make a seed, and stay on your SD
card in `/3ds/albw-rando/qol/`.

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
6. Start the game from the app's Speed and QoL page, or turn on Load Every Time there. The top
   screen shows a command such as `/3ds 192.168.1.20`; type it into the ALBW Archipelago client on
   a computer on the same network. The app's own plugin answers the client, so no other plugin is
   needed. Then connect to the multiworld from that client,
   as described in the apworld's setup guide. Any Archipelago server works, including private
   ones.

## Building

Needs devkitPro (`3ds-dev`), Rust from `rustup`, and `cargo-3ds`. The toolchain is pinned in
`rust-toolchain.toml`.

```
cargo 3ds build --release -p app-3ds
sh apworld/build.sh python3
```

The menu sounds are friedspin's Cozy UI SFX. Their license allows them inside the app but not as
loose files, so they are not in this repository. Put the WAV files in `crates/app-3ds/sounds/` to
build them in; without them the app has no menu sounds.

When a `v*` tag is pushed, the release workflow in `.github/workflows/release.yml` builds the
apworld for Windows, macOS and Linux and makes a draft release. Then `sh tools/release-3ds.sh`,
run on the tagged commit, builds the app and CIA with the sounds, adds them and the SD card zip,
and publishes the release.

## Credits and license

See About and Credits in the app, and `crates/app-3ds/assets/README.md` for the art and fonts.
This app is not made or approved by Nintendo.

Licensed under the GNU General Public License, version 2 or later (see `LICENSE`), like the
randomizer and the apworld it builds on.
