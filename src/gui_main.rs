#![deny(warnings)]

use chrono::Local;
use iced::futures::{future, SinkExt};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Application, Element, Length, Settings, Subscription, Theme};
use scope_core::engine::{EngineCommand, EngineEvent};
use scope_core::format::{bytes_to_mixed_segments, SegmentKind};
use scope_core::model::{ConnectionState, Direction, SerialConfig};
use tokio::sync::Mutex;

fn main() -> iced::Result {
    ScopeGui::run(Settings {
        window: iced::window::Settings {
            size: iced::Size::new(980.0, 640.0),
            ..Default::default()
        },
        ..Default::default()
    })
}

struct ScopeGui {
    cmd_tx: tokio::sync::mpsc::Sender<EngineCommand>,

    // We keep the receiver behind a mutex so a Subscription can poll it.
    evt_rx: &'static Mutex<tokio::sync::mpsc::Receiver<EngineEvent>>,

    connection: ConnectionState,

    port: String,
    baudrate: String,

    input: String,

    log: Vec<LogLine>,
}

#[derive(Debug, Clone)]
struct LogLine {
    timestamp: String,
    prefix: String,
    segments: Vec<LogSegment>,
}

#[derive(Debug, Clone)]
struct LogSegment {
    text: String,
    kind: SegmentKind,
}

#[derive(Debug, Clone)]
enum Message {
    PortChanged(String),
    BaudChanged(String),
    InputChanged(String),
    ConnectClicked,
    DisconnectClicked,
    SendPressed,
    EngineEvent(EngineEvent),
}

impl Application for ScopeGui {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        let handle = scope_core::engine::spawn();

        // Leak a mutex to obtain a 'static ref usable by iced subscriptions.
        let evt_rx = Box::leak(Box::new(Mutex::new(handle.evt_rx)));

