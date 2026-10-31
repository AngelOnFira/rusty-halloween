//! Fog machine cues generated from the existing show data, so the 2022-2024
//! shows drive the fog machines without being re-authored.
//!
//! - Fog machine 3 follows the middle PWM light. Each time the light turns on,
//!   the fog machine stays on for at least 5 seconds, so a flickering light
//!   still makes fog. It turns off once the light is off and the 5 seconds are
//!   up.
//! - Fog machines 1 and 2 take turns firing a burst each time the laser
//!   projector turns on, which makes the beams visible. They never run at the
//!   same time, and each one rests between bursts so it can heat back up. A
//!   trigger that lands while the next machine is busy or resting is skipped.

use common::{DmxStateData, Frame};

use crate::config::Config;

use super::{MAX_LASERS, MAX_LIGHTS};

/// The fog controller in hardware.json
const FOG_DEVICE: &str = "fog-1";

/// Fog machine 3 follows this light (the middle of the 7 PWM lights)
const FOLLOW_LIGHT: &str = "light-4";

/// How long fog machine 3 stays on after the light turns on, even if the light
/// turns off sooner
const FOLLOW_MIN_ON_MS: u64 = 5_000;

/// Fog machines 1 and 2 fire when this laser projector turns on
const LASER: &str = "lp-1";

/// How long each burst from fog machines 1 and 2 lasts
const BURST_MS: u64 = 5_000;

/// How long fog machines 1 and 2 rest after a burst before they can fire again
const REST_MS: u64 = 15_000;

/// Fog controller machine commands (0-9 off, 200-255 continuous)
const FOG_OFF: DmxStateData = 0;
const FOG_ON: DmxStateData = 255;

/// Fog controller master enable (128-255 enabled)
const FOG_ENABLE: DmxStateData = 255;

/// The laser projector's ON/OFF lamp channel turns the lights off at 0 and
/// 250-255. Everything else is one of its on modes.
fn is_laser_on(value: DmxStateData) -> bool {
    (1..250).contains(&value)
}

/// Add the fog cues to a show's frames, which must be sorted by timestamp.
/// Shows that already control the fog machines are left alone.
pub fn add_fog_cues(frames: &mut Vec<Frame>, config: &Config) {
    let fog_channel = |var_name| config.get_dmx_state_var_position(FOG_DEVICE, var_name);
    let (Some(enable), Some(machine_1), Some(machine_2), Some(machine_3)) = (
        fog_channel("enable"),
        fog_channel("machine-1"),
        fog_channel("machine-2"),
        fog_channel("machine-3"),
    ) else {
        return;
    };

    let fog_channels = [enable, machine_1, machine_2, machine_3];
    if frames
        .iter()
        .flat_map(|frame| &frame.dmx)
        .any(|(channel, _)| fog_channels.contains(channel))
    {
        return;
    }

    let light = config.get_dmx_state_var_position(FOLLOW_LIGHT, "state");
    let laser = config.get_dmx_state_var_position(LASER, "state");
    let show_end = frames.last().map_or(0, |frame| frame.timestamp);

    // Enable the fog controller for the show. The zero out at the end of the
    // show disables it again
    let mut cues = vec![(0, (enable, FOG_ENABLE))];

    let burst_machines = [machine_1, machine_2];
    let mut next_machine = 0;
    let mut ready_at = [0; 2];
    let mut burst_end = 0;
    let mut laser_on = false;

    // Fog machine 3 is on while the light is on, or while it's held on waiting
    // to turn off
    let mut light_on = false;
    let mut hold_until = 0;
    let mut pending_off = None;

    for frame in frames.iter() {
        for &(channel, value) in &frame.dmx {
            if Some(channel) == light {
                let now = frame.timestamp;
                if value > 0 && !light_on {
                    match pending_off.take() {
                        // The hold ran out before the light came back on
                        Some(off) if off < now => {
                            cues.push((off, (machine_3, FOG_OFF)));
                            cues.push((now, (machine_3, FOG_ON)));
                        }
                        // Still held on, so it just stays on
                        Some(_) => {}
                        None => cues.push((now, (machine_3, FOG_ON))),
                    }
                    hold_until = now + FOLLOW_MIN_ON_MS;
                } else if value == 0 && light_on {
                    pending_off = Some(now.max(hold_until));
                }
                light_on = value > 0;
            }

            if Some(channel) == laser {
                let turned_on = is_laser_on(value) && !laser_on;
                laser_on = is_laser_on(value);

                let start = frame.timestamp;
                if turned_on && start >= burst_end && start >= ready_at[next_machine] {
                    // Bursts are cut short at the end of the show, which turns
                    // everything off anyway
                    let machine = burst_machines[next_machine];
                    cues.push((start, (machine, FOG_ON)));
                    cues.push(((start + BURST_MS).min(show_end), (machine, FOG_OFF)));

                    burst_end = start + BURST_MS;
                    ready_at[next_machine] = burst_end + REST_MS;
                    next_machine = 1 - next_machine;
                }
            }
        }
    }

    if let Some(off) = pending_off {
        cues.push((off.min(show_end), (machine_3, FOG_OFF)));
    }

    // Add each cue to the frame at its timestamp, or a new frame if there
    // isn't one
    for (timestamp, position) in cues {
        match frames.iter_mut().find(|frame| frame.timestamp == timestamp) {
            Some(frame) => frame.dmx.push(position),
            None => frames.push(Frame {
                timestamp,
                lights: vec![None; MAX_LIGHTS],
                lasers: vec![None; MAX_LASERS],
                dmx: vec![position],
            }),
        }
    }

    frames.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
}
