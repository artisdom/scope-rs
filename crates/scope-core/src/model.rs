use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Rx,
    Tx,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerialConfig {
    pub port: String,
    pub baudrate: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogMessage {
    pub at: DateTime<Local>,
    pub direction: Direction,
    pub bytes: Vec<u8>,
}

impl LogMessage {
    pub fn new(direction: Direction, bytes: Vec<u8>) -> Self {
        Self {
            at: Local::now(),
            direction,
            bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
}
