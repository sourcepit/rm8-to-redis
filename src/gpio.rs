use common_failures::prelude::*;

use libc::mmap;
use libc::munmap;
use libc::MAP_SHARED;
use libc::PROT_READ;
use libc::PROT_WRITE;
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::ptr;
use std::slice;

const GPIO_LENGTH: usize = 4096;
#[cfg(target_arch = "arm")]
const GPIO_OFFSET: i32 = 0x200000;
#[cfg(target_arch = "x86_64")]
const GPIO_OFFSET: i64 = 0x200000;

#[derive(PartialEq)]
pub enum PinDirection {
    In,
    Out,
}

#[derive(PartialEq)]
pub enum PinValue {
    High,
    Low,
}

pub struct Gpio<'a> {
    mem: &'a mut [u32],
}

impl<'a> Gpio<'a> {
    pub fn open() -> Result<Gpio<'a>> {
        let gpiomem_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/gpiomem")?;

        let gpio_mem = unsafe {
            let gpio_mem_ptr = mmap(
                ptr::null_mut(),
                GPIO_LENGTH,
                PROT_READ | PROT_WRITE,
                MAP_SHARED,
                gpiomem_file.as_raw_fd(),
                GPIO_OFFSET,
            ) as *mut u32;
            let len = GPIO_LENGTH / 32;
            slice::from_raw_parts_mut(gpio_mem_ptr, len)
        };

        Ok(Gpio { mem: gpio_mem })
    }

    pub fn set_pin_direction(&mut self, pin: usize, direction: PinDirection) {
        match direction {
            PinDirection::In => {
                self.mem[pin / 10] &= !(7 << ((pin % 10) * 3));
            }
            PinDirection::Out => {
                self.set_pin_direction(pin, PinDirection::In);
                self.mem[pin / 10] |= 1 << ((pin % 10) * 3);
            }
        }
    }

    pub fn set_pin_value(&mut self, pin: usize, value: PinValue) {
        match value {
            PinValue::High => {
                self.mem[7] = 1 << pin;
            }
            PinValue::Low => {
                self.mem[10] = 1 << pin;
            }
        }
    }

    pub fn _get_pin_value(&self, pin: usize) -> PinValue {
        match self.mem[13] & 1 << pin > 0 {
            true => PinValue::High,
            false => PinValue::Low,
        }
    }
}

impl<'a> Drop for Gpio<'a> {
    fn drop(&mut self) {
        unsafe {
            munmap(self.mem.as_mut_ptr() as *mut _, GPIO_LENGTH);
        }
    }
}
