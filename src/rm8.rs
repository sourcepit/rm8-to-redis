use common_failures::prelude::*;

use gpio::Gpio;
use gpio::PinDirection::Out;
use gpio::PinValue::High;
use gpio::PinValue::Low;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Relay {
    Relay1,
    Relay2,
    Relay3,
    Relay4,
    Relay5,
    Relay6,
    Relay7,
    Relay8,
}

impl Relay {
    fn add_bits(&self, buf: &mut [u8]) {
        let idx = match self {
            Relay::Relay1 => 0,
            Relay::Relay2 => 1,
            Relay::Relay3 => 2,
            Relay::Relay4 => 3,
            Relay::Relay5 => 4,
            Relay::Relay6 => 5,
            Relay::Relay7 => 6,
            Relay::Relay8 => 7,
        };
        for i in 0..8 {
            if i == idx {
                buf[i] = b'0';
            } else {
                buf[i] = b'F';
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RelayState {
    On,
    Off,
}

impl RelayState {
    fn add_bits(&self, buf: &mut [u8]) {
        let idx = match self {
            RelayState::On => 0,
            RelayState::Off => 1,
        };
        for i in 0..2 {
            if i == idx {
                buf[i] = b'0';
            } else {
                buf[i] = b'F';
            }
        }
    }
}

pub struct RemoteControl<'a> {
    gpio: Gpio<'a>,
    pin: usize,
}

impl<'a> RemoteControl<'a> {
    pub fn open(pin: usize) -> Result<RemoteControl<'a>> {
        let mut gpio = Gpio::open()?;
        gpio.set_pin_direction(pin, Out);
        Ok(RemoteControl { gpio, pin })
    }

    pub fn send(&mut self, relay: &Relay, state: RelayState) {
        // TODO
        let value = match state {
            RelayState::On => High,
            RelayState::Off => Low,
        };
        self.gpio.set_pin_value(self.pin, value)
    }
}
