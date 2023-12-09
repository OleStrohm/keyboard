#![no_std]
#![no_main]

use bsp::{
    entry,
    hal::{Clock, Timer},
};
use cortex_m::peripheral::NVIC;
use defmt::*;
use defmt_rtt as _;
use embedded_hal::timer::CountDown;
use fugit::{ExtU32, RateExtU32};
use panic_probe as _;

use port_expander::Pca9555;
use seeeduino_xiao_rp2040 as bsp;

use bsp::hal::{
    clocks::init_clocks_and_plls,
    i2c::I2C,
    pac::{self, Interrupt},
    sio::Sio,
    usb::UsbBus,
    watchdog::Watchdog,
};
use switch_hal::{ActiveLow, OutputSwitch, Switch};
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_human_interface_device::device::keyboard::NKROBootKeyboardConfig;
use usbd_human_interface_device::page::Keyboard;
use usbd_human_interface_device::prelude::*;

#[entry]
fn main() -> ! {
    run();
}

fn run() -> ! {
    info!("Program start");
    let mut pac = pac::Peripherals::take().unwrap();
    let mut core = pac::CorePeripherals::take().unwrap();
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

    let delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());
    let timer = Timer::new(pac.TIMER, &mut pac.RESETS);

    let pins = bsp::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let mut red = Switch::<_, ActiveLow>::new(pins.led_red.into_push_pull_output());
    red.off().unwrap();
    let mut blue = Switch::<_, ActiveLow>::new(pins.led_blue.into_push_pull_output());
    blue.off().unwrap();

    let mut green = Switch::<_, ActiveLow>::new(pins.led_green.into_push_pull_output());

    let i2c = I2C::i2c1(
        pac.I2C1,
        pins.sda.into_mode(),
        pins.scl.into_mode(),
        400u32.kHz(),
        &mut pac.RESETS,
        125_000_000u32.Hz(),
    );
    let mut pca = Pca9555::new(i2c, true, false, false);
    let pca_pins = pca.split();

    let sw7 = pca_pins.io0_5;
    let sw8 = pca_pins.io0_4;
    let sw9 = pca_pins.io0_3;

    let usb_alloc = UsbBusAllocator::new(UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    let mut keyboard = UsbHidClassBuilder::new()
        .add_device(NKROBootKeyboardConfig::default())
        .build(&usb_alloc);

    let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("Ole Marius Strohm")
        .product("Rusty Keyboard")
        .serial_number("OLESTROHM")
        .build();

    //unsafe {
    //    core.NVIC.set_priority(Interrupt::USBCTRL_IRQ, 1);
    //    NVIC::unmask(Interrupt::USBCTRL_IRQ);
    //}

    info!("Firmware started!");

    //let mut prev_report = KeyboardReport {
    //    modifier: 0,
    //    reserved: 0,
    //    leds: 0,
    //    keycodes: [0; 6],
    //};
    let mut input_timer = timer.count_down();
    input_timer.start(10.millis());
    let mut tick_timer = timer.count_down();
    tick_timer.start(1.millis());

    loop {
        if input_timer.wait().is_ok() {
            let mut keys = [0, 0, 0, 0, 0, 0];
            let mut pressed_keys = 0;
            if sw7.is_low().unwrap() {
                //info!("pressed SW7");
                keys[pressed_keys] = 30;
                pressed_keys += 1;
            }
            if sw8.is_low().unwrap() {
                //info!("pressed SW8");
                keys[pressed_keys] = 31;
                pressed_keys += 1;
            }
            if sw9.is_low().unwrap() {
                //info!("pressed SW9");
                keys[pressed_keys] = 32;
                pressed_keys += 1;
            }

            let keys = if sw8.is_low().unwrap() {
                info!("pressed SW8");
                [Keyboard::A]
            } else {
                [Keyboard::NoEventIndicated]
            };

            if pressed_keys == 0 {
                green.off().unwrap();
            } else {
                green.on().unwrap();
            }

            match keyboard.device().write_report(keys) {
                Ok(_) | Err(UsbHidError::WouldBlock | UsbHidError::Duplicate) => {}
                Err(e) => core::panic!("Failed to write keyboard report: {e:?}"),
            }
        }

        if tick_timer.wait().is_ok() {
            match keyboard.tick() {
                Ok(_) | Err(UsbHidError::WouldBlock) => {}
                Err(e) => core::panic!("tick error: {e:?}"),
            }
        }
        //let new_report = KeyboardReport {
        //    modifier: 0,
        //    reserved: 0,
        //    leds: 0,
        //    keycodes: keys,
        //};

        //if new_report.keycodes != prev_report.keycodes {
        //    println!("report is different: {}", keys);
        //    prev_report = new_report;
        //    if let Ok(size) = push_key(new_report) {
        //        println!("Sent keyboard report of size {}", size);
        //    }
        //}

        #[allow(clippy::collapsible_if)]
        if usb_dev.poll(&mut [&mut keyboard]) {
            match keyboard.device().read_report() {
                Ok(_) => info!("update leds"),
                Err(UsbError::WouldBlock) => {}
                Err(e) => core::panic!("Failed to read keyboard input: {e:?}"),
            }
        }
    }
}

//fn push_key(report: KeyboardReport) -> Result<usize, usb_device::UsbError> {
//    cortex_m::interrupt::free(|_| unsafe {
//        let hid = USB_HID.as_mut().unwrap();
//        hid.push_input(&report)
//    })
//}

//static mut USB_BUS: Option<UsbBusAllocator<UsbBus>> = None;
//static mut USB_HID: Option<HIDClass<UsbBus>> = None;
//static mut USB_DEVICE: Option<UsbDevice<UsbBus>> = None;

//fn poll_usb() {
//    unsafe {
//        let Some(dev) = USB_DEVICE.as_mut() else {
//            return;
//        };
//        let Some(hid) = USB_HID.as_mut() else { return };
//
//        dev.poll(&mut [hid]);
//    }
//}

//#[allow(non_snake_case)]
//#[interrupt]
//fn USBCTRL_IRQ() {
//    poll_usb();
//}
