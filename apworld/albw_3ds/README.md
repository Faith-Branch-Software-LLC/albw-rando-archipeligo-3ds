# A Link Between Worlds - 3DS

The Archipelago world for the ALBW Randomizer 3DS app, made by Faith Branch Software LLC.

This is a fork of randomsalience's [albw-archipelago](https://github.com/randomsalience/albw-archipelago)
(v0.3.6), which is based on the standalone [z17-randomizer](https://github.com/rickfay/z17-randomizer/).
Thank you to randomsalience for the original Archipelago world, and to rickfay and the z17-randomizer
contributors for the randomizer itself.

## How this edition differs

- The game is named "A Link Between Worlds - 3DS", so it can be installed next to the original
  apworld. Players of this edition need this apworld on the computer that generates the multiworld.
- Patch files end in `.apalbw3ds`. Each player patches their own game on the 3DS with the app,
  which can also send the options file and receive the patch over the local network at `albw.local`.
- The randomizer gives the same results on 32-bit and 64-bit systems, so a patch made on a computer
  matches the patch rebuilt on a 3DS.
- Original-edition patches (`.apalbw`) do not load in this edition.

## Building

Run `apworld/build.sh` from the app's repository. It builds the randomizer as a Python module for
this computer and writes `target/apworld/albw_3ds.apworld`.

## License

GPL-2.0, the same as the original apworld. See `LICENSE`.
