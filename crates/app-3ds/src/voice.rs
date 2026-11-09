//! Plays samples of the chosen voice when the player presses Y on the Models and Voices page.
//!
//! Clips come from the voice pack's sound archive, or the game's own for the original voice. They
//! are read the first time a voice is played and kept until another voice is chosen.

use ctru::linear::LinearAllocator;
use ctru::services::ndsp::wave::Wave;
use ctru::services::ndsp::{AudioFormat, AudioMix, InterpolationType, Ndsp, OutputMode};
use ui_core::csar::{Clip, VOICE_CLIPS};

use crate::job;

type Samples = Vec<u8, LinearAllocator>;

/// Turns a clip into a sound buffer the DSP can read.
fn samples(clip: &Clip) -> Samples {
    let mut buffer = Vec::with_capacity_in(clip.samples.len() * 2, LinearAllocator);
    buffer.extend(clip.samples.iter().flat_map(|sample| sample.to_le_bytes()));
    buffer
}

pub struct Voices {
    // The wave is declared before the DSP service, so it is dropped first.
    wave: Option<Wave<Samples>>,
    ndsp: Option<Ndsp>,
    /// The voice the clips belong to, with `None` for the game's own.
    loaded: Option<(Option<String>, Vec<Clip>)>,
    next: usize,
}

impl Voices {
    pub fn new() -> Self {
        Self { wave: None, ndsp: None, loaded: None, next: 0 }
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
        let mut wave = Wave::new(samples(clip), AudioFormat::PCM16Mono, false);
        let ndsp = self.ndsp.as_ref().expect("started above");
        let mut channel = ndsp.channel(0).map_err(|err| err.to_string())?;
        channel.clear_queue();
        channel.set_interpolation(InterpolationType::Linear);
        channel.set_sample_rate(clip.sample_rate as f32);
        channel.set_format(AudioFormat::PCM16Mono);
        channel.set_mix(&AudioMix::default());
        channel.queue_wave(&mut wave).map_err(|err| err.to_string())?;
        drop(channel);
        self.wave = Some(wave);
        Ok(())
    }
}
