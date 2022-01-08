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

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RelayState {
    On,
    Off,
}

pub struct Rm8Control<'a> {
    gpio: Gpio<'a>,
    pins: Vec<usize>,
    invert_outputs: bool,
}

impl<'a> Rm8Control<'a> {
    pub fn open(pins: Vec<usize>, invert_outputs: bool) -> Result<Rm8Control<'a>> {
        let mut gpio = Gpio::open()?;
        for pin in &pins {
            gpio.set_pin_direction(*pin, Out);
        }
        Ok(Rm8Control {
            gpio,
            pins,
            invert_outputs,
        })
    }

    pub fn send(&mut self, relay: &Relay, state: RelayState) {
        let idx = match relay {
            Relay::Relay1 => 0,
            Relay::Relay2 => 1,
            Relay::Relay3 => 2,
            Relay::Relay4 => 3,
            Relay::Relay5 => 4,
            Relay::Relay6 => 5,
            Relay::Relay7 => 6,
            Relay::Relay8 => 7,
        };

        let pin: usize = *self.pins.get(idx).unwrap();

        let value = match state {
            RelayState::On => {
                if self.invert_outputs {
                    Low
                } else {
                    High
                }
            }
            RelayState::Off => {
                if self.invert_outputs {
                    High
                } else {
                    Low
                }
            }
        };

        self.gpio.set_pin_value(pin, value)
    }
}
