//! Blinks the LED on a Pico board
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
#![no_std]
#![no_main]

use bsp::entry;
use defmt::*;
use defmt_rtt as _;
use embedded_hal::digital::v2::OutputPin;
use panic_probe as _;

// Provide an alias for our BSP so we can switch targets quickly.
// Uncomment the BSP you included in Cargo.toml, the rest of the code does not need to change.
use rp_pico as bsp;
// use sparkfun_pro_micro_rp2040 as bsp;

use bsp::hal::{
    clocks::{init_clocks_and_plls, Clock},
    pac,
    sio::Sio,
    watchdog::Watchdog,
};

const DEBUG: bool = true;

const MAXSPEED: u32 = 40000;
const MINSPEED: u32 = 10000;
const HOMINGSPEED: u32 = 2000;
const SLOWHOMESPEED: u32 = 500;
const ACCELERATION: u32 = 100000;
const Y_HOME_POS: u32 = 300;
const X_HOME_POS: u32 = 580;
const MAX_Y: u32 = 300;
const MAX_X: u32 = 300;
const TRANSFER_SIZE: u32 = 51;
const PROJECTOR_ID: u32 = 1;
const ALL_PROJECTORS: u32 = 0xF;
const BAUDRATE: u32 = 9600;

// Definied these by myself for the storage offset
const PICO_FLASH_SIZE_BYTES: u32 = 2 * 1024 * 1024;
const FLASH_SECTOR_SIZE: u32 = 4 * 1024;
const STORAGE_OFFSET: u32 = PICO_FLASH_SIZE_BYTES - (FLASH_SECTOR_SIZE * 2);
const LOAD_CONFIG: usize = 0;
const ACCELERATION_CONFIG: usize = 1;
const TRANSFER_SIZE_CONFIG: usize = 2;
const MAX_SPEED_CONFIG: usize = 3;
const MIN_SPEED_CONFIG: usize = 4;
const X_HOME_CONFIG: usize = 5;
const Y_HOME_CONFIG: usize = 6;
const PROJECTOR_ID_CONFIG: usize = 7;

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
const CONFIG_MASK: u32 = 0x00020000;
const CONFIG_SHIFT: u32 = 17;
const BOUNDARY_MASK: u32 = 0x00010000;
const BOUNDARY_SHIFT: u32 = 16;
const ONESHOT_MASK: u32 = 0x00008000;
const ONESHOT_SHIFT: u32 = 15;
const SPEED_PROFILE_MASK: u32 = 0x00007000;
const SPEED_PROFILE_SHIFT: u32 = 12;
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

const ACCELERATION_MASK: u32 = 0xFFFFF000;
const ACCELERATION_SHIFT: u32 = 12;
const TRANSFER_SIZE_MASK: u32 = 0x00000FFE;
const TRANSFER_SIZE_SHIFT: u32 = 1;
const MAX_SPEED_MASK: u32 = 0xFFFFC000;
const MAX_SPEED_SHIFT: u32 = 14;
const MIN_SPEED_MASK: u32 = 0x00003FFE;
const MIN_SPEED_SHIFT: u32 = 1;
const X_HOME_MASK: u32 = 0xFFF00000;
const X_HOME_SHIFT: u32 = 20;
const Y_HOME_MASK: u32 = 0x000FFF00;
const Y_HOME_SHIFT: u32 = 8;
const PROJECTOR_ID_MASK: u32 = 0x000000F0;
const PROJECTOR_ID_SHIFT: u32 = 4;

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

const NUM_PROFILES: u32 = 7;
const SPEED_0: u32 = 500;
const SPEED_1: u32 = 1000;
const SPEED_2: u32 = 2000;
const SPEED_3: u32 = 2500;
const SPEED_4: u32 = 5000;
const SPEED_5: u32 = 10000;
const SPEED_6: u32 = 15000;

// This was custom
const XIP_BASE: u32 = 0x10000000;
static mut config_saved: [u32; (STORAGE_OFFSET + XIP_BASE) as usize] =
    [0; (STORAGE_OFFSET + XIP_BASE) as usize];


// PicoStepper devices[2];
static mut devices: [PicoStepper; 2] = [PicoStepper::new(); 2];
// int positions[2];
static mut positions: [i32; 2] = [0; 2];

// PicoStepper YAxis;
static mut YAxis: PicoStepper = PicoStepper::new();
// PicoStepper XAxis;
static mut XAxis: PicoStepper = PicoStepper::new();

// volatile uint8_t dma_chan;
static mut dma_chan: u8 = 0;
// volatile PIO pio;
static mut pio: PIO = PIO::new();
// volatile uint8_t sm;
static mut sm: u8 = 0;

// volatile bool xfrReceived = false;
static mut xfrReceived: bool = false;
// volatile uint8_t buffer_id = 0;
static mut buffer_id: u8 = 0;

