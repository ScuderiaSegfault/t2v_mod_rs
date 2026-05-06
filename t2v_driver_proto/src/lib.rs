use serde::{Deserialize, Serialize};
use t2v_module::{IrNecFrame, TireHallSensorReading};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Request {
    ReceiveNecFrames { enable: bool },
    ReceiveTireHallSensorReadings { enable: bool },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Event {
    IrNecFrame(IrNecFrame),
    TireHallSensorReading(TireHallSensorReading),
    Connected,
    Disconnected,
}
