#![no_std]
#![no_main]

use bsp::entry;
use cortex_m::peripheral::NVIC;
use defmt::*;
use defmt_rtt as _;
use fugit::RateExtU32;
use panic_probe as _;

use port_expander::Pca9555;
use seeeduino_xiao_rp2040 as bsp;

use bsp::hal::{
    clocks::{init_clocks_and_plls, Clock},
    i2c::I2C,
    pac::{self, interrupt, Interrupt},
    sio::Sio,
    usb::UsbBus,
    watchdog::Watchdog,
};
use switch_hal::{ActiveLow, OutputSwitch, Switch};
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_hid::{
    descriptor::{KeyboardReport, SerializedDescriptor},
    hid_class::HIDClass,
};

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

    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());

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

    let bus = unsafe {
        USB_BUS = Some(UsbBusAllocator::new(UsbBus::new(
            pac.USBCTRL_REGS,
            pac.USBCTRL_DPRAM,
            clocks.usb_clock,
            true,
            &mut pac.RESETS,
        )));
        USB_BUS.as_ref().unwrap()
    };

    unsafe {
        USB_HID = Some(HIDClass::new(bus, KeyboardReport::desc(), 60));
        USB_DEVICE = Some(
            UsbDeviceBuilder::new(bus, UsbVidPid(0x16c0, 0x27dd))
                .manufacturer("Ole Marius Strohm")
                .product("Rusty Keyboard")
                .serial_number("OLESTROHM")
                .build(),
        );
    }

    unsafe {
        core.NVIC.set_priority(Interrupt::USBCTRL_IRQ, 1);
        NVIC::unmask(Interrupt::USBCTRL_IRQ);
    }

    info!("Preparing to send key!");

    delay.delay_ms(2500);
    info!("Sent key!");

    loop {
        let mut keys = [0, 0, 0, 0, 0, 0];
        let mut pressed_keys = 0;
        if sw7.is_low().unwrap() {
            info!("pressed SW7");
            keys[pressed_keys] = 30;
            pressed_keys += 1;
        }
        if sw8.is_low().unwrap() {
            info!("pressed SW8");
            keys[pressed_keys] = 31;
            pressed_keys += 1;
        }
        if sw9.is_low().unwrap() {
            info!("pressed SW9");
            keys[pressed_keys] = 32;
            pressed_keys += 1;
        }
        push_key(KeyboardReport {
            modifier: 0,
            reserved: 0,
            leds: 0,
            keycodes: keys,
        })
        .unwrap();

        if pressed_keys == 0 {
            green.off().unwrap();
        } else {
            green.on().unwrap();
        }
        delay.delay_ms(70);
    }
}

fn push_key(report: KeyboardReport) -> Result<usize, usb_device::UsbError> {
    cortex_m::interrupt::free(|_| unsafe {
        USB_HID.as_mut().map(|hid| hid.push_input(&report)).unwrap()
    })
}

static mut USB_BUS: Option<UsbBusAllocator<UsbBus>> = None;
static mut USB_HID: Option<HIDClass<UsbBus>> = None;
static mut USB_DEVICE: Option<UsbDevice<UsbBus>> = None;

fn poll_usb() {
    unsafe {
        let Some(dev) = USB_DEVICE.as_mut() else { return };
        let Some(hid) = USB_HID.as_mut() else { return };

        dev.poll(&mut [hid]);
    }
}

#[allow(non_snake_case)]
#[interrupt]
fn USBCTRL_IRQ() {
    poll_usb();
}
