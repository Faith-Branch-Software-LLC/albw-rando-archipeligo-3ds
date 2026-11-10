# Built-in menu art

These files are built into the app. A file with the same name in `/3ds/albw-rando/art/` on the SD
card replaces the built-in one when the app starts, so art can be tried without a rebuild.

| File | What it is | Source |
| --- | --- | --- |
| `top.png` | Top screen picture for the menus, 400x240. Text goes in its dark box (about x 38-362, y 116-226). | Made by Sebastian Neiswanger. |
| `bottom.png` | Bottom screen background, 320x240. The hint line sits in the strip at y 199-239. | Made by Sebastian Neiswanger. |
| `bar.png`, `bar-focused.png` | Menu rows, 304x34, drawn at x 8, every 37 pixels from y 8. The focused row also gets a gold glow drawn in code. | Made by Sebastian Neiswanger. |
| `card.png`, `card-focused.png` | Option cards, 304x56. The name goes in y 0-28 and the pills in y 28-48, x 8-296. | Made by Sebastian Neiswanger. |
| `line.png`, `line-focused.png` | Two-line rows in spoiler logs and credits, 304x44. | Made by Sebastian Neiswanger. |
| `model-box.png` | The box the spinning model shows in on Models and Voices, 128x106 at x 96, y 88. | Made by Sebastian Neiswanger. |
| `title.bcfnt` | Font for page names. | Hylia Serif Beta 0.009 by Omni Jacala, https://artsyomni.com/hyliaserif (free, not for profit). |
| `body.bcfnt`, `body-large.bcfnt` | Font for all other text, converted at two sizes (14 and 18 pixel glyph cells) so every size on the Display page stays sharp. | Reggae One by Fontworks, https://github.com/fontworks-fonts/Reggae, under the SIL Open Font License 1.1 (`ReggaeOne-OFL.txt`). |

The fonts were converted with devkitPro's `mkbcfnt`, at about the size they are drawn so text stays sharp. citro2d draws every font with 30 pixel glyph cells at scale 1, so the app draws each at its size in pixels over 30. The body font keeps only the characters in
`latin.txt`, because the full font also covers Japanese and would be much larger:

```
mkbcfnt -s 15 -o title.bcfnt HyliaSerifBeta-Regular.otf
mkbcfnt -s 7 -w latin.txt -o body.bcfnt ReggaeOne-Regular.ttf
mkbcfnt -s 9 -w latin.txt -o body-large.bcfnt ReggaeOne-Regular.ttf
```
