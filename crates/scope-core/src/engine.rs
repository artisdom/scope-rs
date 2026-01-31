use crate::model::{ConnectionState, Direction, LogMessage, SerialConfig};
use anyhow::Context;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum EngineCommand {
    Connect(SerialConfig),
    Disconnect,
    SendBytes(Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    ConnectionState(ConnectionState),
    Message(LogMessage),
    Error(String),
}

pub struct EngineHandle {
    pub cmd_tx: mpsc::Sender<EngineCommand>,
    pub evt_rx: mpsc::Receiver<EngineEvent>,
}

/// Spawn a UI-agnostic engine task.
///
/// NOTE: This is an intentionally small starting point. We will evolve it to match
/// the existing Scope feature set (auto-reconnect, history, record/save, etc.).
pub fn spawn() -> EngineHandle {
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<EngineCommand>(64);
    let (evt_tx, evt_rx) = mpsc::channel::<EngineEvent>(512);

    tokio::spawn(async move {
        let mut state = ConnectionState::Disconnected;
        let _ = evt_tx.send(EngineEvent::ConnectionState(state.clone())).await;

        // For now, we keep the serial port inside this task.
        let mut port: Option<Box<dyn serialport::SerialPort>> = None;

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                EngineCommand::Connect(cfg) => {
                    if matches!(state, ConnectionState::Connected) {
                        continue;
                    }
                    state = ConnectionState::Connecting;
                    let _ = evt_tx.send(EngineEvent::ConnectionState(state.clone())).await;

                    match serialport::new(cfg.port.clone(), cfg.baudrate)
                        .timeout(std::time::Duration::from_millis(50))
                        .open()
                        .with_context(|| format!("Failed to open serial port {} @ {}", cfg.port, cfg.baudrate))
                    {
                        Ok(p) => {
                            port = Some(p);
                            state = ConnectionState::Connected;
                            let _ = evt_tx.send(EngineEvent::ConnectionState(state.clone())).await;
                            let _ = evt_tx
                                .send(EngineEvent::Message(LogMessage::new(
                                    Direction::System,
                                    format!("Connected to {} @ {}", cfg.port, cfg.baudrate).into_bytes(),
                                )))
                                .await;
                        }
                        Err(err) => {
                            port = None;
                            state = ConnectionState::Disconnected;
                            let _ = evt_tx.send(EngineEvent::ConnectionState(state.clone())).await;
                            let _ = evt_tx.send(EngineEvent::Error(err.to_string())).await;
                        }
                    }
                }
                EngineCommand::Disconnect => {
                    port = None;
                    state = ConnectionState::Disconnected;
                    let _ = evt_tx.send(EngineEvent::ConnectionState(state.clone())).await;
                }
                EngineCommand::SendBytes(bytes) => {
                    if let Some(p) = port.as_mut() {
                        if let Err(err) = p.write_all(&bytes) {
                            let _ = evt_tx.send(EngineEvent::Error(err.to_string())).await;
                        } else {
                            let _ = evt_tx
                                .send(EngineEvent::Message(LogMessage::new(Direction::Tx, bytes)))
                                .await;
                        }
                    }
                }
            }

            // Read opportunistically after handling a command.
            if let Some(p) = port.as_mut() {
                let mut buf = [0u8; 4096];
                match p.read(&mut buf) {
                    Ok(0) => {}
                    Ok(n) => {
                        let _ = evt_tx
                            .send(EngineEvent::Message(LogMessage::new(
                                Direction::Rx,
                                buf[..n].to_vec(),
                            )))
                            .await;
                    }
                    Err(_timeout) => {
                        // ignore timeouts
                    }
                }
            }
        }
    });

    EngineHandle { cmd_tx, evt_rx }
}
