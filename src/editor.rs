use atomic_float::AtomicF32;
use nih_plug::prelude::{util, Editor};
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::widgets::*;
use nih_plug_vizia::{assets, create_vizia_editor, ViziaState, ViziaTheming};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use crate::GainParams;

#[derive(Lens)]
struct Data {
    params: Arc<GainParams>,
    peak_meter: Arc<AtomicF32>,
}

impl Model for Data {}

// Makes sense to also define this here, makes it a bit easier to keep track of
pub(crate) fn default_state() -> Arc<ViziaState> {
    ViziaState::new(|| (200, 150))
}

static THEME: &'static str = include_str!("../assets/theme.css");
static WIDGETS: &'static str = include_str!("../assets/widgets.css");

pub(crate) fn create(
    params: Arc<GainParams>,
    peak_meter: Arc<AtomicF32>,
    editor_state: Arc<ViziaState>,
) -> Option<Box<dyn Editor>> {
    create_vizia_editor(editor_state, ViziaTheming::Builtin, move |cx, _| {
        let _ = cx.add_theme(THEME);
        let _ = cx.add_theme(WIDGETS);
        nih_plug_vizia::vizia_assets::register_roboto(cx);
        assets::register_noto_sans_light(cx);
        assets::register_noto_sans_thin(cx);

        Data {
            params: params.clone(),
            peak_meter: peak_meter.clone(),
        }
        .build(cx);

        ResizeHandle::new(cx);

        VStack::new(cx, |cx| {
            PeakMeter::new(
                cx,
                Data::peak_meter
                    .map(|peak_meter| util::gain_to_db(peak_meter.load(Ordering::Relaxed))),
                Some(Duration::from_millis(600)),
            )
            .top(Pixels(10.0));

            ParamSlider::new(cx, Data::params, |params| &params.gain).top(Pixels(10.0));

            HStack::new(cx, |cx| {
                ParamButton::new(cx, Data::params, |params| &params.left_mute);
                ParamButton::new(cx, Data::params, |params| &params.left_polarity);
                ParamButton::new(cx, Data::params, |params| &params.right_polarity);
                ParamButton::new(cx, Data::params, |params| &params.right_mute);
            });
        })
        .row_between(Pixels(0.0))
        .child_left(Stretch(1.0))
        .child_right(Stretch(1.0));
    })
}
