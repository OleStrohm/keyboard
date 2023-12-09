use core::iter::zip;

use heapless::Vec;
use port_expander::dev::pca9555::Driver;
use port_expander::{I2cBus, Pca9555};
use shared_bus::NullMutex;
use usbd_human_interface_device::page::Keyboard as Key;

pub struct Keyboard<I2C: I2cBus> {
    pca: Pca9555<NullMutex<Driver<I2C>>>,
}

macro_rules! keymap {
    ($map:ident == $($id:tt)*) => {
        const $map: [Key; 3] = [ $(Key::$id),* ];
    };
}

#[rustfmt::skip]
keymap!(
  KEYMAP
  ==
  A B
  C
);

impl<I2C: I2cBus> Keyboard<I2C>
where
    I2C::BusError: core::fmt::Debug,
{
    pub fn new(i2c: I2C) -> Self {
        Self {
            pca: Pca9555::new(i2c, true, false, false),
        }
    }

    pub fn pressed_keys(&mut self) -> Vec<Key, 3> {
        let pca_pins = self.pca.split();

        let sw7 = pca_pins.io0_5;
        let sw8 = pca_pins.io0_4;
        let sw9 = pca_pins.io0_3;

        let keys = [sw7, sw8, sw9];

        zip(keys, KEYMAP)
            .map(|(key, action)| {
                if key.is_low().unwrap() {
                    action
                } else {
                    Key::NoEventIndicated
                }
            })
            .collect::<Vec<_, 3>>()
    }
}
