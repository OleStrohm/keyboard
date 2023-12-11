#![no_std]
#![no_main]

use bsp::{entry, hal::Timer};
use defmt::*;
use defmt_rtt as _;
use embedded_hal::timer::CountDown;
use fugit::ExtU32;
use fugit::RateExtU32;
use panic_probe as _;

use seeeduino_xiao_rp2040 as bsp;

use bsp::hal::{clocks::init_clocks_and_plls, pac, sio::Sio, usb::UsbBus, watchdog::Watchdog};
use switch_hal::{ActiveLow, OutputSwitch, Switch};
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_human_interface_device::{
    device::keyboard::NKROBootKeyboardConfig, page::Keyboard as Key, prelude::*,
};

use keyboard::Keyboard;

#[entry]
fn main() -> ! {
    run();
}

fn run() -> ! {
    info!("Program start");
    let mut pac = pac::Peripherals::take().unwrap();
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

    let timer = Timer::new(pac.TIMER, &mut pac.RESETS);

    let pins = bsp::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let i2c = bsp::hal::i2c::I2C::i2c1(
        pac.I2C1,
        pins.sda.into_mode(),
        pins.scl.into_mode(),
        400u32.kHz(),
        &mut pac.RESETS,
        125_000_000u32.Hz(),
    );

    let mut keyboard = Keyboard::new(i2c);

    let mut red = Switch::<_, ActiveLow>::new(pins.led_red.into_push_pull_output());
    red.off().unwrap();
    let mut blue = Switch::<_, ActiveLow>::new(pins.led_blue.into_push_pull_output());
    blue.off().unwrap();

    let mut green = Switch::<_, ActiveLow>::new(pins.led_green.into_push_pull_output());

    let usb_alloc = UsbBusAllocator::new(UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    let mut usb_keyboard = UsbHidClassBuilder::new()
        .add_device(NKROBootKeyboardConfig::default())
        .build(&usb_alloc);

    let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1609, 0x0001))
        .manufacturer("Ole Marius Strohm")
        .product("Rusty Keyboard")
        .serial_number("OLESTROHM")
        .build();

    info!("Firmware started!");

    // Poll input every 10 milliseconds
    let mut input_timer = timer.count_down();
    input_timer.start(10.millis());

    // Tick usb connection every 1 millisecond
    let mut tick_timer = timer.count_down();
    tick_timer.start(1.millis());

    loop {
        if input_timer.wait().is_ok() {
            let keys = keyboard.pressed_keys();

            if keys.iter().any(|&k| k != Key::NoEventIndicated) {
                green.on().unwrap();
            } else {
                green.off().unwrap();
            }

            match usb_keyboard.device().write_report(keys) {
                Ok(_) | Err(UsbHidError::WouldBlock | UsbHidError::Duplicate) => {}
                Err(e) => core::panic!("Failed to write keyboard report: {e:?}"),
            }
        }

        if tick_timer.wait().is_ok() {
            match usb_keyboard.tick() {
                Ok(_) | Err(UsbHidError::WouldBlock) => {}
                Err(e) => core::panic!("tick error: {e:?}"),
            }
        }

        if usb_dev.poll(&mut [&mut usb_keyboard]) {
            match usb_keyboard.device().read_report() {
                Ok(_) => info!("update leds"),
                Err(UsbError::WouldBlock) => {}
                Err(e) => core::panic!("Failed to read keyboard input: {e:?}"),
            }
        }
    }
}