// uint32_t *projector_buffer;
static mut projector_buffer: [u32; TRANSFER_SIZE as usize] = [0; TRANSFER_SIZE as usize];
// uint32_t *buffer_one;
static mut buffer_one: [u32; TRANSFER_SIZE as usize] = [0; TRANSFER_SIZE as usize];
// uint32_t *buffer_two;
static mut buffer_two: [u32; TRANSFER_SIZE as usize] = [0; TRANSFER_SIZE as usize];

// uint32_t config_buffer[FLASH_PAGE_SIZE/sizeof(uint32_t)];
static mut config_buffer: [u32; (FLASH_PAGE_SIZE / 4) as usize] = [0; (FLASH_PAGE_SIZE / 4) as usize];
// uint32_t *config_saved = (uint32_t *)(STORAGE_OFFSET + XIP_BASE);
// bool use_config = false;
static mut use_config: bool = false;
// volatile uint8_t speed_profile = 0;
static mut speed_profile: u8 = 0;

// volatile bool receiving;
static mut receiving: bool = false;

#[entry]
fn main() -> ! {
    info!("Program start");
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let sio = Sio::new(pac.SIO);

    // External high-speed crystal on the pico board is 12Mhz
    let external_xtal_freq_hz = 12_000_000u32;
    let clocks = init_clocks_and_plls(
        external_xtal_freq_hz,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());

    let pins = bsp::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // This is the correct pin on the Raspberry Pico board. On other boards, even if they have an
    // on-board LED, it might need to be changed.
    // Notably, on the Pico W, the LED is not connected to any of the RP2040 GPIOs but to the cyw43 module instead. If you have
    // a Pico W and want to toggle a LED with a simple GPIO output pin, you can connect an external
    // LED to one of the GPIO pins, and reference that pin here.
    let mut led_pin = pins.led.into_push_pull_output();

    // Make the light flash faster and slower
    let mut speed = 100;

    loop {
        // info!("on!");
        // led_pin.set_high().unwrap();
        // delay.delay_ms(100);
        // info!("off!");
        // led_pin.set_low().unwrap();
        // delay.delay_ms(100);

        led_pin.set_high().unwrap();
        delay.delay_ms(speed);
        led_pin.set_low().unwrap();
        delay.delay_ms(speed);

        if speed > 0 {
            speed -= 1;
        } else {
            speed = 100;
        }
    }

    // // Initialize stdio and enable serial output
    // stdio_init_all();
    // sleep_ms(3000);
    // printf("Initializing...\n");
    info!("Initializing...");
    // init_config();
    init_config();

    // printf("Config loaded\n");
    info!("Config loaded");
    // init_buffers();
    init_buffers();
    // printf("Buffers initialized\n");
    info!("Buffers initialized");

    // sleep_ms(5000);

    // printf("%d\n", NUMSTEPS);
    info!("{}", NUMSTEPS);

    // Set DMA interrupt priority
    // bus_ctrl_hw->priority = BUSCTRL_BUS_PRIORITY_DMA_W_BITS | BUSCTRL_BUS_PRIORITY_DMA_R_BITS;
    bus_ctrl_hw

    // if(!DEBUG){
    //     init_steppers();
    //     printf("Steppers intialized\n");
    //     set_stepper_values();
    //     printf("Steppers configured\n");
    //     init_gpio();
    //     lasers_off();
    //     printf("GPIO initialized\n");

    //     home_steppers();
    //     printf("Steppers homed\n");
    //     set_stepper_values();
    //     printf("Steppers configured\n");
    // }

    // //set_home();

    // // Start Core 1
    // multicore_launch_core1(serialReceiver);

    // sleep_ms(5000);

    // printf("Projector %d ready...\n", config_buffer[PROJECTOR_ID_CONFIG]);

    // watchdog_enable(5000, 1);

    // // Call the draw function regularly
    // while(true){

    //     watchdog_update();

    //     if(!DEBUG){
    //         draw();
    //     }
    //     sleep_ms(100);

    // }

    // // This should never be reached
    // return -1;
}

// Load the stored config, or initialize it if needed
fn init_config() {
    // if unsafe { config_saved }[LOAD_CONFIG] != 0 && false {
    //     info!("Loading defaults");
    //     load_default_config();
    //     // write_config();
    // load_default_config();

    for idx in 0..8 {
        info!("Config at {}: {}", idx, unsafe { config_saved[idx] });
    }
}

// Load default config settings from defined values
fn load_default_config() {
    unsafe {
        config_saved[LOAD_CONFIG] = 1;
        config_saved[ACCELERATION_CONFIG] = ACCELERATION;
        config_saved[TRANSFER_SIZE_CONFIG] = TRANSFER_SIZE;
        config_saved[MAX_SPEED_CONFIG] = MAXSPEED;
        config_saved[MIN_SPEED_CONFIG] = MINSPEED;
        config_saved[X_HOME_CONFIG] = X_HOME_POS;
        config_saved[Y_HOME_CONFIG] = Y_HOME_POS;
        config_saved[PROJECTOR_ID_CONFIG] = PROJECTOR_ID;
    }
}

// Initialize buffers by allocating space as needed
fn init_buffers() {
    // Nothing needs to be done here?
    // I think Aidan is big dum
}