use atomic_float::AtomicF32;
use nih_plug::prelude::*;
use nih_plug_vizia::ViziaState;
use std::sync::Arc;

mod editor;

/// The time it takes for the peak meter to decay by 12 dB after switching to complete silence.
const PEAK_METER_DECAY_MS: f64 = 150.0;

/// This is mostly identical to the gain example, minus some fluff, and with a GUI.
pub struct Gain {
    params: Arc<GainParams>,

    /// Needed to normalize the peak meter's response based on the sample rate.
    peak_meter_decay_weight: f32,
    /// The current data for the peak meter. This is stored as an [`Arc`] so we can share it between
    /// the GUI and the audio processing parts. If you have more state to share, then it's a good
    /// idea to put all of that in a struct behind a single `Arc`.
    ///
    /// This is stored as voltage gain.
    peak_meter: Arc<AtomicF32>,
}

#[derive(Params)]
pub struct GainParams {
    /// The editor state, saved together with the parameter state so the custom scaling can be
    /// restored.
    #[persist = "editor-state"]
    editor_state: Arc<ViziaState>,

    #[id = "gain"]
    pub gain: FloatParam,

    #[id = "left_polarity"]
    pub left_polarity: BoolParam,

    #[id = "right_polarity"]
    pub right_polarity: BoolParam,

    #[id = "left_mute"]
    pub left_mute: BoolParam,

    #[id = "right_mute"]
    pub right_mute: BoolParam,
}

impl Default for Gain {
    fn default() -> Self {
        Self {
            params: Arc::new(GainParams::default()),

            peak_meter_decay_weight: 1.0,
            peak_meter: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
        }
    }
}

impl Default for GainParams {
    fn default() -> Self {
        Self {
            editor_state: editor::default_state(),

            // See the main gain example for more details
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(30.0),
                    factor: FloatRange::gain_skew_factor(-30.0, 30.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            left_mute: BoolParam::new("L", false),
            left_polarity: BoolParam::new("~", false),
            right_polarity: BoolParam::new("~", false),
            right_mute: BoolParam::new("R", false),
        }
    }
}

impl Plugin for Gain {
    const NAME: &'static str = "MinimalVST - Gain";
    const VENDOR: &'static str = "DanceMore";
    const URL: &'static str = "https://github.com/DanceMore/minimal_vst_gain";
    const EMAIL: &'static str = "dancemore@protonmail.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(
            self.params.clone(),
            self.peak_meter.clone(),
            self.params.editor_state.clone(),
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        // After `PEAK_METER_DECAY_MS` milliseconds of pure silence, the peak meter's value should
        // have dropped by 12 dB
        self.peak_meter_decay_weight = 0.25f64
            .powf((buffer_config.sample_rate as f64 * PEAK_METER_DECAY_MS / 1000.0).recip())
            as f32;

        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        for channel_samples in buffer.iter_samples() {
            let mut amplitude = 0.0;
            let num_samples = channel_samples.len();

            let gain = self.params.gain.smoothed.next();
            for sample in channel_samples {
                *sample *= gain;
                amplitude += *sample;
            }

            // To save resources, a plugin can (and probably should!) only perform expensive
            // calculations that are only displayed on the GUI while the GUI is open
            if self.params.editor_state.is_open() {
                amplitude = (amplitude / num_samples as f32).abs();
                let current_peak_meter = self.peak_meter.load(std::sync::atomic::Ordering::Relaxed);
                let new_peak_meter = if amplitude > current_peak_meter {
                    amplitude
                } else {
                    current_peak_meter * self.peak_meter_decay_weight
                        + amplitude * (1.0 - self.peak_meter_decay_weight)
                };

                self.peak_meter
                    .store(new_peak_meter, std::sync::atomic::Ordering::Relaxed)
            }
        }

        if buffer.channels() == 2 {
            for mut channel_samples in buffer.iter_samples() {
                let left_polarity = self.params.left_polarity.value();
                let right_polarity = self.params.right_polarity.value();
                let left_mute = self.params.left_mute.value();
                let right_mute = self.params.right_mute.value();

                if left_polarity || right_polarity {
                    // assume 0 is left, 1 is right
                    for (sample0, sample1) in channel_samples
                        .iter_mut()
                        .zip(channel_samples.iter_mut().skip(1))
                    {
                        // Perform phase polarity inversion
                        if left_polarity {
                            *sample0 = -(*sample0);
                        }
                        if right_polarity {
                            *sample1 = -(*sample1);
                        }
                    }
                }

                if left_mute || right_mute {
                    // assume 0 is left, 1 is right
                    for (sample0, sample1) in channel_samples
                        .iter_mut()
                        .zip(channel_samples.iter_mut().skip(1))
                    {
                        if left_mute {
                            *sample0 = 0.0;
                        }
                        if right_mute {
                            *sample1 = 0.0;
                        }
                    }
                }
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Gain {
    const CLAP_ID: &'static str = "io.github.dancemore.minimal_vst_pan";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A minimal Gain plugin");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for Gain {
    const VST3_CLASS_ID: [u8; 16] = *b"DancemorMVSTGain";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nih_export_clap!(Gain);
nih_export_vst3!(Gain);