        (
            Self {
                cmd_tx: handle.cmd_tx,
                evt_rx,
                connection: ConnectionState::Disconnected,
                port: String::new(),
                baudrate: "115200".to_string(),
                input: String::new(),
                log: vec![LogLine {
                    timestamp: Local::now().format("%H:%M:%S.%3f").to_string(),
                    prefix: "[SYS]".to_string(),
                    segments: vec![LogSegment {
                        text: "Scope (GUI) started".to_string(),
                        kind: SegmentKind::Plain,
                    }],
                }],
            },
            iced::Command::none(),
        )
    }

    fn title(&self) -> String {
        "Scope (GUI)".to_string()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let evt_rx = self.evt_rx;
        iced::subscription::channel("engine-events", 256, move |mut output| async move {
            loop {
                let evt = {
                    let mut rx = evt_rx.lock().await;
                    rx.recv().await
                };

                let msg = match evt {
                    Some(evt) => Message::EngineEvent(evt),
                    None => Message::EngineEvent(EngineEvent::Error("engine stopped".into())),
                };

                if output.send(msg).await.is_err() {
                    break;
                }
            }

            // Subscription API expects this task to never finish.
            future::pending::<std::convert::Infallible>().await
        })
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        match message {
            Message::PortChanged(s) => self.port = s,
            Message::BaudChanged(s) => self.baudrate = s,
            Message::InputChanged(s) => self.input = s,

            Message::ConnectClicked => {
                if self.port.trim().is_empty() {
                    self.log.push(LogLine {
                        timestamp: Local::now().format("%H:%M:%S.%3f").to_string(),
                        prefix: "[ERR]".to_string(),
                        segments: vec![LogSegment {
                            text: "Port is empty".to_string(),
                            kind: SegmentKind::Plain,
                        }],
                    });
                    return iced::Command::none();
                }

                let baudrate = self.baudrate.trim().parse::<u32>();
                let baudrate = match baudrate {
                    Ok(b) => b,
                    Err(_) => {
                        self.log.push(LogLine {
                            timestamp: Local::now().format("%H:%M:%S.%3f").to_string(),
                            prefix: "[ERR]".to_string(),
                            segments: vec![LogSegment {
                                text: "Baudrate is not a number".to_string(),
                                kind: SegmentKind::Plain,
                            }],
                        });
                        return iced::Command::none();
                    }
                };

                let cfg = SerialConfig {
                    port: self.port.trim().to_string(),
                    baudrate,
                };

                let tx = self.cmd_tx.clone();
                return iced::Command::perform(
                    async move {
                        let _ = tx.send(EngineCommand::Connect(cfg)).await;
                    },
                    |_| Message::InputChanged(String::new()),
                );
            }

            Message::DisconnectClicked => {
                let tx = self.cmd_tx.clone();
                return iced::Command::perform(
                    async move {
                        let _ = tx.send(EngineCommand::Disconnect).await;
                    },
                    |_| Message::InputChanged(String::new()),
                );
            }

            Message::SendPressed => {
                if self.input.is_empty() {
                    return iced::Command::none();
                }
                let bytes = self.input.clone().into_bytes();
                self.input.clear();

                let tx = self.cmd_tx.clone();
                return iced::Command::perform(
                    async move {
                        let _ = tx.send(EngineCommand::SendBytes(bytes)).await;
                    },
                    |_| Message::InputChanged(String::new()),
                );
            }

            Message::EngineEvent(evt) => match evt {
                EngineEvent::ConnectionState(s) => {
                    self.connection = s;
                }
                EngineEvent::Message(m) => {
                    let dir = match m.direction {
                        Direction::Rx => "RX",
                        Direction::Tx => "TX",
                        Direction::System => "SYS",
                    };
                    let segments = bytes_to_mixed_segments(&m.bytes)
                        .into_iter()
                        .map(|s| LogSegment {
                            text: s.text,
                            kind: s.kind,
                        })
                        .collect::<Vec<_>>();

                    self.log.push(LogLine {
                        timestamp: m.at.format("%H:%M:%S.%3f").to_string(),
                        prefix: format!("[{dir}]"),
                        segments,
                    });

                    // Keep log bounded (simple cap for now)
                    if self.log.len() > 5000 {
                        let drain = self.log.len() - 5000;
                        self.log.drain(0..drain);
                    }
                }
                EngineEvent::Error(e) => {
                    self.log.push(LogLine {
                        timestamp: Local::now().format("%H:%M:%S.%3f").to_string(),
                        prefix: "[ERR]".to_string(),
                        segments: vec![LogSegment {
                            text: e,
                            kind: SegmentKind::Plain,
                        }],
                    });
                }
            },
        }

        iced::Command::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let status = match self.connection {
            ConnectionState::Disconnected => "Disconnected",
            ConnectionState::Connecting => "Connecting…",
            ConnectionState::Connected => "Connected",
        };

        let header = row![
            text("Scope (GUI)").size(22),
            text(format!("Status: {status}")),
        ]
        .spacing(16);

        let controls = row![
            text("Port:"),
            text_input("COM3 or /dev/ttyUSB0", &self.port)
                .on_input(Message::PortChanged)
                .width(Length::Fixed(220.0)),
            text("Baud:"),
            text_input("115200", &self.baudrate)
                .on_input(Message::BaudChanged)
                .width(Length::Fixed(120.0)),
            button("Connect").on_press(Message::ConnectClicked),
            button("Disconnect").on_press(Message::DisconnectClicked),
        ]
        .spacing(12);

        let escape_color = iced::Color::from_rgb8(0xE6, 0xC2, 0x2E); // warm yellow
        let plain_color = iced::Color::WHITE;
        let meta_color = iced::Color::from_rgb8(0x88, 0x88, 0x88);
        let monospace = iced::Font::MONOSPACE;

        let log_column = self.log.iter().fold(column![], |c, line| {
            let segs_row = line.segments.iter().fold(row![].spacing(0), |r, seg| {
                let color = match seg.kind {
                    SegmentKind::Plain => plain_color,
                    SegmentKind::Escape => escape_color,
                };

                r.push(text(&seg.text).font(monospace).style(color))
            });

            // Fixed-width prefix by padding (monospace font).
            let prefix = format!("{:<5}", line.prefix);

            c.push(
                row![
                    text(&line.timestamp).font(monospace).style(meta_color),
                    text(prefix).font(monospace).style(meta_color),
                    segs_row,
                ]
                .spacing(12)
                .width(Length::Shrink),
            )
        });

        let log_view = scrollable(log_column)
            .direction(scrollable::Direction::Both {
                vertical: scrollable::Properties::new(),
                horizontal: scrollable::Properties::new(),
            })
            .height(Length::Fill)
            .width(Length::Fill);

        let input = row![
            text_input("Type and press Enter to send…", &self.input)
                .on_input(Message::InputChanged)
                .on_submit(Message::SendPressed)
                .width(Length::Fill),
            button("Send").on_press(Message::SendPressed),
        ]
        .spacing(12);

        let content = column![header, controls, log_view, input]
            .spacing(12)
            .padding(16)
            .height(Length::Fill);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
