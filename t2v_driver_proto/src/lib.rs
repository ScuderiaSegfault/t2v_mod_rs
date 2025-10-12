
use serde::{Deserialize, Serialize};
use t2v_module::IrNecFrame;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Request {
    ReceiveNecFrames { enable: bool },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Event {
    IrNecFrame(IrNecFrame),
    Connected,
    Disconnected,
}
