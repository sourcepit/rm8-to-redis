use common_failures::prelude::*;

use gpio::Gpio;
use gpio::PinDirection::Out;
use gpio::PinValue::High;
use gpio::PinValue::Low;
use std::thread;
use std::time::Duration;

const PULSE_LENGTH: usize = 300;

pub struct Switch {
    system_code: [bool; 5],
    switch_code: SwitchCode,
}

impl Switch {
    pub fn new(system_code: &[bool; 5], switch_code: SwitchCode) -> Switch {
        let system_code = system_code.clone();
        Switch {
            system_code,
            switch_code,
        }
    }
}

pub enum SwitchCode {
    SwitchA,
    SwitchB,
    SwitchC,
    SwitchD,
    SwitchE,
}

impl SwitchCode {
    fn add_bits(&self, buf: &mut [u8]) {
        let idx = match self {
            SwitchCode::SwitchA => 0,
            SwitchCode::SwitchB => 1,
            SwitchCode::SwitchC => 2,
            SwitchCode::SwitchD => 3,
            SwitchCode::SwitchE => 4,
        };
        for i in 0..5 {
            if i == idx {
                buf[i] = b'0';
            } else {
                buf[i] = b'F';
            }
        }
    }
}

pub enum SwitchState {
    On,
    Off,
}

impl SwitchState {
    fn add_bits(&self, buf: &mut [u8]) {
        let idx = match self {
            SwitchState::On => 0,
            SwitchState::Off => 1,
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

    pub fn send(&mut self, switch: &Switch, state: SwitchState) {
        let code_word = get_code_word(&switch.system_code, &switch.switch_code, state);
        send_tri_state(&mut self.gpio, self.pin, PULSE_LENGTH, &code_word);
    }
}

pub fn get_code_word(system: &[bool; 5], switch: &SwitchCode, state: SwitchState) -> [u8; 12] {
    let mut buf = [0u8; 12];
    for i in 0..5 {
        if system[i] {
            buf[i] = b'0';
        } else {
            buf[i] = b'F';
        }
    }
    switch.add_bits(&mut buf[5..10]);
    state.add_bits(&mut buf[10..]);
    buf
}

fn send_tri_state(gpio: &mut Gpio, pin: usize, pulse_length: usize, code_word: &[u8; 12]) {
    for _ in 0..10 {
        for c in code_word {
            match c {
                b'0' => send_t0(gpio, pin, pulse_length),
                b'F' => send_tf(gpio, pin, pulse_length),
                b'1' => send_t1(gpio, pin, pulse_length),
                _ => (),
            }
        }
        send_sync(gpio, pin, pulse_length);
    }
}

// Sends a Tri-State "0" Bit
//            _     _
// Waveform: | |___| |___
//
fn send_t0(gpio: &mut Gpio, pin: usize, pulse_length: usize) {
    transmit(gpio, pin, pulse_length, 1, 3);
    transmit(gpio, pin, pulse_length, 1, 3);
}

// Sends a Tri-State "1" Bit
//            ___   ___
// Waveform: |   |_|   |_
//
fn send_t1(gpio: &mut Gpio, pin: usize, pulse_length: usize) {
    transmit(gpio, pin, pulse_length, 3, 1);
    transmit(gpio, pin, pulse_length, 3, 1);
}

// Sends a Tri-State "F" Bit
//            _     ___
// Waveform: | |___|   |_
//
fn send_tf(gpio: &mut Gpio, pin: usize, pulse_length: usize) {
    transmit(gpio, pin, pulse_length, 1, 3);
    transmit(gpio, pin, pulse_length, 3, 1);
}

fn send_sync(gpio: &mut Gpio, pin: usize, pulse_length: usize) {
    transmit(gpio, pin, pulse_length, 1, 31);
}

fn transmit(
    gpio: &mut Gpio,
    pin: usize,
    pulse_length: usize,
    high_pulses: usize,
    low_pulses: usize,
) {
    for _ in 0..high_pulses {
        gpio.set_pin_value(pin, High);
        thread::sleep(Duration::from_micros(pulse_length as u64));
    }
    for _ in 0..low_pulses {
        gpio.set_pin_value(pin, Low);
        thread::sleep(Duration::from_micros(pulse_length as u64));
    }
}
