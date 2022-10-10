mod layout;

use anyhow::Error;
use rill_protocol::flow::core::FlowMode;
use rillrate::prime::table::{Col, Row};
use rillrate::prime::*;
use tokio::sync::mpsc::{self};
use tokio::time::{sleep, Duration};

use crate::proto_schema::schema::PicoMessage;

const FIRST_LIMIT: usize = 10;
const SECOND_LIMIT: usize = 50;

pub struct Dashboard {}

impl Dashboard {
    pub async fn init(sender: mpsc::Sender<PicoMessage>) -> Result<(), Error> {
        env_logger::try_init()?;
        rillrate::install("demo")?;
        layout::add();

        let pulse = Pulse::new(
            "messages.dashboard.all.pulse",
            Default::default(),
            PulseOpts::default().min(0).max(100).retain(1 as u32),
        );

        // let input = Input::new(
        //     "app.dashboard-1.controls.input-1",
        //     InputOpts::default().label("Input value"),
        // );
        // //let this = input.clone();
        // input.sync_callback(move |envelope| {
        //     if let Some(action) = envelope.action {
        //         log::warn!("ACTION: {:?}", action);
        //     }
        //     Ok(())
        // });

        // let wide_input = Input::new(
        //     "app.dashboard-1.controls.wide-input-1",
        //     InputOpts::default().label("Wide Input value").wide(true),
        // );
        // //let this = input.clone();
        // wide_input.sync_callback(move |envelope| {
        //     if let Some(action) = envelope.action {
        //         log::warn!("ACTION: {:?}", action);
        //     }
        //     Ok(())
        // });

        // let selector = Selector::new(
        //     "app.dashboard-1.controls.selector-1",
        //     SelectorOpts::default()
        //         .label("Select Me!")
        //         .options(["One", "Two", "Three"]),
        // );
        // let this = selector.clone();
        // selector.sync_callback(move |envelope| {
        //     if let Some(action) = envelope.action {
        //         log::warn!("ACTION: {:?}", action);
        //         this.apply(action);
        //     }
        //     Ok(())
        // });

        // === The main part ===
        // TODO: Improve that busy paths declarations...
        let counter_1 = Counter::new(
            "app.dashboard-1.counters.counter-1",
            Default::default(),
            CounterOpts::default(),
        );
        let counter_2 = Counter::new(
            "app.dashboard-1.counters.counter-2",
            Default::default(),
            CounterOpts::default(),
        );
        let counter_3 = Counter::new(
            "app.dashboard-1.counters.counter-3",
            Default::default(),
            CounterOpts::default(),
        );

        let gauge_1 = Gauge::new(
            "app.dashboard-1.gauges.gauge-1",
            Default::default(),
            GaugeOpts::default().min(0.0).max(FIRST_LIMIT as f64),
        );

        let gauge_2 = Gauge::new(
            "app.dashboard-1.gauges.gauge-2",
            Default::default(),
            GaugeOpts::default().min(0.0).max(SECOND_LIMIT as f64),
        );

        let pulse_1 = Pulse::new(
            "app.dashboard-1.pulses.pulse-1",
            Default::default(),
            PulseOpts::default(),
        );
        // let board_1 = Board::new(
        //     "app.dashboard-1.others.board-1",
        //     Default::default(),
        //     BoardOpts::default(),
        // );
        // let histogram_1 = Histogram::new(
        //     "app.dashboard-1.others.histogram-1",
        //     Default::default(),
        //     HistogramOpts::default().levels([10, 20, 100, 500]),
        // );
        // histogram_1.add(120.0);
        // histogram_1.add(11.0);

        Ok(())
    }
}
