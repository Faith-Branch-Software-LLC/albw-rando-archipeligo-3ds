//! Text sizes the player can change from the Display page. They are saved to the SD card as a few
//! `name value` lines, so they are easy to read and edit by hand.

/// Text sizes in pixels, as the height of a font's glyph cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Display {
    /// Rows, hints, and the top screen's text.
    pub text: u8,
    /// Page names.
    pub title: u8,
}

/// The sizes on offer. The fonts are converted at the largest, so smaller sizes stay sharp.
pub const TEXT_SIZES: std::ops::RangeInclusive<u8> = 12..=18;
pub const TITLE_SIZES: std::ops::RangeInclusive<u8> = 18..=26;

/// Which size a Display row changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Text,
    Title,
}

impl Default for Display {
    fn default() -> Self {
        Self { text: 14, title: 26 }
    }
}

impl Display {
    /// Reads saved sizes. Unknown lines are skipped, and out-of-range sizes are clamped.
    pub fn parse(saved: &str) -> Self {
        let mut display = Self::default();
        for line in saved.lines() {
            let mut parts = line.split_whitespace();
            let (Some(name), Some(Ok(value))) = (parts.next(), parts.next().map(str::parse::<u8>)) else { continue };
            match name {
                "text" => display.text = value.clamp(*TEXT_SIZES.start(), *TEXT_SIZES.end()),
                "title" => display.title = value.clamp(*TITLE_SIZES.start(), *TITLE_SIZES.end()),
                _ => {}
            }
        }
        display
    }

    pub fn save(&self) -> String {
        format!("text {}\ntitle {}\n", self.text, self.title)
    }

    pub fn get(&self, setting: Setting) -> u8 {
        match setting {
            Setting::Text => self.text,
            Setting::Title => self.title,
        }
    }

    /// Changes a size by `delta` pixels, staying in range.
    pub fn step(&mut self, setting: Setting, delta: i32) {
        let (value, range) = match setting {
            Setting::Text => (&mut self.text, TEXT_SIZES),
            Setting::Title => (&mut self.title, TITLE_SIZES),
        };
        *value = (i32::from(*value) + delta).clamp(i32::from(*range.start()), i32::from(*range.end())) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_reads_back() {
        let display = Display { text: 16, title: 20 };
        assert_eq!(Display::parse(&display.save()), display);
    }

    #[test]
    fn bad_lines_keep_defaults_and_sizes_stay_in_range() {
        assert_eq!(Display::parse("text big\nother 3\n"), Display::default());
        assert_eq!(Display::parse("text 99\ntitle 1\n"), Display { text: 18, title: 18 });
    }

    #[test]
    fn steps_stop_at_the_ends() {
        let mut display = Display::default();
        display.step(Setting::Text, 100);
        assert_eq!(display.text, 18);
        display.step(Setting::Title, -100);
        assert_eq!(display.title, 18);
    }
}
