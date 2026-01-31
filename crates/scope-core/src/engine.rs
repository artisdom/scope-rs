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
        let mut desired: Option<SerialConfig> = None;
        let mut port: Option<Box<dyn serialport::SerialPort>> = None;
        let mut backoff_ms = 200u64;
        let mut next_retry = tokio::time::Instant::now();

        let _ = evt_tx.send(EngineEvent::ConnectionState(state.clone())).await;

        let mut tick = tokio::time::interval(std::time::Duration::from_millis(30));

        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => {
                    let Some(cmd) = cmd else { break; };
                    match cmd {
                        EngineCommand::Connect(cfg) => {
                            desired = Some(cfg.clone());
                            port = None;
                            state = ConnectionState::Connecting;
                            let _ = evt_tx.send(EngineEvent::ConnectionState(state.clone())).await;
                            match try_open(&cfg).with_context(|| format!("Failed to open serial port {} @ {}", cfg.port, cfg.baudrate)) {
                                Ok(p) => {
                                    port = Some(p);
                                    state = ConnectionState::Connected;
                                    backoff_ms = 200;
                                    next_retry = tokio::time::Instant::now();
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
                                    let _ = evt_tx.send(EngineEvent::Error(err.to_string())).await;
                                    next_retry = tokio::time::Instant::now()
                                        + std::time::Duration::from_millis(backoff_ms);
                                    backoff_ms = (backoff_ms * 2).min(2000);
                                }
                            }
                        }
                        EngineCommand::Disconnect => {
                            desired = None;
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
                            } else {
                                let _ = evt_tx
                                    .send(EngineEvent::Error("Not connected".to_string()))
                                    .await;
                            }
                        }
                    }
                }
                _ = tick.tick() => {
                    if let Some(cfg) = desired.clone() {
                        if port.is_none() {
                            if tokio::time::Instant::now() >= next_retry {
                                state = ConnectionState::Connecting;
                                let _ = evt_tx.send(EngineEvent::ConnectionState(state.clone())).await;
                                match try_open(&cfg).with_context(|| format!("Failed to open serial port {} @ {}", cfg.port, cfg.baudrate)) {
                                    Ok(p) => {
                                        port = Some(p);
                                        state = ConnectionState::Connected;
                                        backoff_ms = 200;
                                        next_retry = tokio::time::Instant::now();
                                        let _ = evt_tx.send(EngineEvent::ConnectionState(state.clone())).await;
                                        let _ = evt_tx
                                            .send(EngineEvent::Message(LogMessage::new(
                                                Direction::System,
                                                format!("Connected to {} @ {}", cfg.port, cfg.baudrate).into_bytes(),
                                            )))
                                            .await;
                                    }
                                    Err(err) => {
                                        let _ = evt_tx.send(EngineEvent::Error(err.to_string())).await;
                                        next_retry = tokio::time::Instant::now()
                                            + std::time::Duration::from_millis(backoff_ms);
                                        backoff_ms = (backoff_ms * 2).min(2000);
                                    }
                                }
                            }
                        }
                    }

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
                            Err(err) => {
                                if err.kind() != std::io::ErrorKind::TimedOut {
                                    port = None;
                                    let _ = evt_tx.send(EngineEvent::Error(err.to_string())).await;
                                    if desired.is_some() {
                                        state = ConnectionState::Connecting;
                                        let _ = evt_tx.send(EngineEvent::ConnectionState(state.clone())).await;
                                        next_retry = tokio::time::Instant::now()
                                            + std::time::Duration::from_millis(backoff_ms);
                                        backoff_ms = (backoff_ms * 2).min(2000);
                                    } else {
                                        state = ConnectionState::Disconnected;
                                        let _ = evt_tx.send(EngineEvent::ConnectionState(state.clone())).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    EngineHandle { cmd_tx, evt_rx }
}

fn try_open(cfg: &SerialConfig) -> Result<Box<dyn serialport::SerialPort>, serialport::Error> {
    serialport::new(cfg.port.clone(), cfg.baudrate)
        .timeout(std::time::Duration::from_millis(50))
        .open()
}
