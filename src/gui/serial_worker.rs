use chrono::{DateTime, Local};
use serialport::{DataBits, FlowControl, Parity, StopBits};
use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct SerialConfig {
    pub port: String,
    pub baud_rate: u32,
    pub data_bits: DataBits,
    pub stop_bits: StopBits,
    pub parity: Parity,
    pub flow_control: FlowControl,
}

#[derive(Debug)]
pub enum SerialCommand {
    Connect(SerialConfig),
    Disconnect,
    Send(Vec<u8>),
    Shutdown,
}

#[derive(Debug)]
pub enum SerialEvent {
    Connected {
        port: String,
        baud_rate: u32,
    },
    Disconnected,
    Error(String),
    RxLine {
        timestamp: DateTime<Local>,
        bytes: Vec<u8>,
    },
    TxEcho {
        timestamp: DateTime<Local>,
        bytes: Vec<u8>,
    },
}

pub struct SerialHandle {
    pub cmd_tx: Sender<SerialCommand>,
    pub event_rx: Receiver<SerialEvent>,
}

const READ_TIMEOUT_MS: u64 = 50;
const IDLE_FLUSH_MS: u128 = 250;
const READ_BUF_SIZE: usize = 1024;

pub fn spawn_serial_worker(egui_ctx: egui::Context) -> SerialHandle {
    let (cmd_tx, cmd_rx) = channel::<SerialCommand>();
    let (event_tx, event_rx) = channel::<SerialEvent>();
    thread::Builder::new()
        .name("serial-worker".to_string())
        .spawn(move || worker_loop(cmd_rx, event_tx, egui_ctx))
        .expect("failed to spawn serial worker thread");
    SerialHandle { cmd_tx, event_rx }
}

fn worker_loop(
    cmd_rx: Receiver<SerialCommand>,
    event_tx: Sender<SerialEvent>,
    egui_ctx: egui::Context,
) {
    let send_event = |evt: SerialEvent| {
        let _ = event_tx.send(evt);
        egui_ctx.request_repaint();
    };

    let mut port: Option<Box<dyn serialport::SerialPort>> = None;
    let mut buf = [0u8; READ_BUF_SIZE];
    let mut line_buf: Vec<u8> = Vec::with_capacity(READ_BUF_SIZE);
    let mut last_byte_at = Instant::now();

    loop {
        match cmd_rx.try_recv() {
            Ok(SerialCommand::Connect(cfg)) => {
                port = None;
                line_buf.clear();
                let res = serialport::new(&cfg.port, cfg.baud_rate)
                    .data_bits(cfg.data_bits)
                    .stop_bits(cfg.stop_bits)
                    .parity(cfg.parity)
                    .flow_control(cfg.flow_control)
                    .timeout(Duration::from_millis(READ_TIMEOUT_MS))
                    .open();
                match res {
                    Ok(p) => {
                        port = Some(p);
                        last_byte_at = Instant::now();
                        send_event(SerialEvent::Connected {
                            port: cfg.port,
                            baud_rate: cfg.baud_rate,
                        });
                    }
                    Err(e) => send_event(SerialEvent::Error(format!(
                        "Failed to open {}: {}",
                        cfg.port, e
                    ))),
                }
            }
            Ok(SerialCommand::Disconnect) => {
                if port.is_some() {
                    port = None;
                    if !line_buf.is_empty() {
                        let bytes = std::mem::take(&mut line_buf);
                        send_event(SerialEvent::RxLine {
                            timestamp: Local::now(),
                            bytes,
                        });
                    }
                    send_event(SerialEvent::Disconnected);
                }
            }
            Ok(SerialCommand::Send(data)) => {
                if let Some(p) = port.as_mut() {
                    match p.write_all(&data) {
                        Ok(_) => {
                            let _ = p.flush();
                            send_event(SerialEvent::TxEcho {
                                timestamp: Local::now(),
                                bytes: data,
                            });
                        }
                        Err(e) => send_event(SerialEvent::Error(format!("Write error: {}", e))),
                    }
                } else {
                    send_event(SerialEvent::Error("Not connected".to_string()));
                }
            }
            Ok(SerialCommand::Shutdown) => return,
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => return,
        }

        if let Some(p) = port.as_mut() {
            match p.read(&mut buf) {
                Ok(n) if n > 0 => {
                    last_byte_at = Instant::now();
                    for &b in &buf[..n] {
                        line_buf.push(b);
                        if b == b'\n' {
                            let bytes = std::mem::take(&mut line_buf);
                            send_event(SerialEvent::RxLine {
                                timestamp: Local::now(),
                                bytes,
                            });
                        }
                    }
                }
                Ok(_) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => {
                    port = None;
                    if !line_buf.is_empty() {
                        let bytes = std::mem::take(&mut line_buf);
                        send_event(SerialEvent::RxLine {
                            timestamp: Local::now(),
                            bytes,
                        });
                    }
                    send_event(SerialEvent::Error(format!("Read error: {}", e)));
                    send_event(SerialEvent::Disconnected);
                }
            }

            if !line_buf.is_empty() && last_byte_at.elapsed().as_millis() > IDLE_FLUSH_MS {
                let bytes = std::mem::take(&mut line_buf);
                send_event(SerialEvent::RxLine {
                    timestamp: Local::now(),
                    bytes,
                });
                last_byte_at = Instant::now();
            }
        } else {
            thread::sleep(Duration::from_millis(20));
        }
    }
}
