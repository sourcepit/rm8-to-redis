use log::info;

use crate::rm8_types::RelayId;
use crate::rm8_types::RelayId::*;
use crate::rm8_types::RelayState;

pub struct Rm8Control {}

impl Rm8Control {
    pub fn open(pins: Vec<usize>, invert_outputs: bool) -> Result<Rm8Control, std::io::Error> {
        Ok(Rm8Control {})
    }

    pub fn set_all(&mut self, state: RelayState) {
        self.set(&Relay1, state);
        self.set(&Relay2, state);
        self.set(&Relay3, state);
        self.set(&Relay4, state);
        self.set(&Relay5, state);
        self.set(&Relay6, state);
        self.set(&Relay7, state);
        self.set(&Relay8, state);
    }

    pub fn set(&mut self, relay: &RelayId, state: RelayState) {
        info!("Set '{:?}' to '{:?}'", relay, state);
    }
}
