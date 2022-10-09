const MAX_SPEED: u32 = 60000;
const MIN_SPEED: u32 = 15000;
const HOMING_SPEED: u32 = 2000;
const SLOW_HOMING_SPEED: u32 = 500;
const ACCELERATION: u32 = 200000;
const Y_HOME_POS: u32 = 300;
const X_HOME_POS: u32 = 300;
const MAX_Y: u32 = 300;
const MAX_X: u32 = 300;
const TRANSFER_SIZE: u32 = 51;
const PROJECTOR_ID: u32 = 0;
const ALL_PROJECTORS: u32 = 0xF;

const PWM_SLICE_ONE: u32 = 5;
const PWM_SLICE_TWO: u32 = 6;
const RED_SLICE: u32 = PWM_SLICE_TWO;
// const RED_CHANNEL: u32 = PWM_CHAN_B;
const GREEN_SLICE: u32 = PWM_SLICE_TWO;
// const GREEN_CHANNEL: u32 = PWM_CHAN_A;
const BLUE_SLICE: u32 = PWM_SLICE_ONE;
// const BLUE_CHANNEL: u32 = PWM_CHAN_B;
const PWM_DEPTH: u32 = 1023;
const PWM_CLOCK_DIV: u32 = 125;
const COLOUR_DEPTH: u32 = 8;
const COLOUR_MULTIPLIER: u32 = 146;

const ID_MASK: u32 = 0xF0000000;
const ID_SHIFT: u32 = 28;
const COUNT_MASK: u32 = 0x0FF00000;
const COUNT_SHIFT: u32 = 20;
const HOME_MASK: u32 = 0x00080000;
const HOME_SHIFT: u32 = 19;
const ENABLE_MASK: u32 = 0x00040000;
const ENABLE_SHIFT: u32 = 18;
const X_MASK: u32 = 0xFF800000;
const X_SHIFT: u32 = 23;
const Y_MASK: u32 = 0x007FC000;
const Y_SHIFT: u32 = 14;
const RED_MASK: u32 = 0x00003800;
const RED_SHIFT: u32 = 11;
const GREEN_MASK: u32 = 0x00000700;
const GREEN_SHIFT: u32 = 8;
const BLUE_MASK: u32 = 0x000000E0;
const BLUE_SHIFT: u32 = 5;

const CLOCK: u32 = 1;
const DATA: u32 = 0;
const BLUE: u32 = 11;
const GREEN: u32 = 12;
const RED: u32 = 13;
const HOME_X: u32 = 14;
const HOME_Y: u32 = 15;
const ENABLE: u32 = 16;
const ENABLE_LOGIC: bool = false;
const DIR_Y: u32 = 18;
const DIR_X: u32 = 21;
const STEP_Y: u32 = 19;
const STEP_X: u32 = 20;

pub struct Pico {}

impl Pico {
    // fn init_steppers() {}
    // fn init_gpio() {}
    // fn home_steppers() {}
    // fn homing_sequence(device: PicoStepper) {}
    // fn set_stepper_values() {}
    // fn serial_receiver() {}
    // fn dma_handler() {}
    // fn checksum(message: u32) -> bool {}
    // fn retrieve(frame: u32, mask: u32, shift: u32) -> u32 {}
    // fn lasers_off() {}
    // fn set_red_pwm(pwm: u8) {}
    // fn set_green_pwm(pwm: u8) {}
    // fn set_blue_pwm(pwm: u8) {}
}
