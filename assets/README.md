# Art sources

Working files for the app's art, so they can be edited on any computer. The finished images the
app builds in live elsewhere:

| Source | Exported to |
| --- | --- |
| `wips/top-screen-background.af` | `crates/app-3ds/assets/top.png` (400x240) |
| `wips/bottom-screen-background.af` | `crates/app-3ds/assets/bottom.png` (320x240) |
| `wips/cards.af` | `crates/app-3ds/assets/` bar, card, card-tall, line (each with a `-focused` version) and `model-box.png` |
| `wips/icon.af` | `crates/app-3ds/icon.png` (48x48) |
| `wips/homescreen-backdrop.af` | `crates/app-3ds/cia/banner.png` (256x128, the HOME Menu banner) |

See `crates/app-3ds/assets/README.md` for every image's name and size.

The menu sounds are not here. Their license (friedspin's Cozy UI SFX) allows them inside the built
app but forbids sharing the raw files, so they stay out of the repository. Copy them into
`crates/app-3ds/sounds/` on each computer by hand.
