//! Plays samples of the chosen voice when the player presses Y on the Models and Voices page.
//!
//! Clips come from the voice pack's sound archive, or the game's own for the original voice. They
//! are read the first time a voice is played and kept until another voice is chosen.
//!
//! The same DSP service plays the menu sounds, on their own channel.

use ctru::linear::LinearAllocator;
use ctru::services::ndsp::wave::Wave;
use ctru::services::ndsp::{AudioFormat, AudioMix, InterpolationType, Ndsp, OutputMode};
use ui_core::csar::{Clip, VOICE_CLIPS};
use ui_core::menu::Sound;

use crate::job;

type Samples = Vec<u8, LinearAllocator>;

mod built {
    include!(concat!(env!("OUT_DIR"), "/sounds.rs"));
}

/// The DSP channel menu sounds play on. Voice clips use channel 0.
const SOUND_CHANNEL: u8 = 1;

/// A menu sound's sample rate and 16-bit mono samples, if the build has it.
fn sound_data(sound: Sound) -> Option<(u32, &'static [u8])> {
    match sound {
        Sound::Hover => built::HOVER,
        Sound::Choose => built::CHOOSE,
        Sound::Select => built::SELECT,
        Sound::PageChange => built::PAGE_CHANGE,
        Sound::Back => built::BACK,
        Sound::Start => built::START,
    }
}

/// Turns a clip into a sound buffer the DSP can read.
fn samples(clip: &Clip) -> Samples {
    let mut buffer = Vec::with_capacity_in(clip.samples.len() * 2, LinearAllocator);
    buffer.extend(clip.samples.iter().flat_map(|sample| sample.to_le_bytes()));
    buffer
}

pub struct Voices {
    // The wave is declared before the DSP service, so it is dropped first.
    wave: Option<Wave<Samples>>,
    sound_wave: Option<Wave<Samples>>,
    ndsp: Option<Ndsp>,
    /// Set once the DSP fails to start for menu sounds, so it is not tried every frame.
    no_sound: bool,
    /// The voice the clips belong to, with `None` for the game's own.
    loaded: Option<(Option<String>, Vec<Clip>)>,
    next: usize,
}

impl Voices {
    pub fn new() -> Self {
        Self { wave: None, sound_wave: None, ndsp: None, no_sound: false, loaded: None, next: 0 }
    }

    fn start_sound(&mut self) -> Result<&Ndsp, String> {
        if self.ndsp.is_none() {
            let mut ndsp = Ndsp::new().map_err(|_| {
                "Sound needs the 3DS's DSP firmware. Run the DSP1 homebrew once to save it to /3ds/dspfirm.cdc.".to_owned()
            })?;
            ndsp.set_output_mode(OutputMode::Stereo);
            self.ndsp = Some(ndsp);
        }
        Ok(self.ndsp.as_ref().expect("set above"))
    }

    /// Plays a menu sound, stopping the one before it. Does nothing if the build has no such sound
    /// or the console has no DSP firmware.
    pub fn play_sound(&mut self, sound: Sound) {
        let Some((rate, data)) = sound_data(sound) else { return };
        if self.no_sound {
            return;
        }
        if let Err(err) = self.start_sound() {
            job::log(&format!("Menu sounds are off: {err}"));
            self.no_sound = true;
            return;
        }
        self.sound_wave = None;
        let mut buffer = Vec::with_capacity_in(data.len(), LinearAllocator);
        buffer.extend_from_slice(data);
        // The DSP keeps a pointer to the wave, so it goes in its final place before it is queued.
        let wave = self.sound_wave.insert(Wave::new(buffer, AudioFormat::PCM16Mono, false));
        let ndsp = self.ndsp.as_ref().expect("started above");
        let Ok(mut channel) = ndsp.channel(SOUND_CHANNEL) else { return };
        channel.clear_queue();
        channel.set_interpolation(InterpolationType::Linear);
        channel.set_sample_rate(rate as f32);
        channel.set_format(AudioFormat::PCM16Mono);
        channel.set_mix(&AudioMix::default());
        let _ = channel.queue_wave(wave);
    }

    /// Plays the next clip of `voice`, the voice pack's folder name. Returns a message for the
    /// player if nothing can play.
    pub fn play(&mut self, voice: Option<&str>) -> Result<(), String> {
        let wanted = voice.map(str::to_owned);
        if self.loaded.as_ref().map(|(name, _)| name) != Some(&wanted) {
            // Stop the old clip before its voice's clips go away.
            self.wave = None;
            let (clips, problems) = match voice {
                Some(name) => job::mod_voice_clips(name, &VOICE_CLIPS)?,
                None => job::game_voice_clips(&VOICE_CLIPS)?,
            };
            if !problems.is_empty() {
                job::log(&format!("Voice clips skipped: {problems:?}"));
            }
            job::log(&format!("Loaded {} voice clips for {}.", clips.len(), voice.unwrap_or("the game")));
            if clips.is_empty() {
                return Err("This voice has none of the clips this page plays.".to_owned());
            }
            self.loaded = Some((wanted, clips));
            self.next = 0;
        }
        self.start_sound()?;
        let clips = &self.loaded.as_ref().expect("loaded above").1;
        let clip = &clips[self.next % clips.len()];
        self.next = (self.next + 1) % clips.len();

        // Replacing the wave stops the one still playing.
        self.wave = None;
        // The DSP keeps a pointer to the wave, so it goes in its final place before it is queued.
        let wave = self.wave.insert(Wave::new(samples(clip), AudioFormat::PCM16Mono, false));
        let ndsp = self.ndsp.as_ref().expect("started above");
        let mut channel = ndsp.channel(0).map_err(|err| err.to_string())?;
        channel.clear_queue();
        channel.set_interpolation(InterpolationType::Linear);
        channel.set_sample_rate(clip.sample_rate as f32);
        channel.set_format(AudioFormat::PCM16Mono);
        channel.set_mix(&AudioMix::default());
        channel.queue_wave(wave).map_err(|err| err.to_string())?;
        Ok(())
    }
}
