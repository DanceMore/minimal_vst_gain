use nih_plug::prelude::*;

use minimal_vst_gain::Gain;

fn main() {
    nih_export_standalone::<Gain>();
}
