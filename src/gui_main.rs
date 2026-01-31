#![deny(warnings)]

mod gui_keyboard;
#[allow(dead_code)]
mod infra;

use chrono::Local;
use iced::futures::{future, SinkExt};
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Application, Element, Length, Settings, Subscription, Theme};
use scope_core::engine::{EngineCommand, EngineEvent};
use scope_core::format::{bytes_to_ansi_segments, AnsiColor, SegmentKind};
use scope_core::model::{ConnectionState, Direction, SerialConfig};
use tokio::sync::Mutex;

use crate::infra::recorder::Recorder;
use crate::infra::typewriter::TypeWriter;

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
    history: Vec<String>,
    history_index: Option<usize>,
    history_backup: String,
    append_crlf: bool,

    log: Vec<LogLine>,

    log_scroll_id: scrollable::Id,
    auto_scroll: bool,
    scroll_x: f32,
    scroll_y: f32,

    typewriter: TypeWriter,
    recorder: Recorder,
}

#[derive(Debug, Clone)]
struct LogLine {
    timestamp: String,
    prefix: String,
    kind: LogKind,
    segments: Vec<LogSegment>,
}

#[derive(Debug, Clone, Copy)]
enum LogKind {
    Rx,
    Tx,
    Sys,
    Err,
}

#[derive(Debug, Clone)]
struct LogSegment {
    text: String,
    kind: SegmentKind,
    color: AnsiColor,
}

#[derive(Debug, Clone)]
enum Message {
    PortChanged(String),
    BaudChanged(String),
    InputChanged(String),
    ConnectClicked,
    DisconnectClicked,
    SendPressed,
    JumpToEnd,
    JumpToStart,
    AutoScrollToggled(bool),
    AppendCrlfToggled(bool),
    LogScrolled(scrollable::Viewport),
    ScrollPageUp,
    ScrollPageDown,
    HistoryPrev,
    HistoryNext,
    SaveHistory,
    ToggleRecord,
    ClearLog,
    EngineEvent(EngineEvent),
}

fn shortcut_to_message(s: gui_keyboard::Shortcut) -> Message {
    match s {
        gui_keyboard::Shortcut::JumpToEnd => Message::JumpToEnd,
        gui_keyboard::Shortcut::JumpToStart => Message::JumpToStart,
        gui_keyboard::Shortcut::ScrollPageUp => Message::ScrollPageUp,
        gui_keyboard::Shortcut::ScrollPageDown => Message::ScrollPageDown,
        gui_keyboard::Shortcut::HistoryPrev => Message::HistoryPrev,
        gui_keyboard::Shortcut::HistoryNext => Message::HistoryNext,
        gui_keyboard::Shortcut::SaveHistory => Message::SaveHistory,
        gui_keyboard::Shortcut::ToggleRecord => Message::ToggleRecord,
        gui_keyboard::Shortcut::ClearLog => Message::ClearLog,
    }
}

impl ScopeGui {
    fn engine_subscription(&self) -> Subscription<Message> {
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

    fn snap_to_end(&self) -> iced::Command<Message> {
        scrollable::snap_to(
            self.log_scroll_id.clone(),
            scrollable::RelativeOffset {
                x: self.scroll_x,
                y: 1.0,
            },
        )
    }

    fn snap_to_start(&self) -> iced::Command<Message> {
        scrollable::snap_to(
            self.log_scroll_id.clone(),
            scrollable::RelativeOffset {
                x: self.scroll_x,
                y: 0.0,
            },
        )
    }

    fn snap_to_relative(&self, y: f32) -> iced::Command<Message> {
        scrollable::snap_to(
            self.log_scroll_id.clone(),
            scrollable::RelativeOffset {
                x: self.scroll_x,
                y: y.clamp(0.0, 1.0),
            },
        )
    }

    fn add_log_line(
        &mut self,
        kind: LogKind,
        timestamp: chrono::DateTime<Local>,
        prefix: &str,
        segments: Vec<LogSegment>,
    ) -> Option<iced::Command<Message>> {
        let line = LogLine {
            timestamp: timestamp.format("%H:%M:%S.%3f").to_string(),
            prefix: prefix.to_string(),
            kind,
            segments,
        };

        let serialized = line.serialize(&timestamp);
        self.typewriter += vec![serialized.clone()];
        if self.recorder.is_recording() {
            if let Err(err) = self.recorder.add_bulk_content(vec![serialized]) {
                self.push_system_error(format!("Recorder error: {err}"));
            }
        }

        self.log.push(line);
        if self.log.len() > 5000 {
            let drain = self.log.len() - 5000;
            self.log.drain(0..drain);
        }

        if self.auto_scroll {
            return Some(self.snap_to_end());
        }

        None
    }

    fn push_system_info(&mut self, msg: impl Into<String>) {
        let timestamp = Local::now();
        let segments = vec![LogSegment {
            text: msg.into(),
            kind: SegmentKind::Plain,
            color: AnsiColor::Reset,
        }];
        let _ = self.add_log_line(LogKind::Sys, timestamp, "[SYS]", segments);
    }

    fn push_system_error(&mut self, msg: impl Into<String>) {
        let timestamp = Local::now();
        let segments = vec![LogSegment {
            text: msg.into(),
            kind: SegmentKind::Plain,
            color: AnsiColor::Red,
        }];
        let _ = self.add_log_line(LogKind::Err, timestamp, "[ERR]", segments);
    }

    fn apply_history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match &mut self.history_index {
            None => {
                self.history_index = Some(self.history.len() - 1);
                self.history_backup.clone_from(&self.input);
            }
            Some(0) => {}
            Some(idx) => *idx -= 1,
        }

        if let Some(idx) = self.history_index {
            self.input.clone_from(&self.history[idx]);
        }
    }

    fn apply_history_next(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match &mut self.history_index {
            None => {}
            Some(idx) if *idx == self.history.len() - 1 => {
                self.history_index = None;
                self.input.clone_from(&self.history_backup);
            }
            Some(idx) => {
                *idx += 1;
                self.input.clone_from(&self.history[*idx]);
            }
        }
    }

    fn connect_from_fields(&mut self) -> Option<iced::Command<Message>> {
        if self.port.trim().is_empty() {
            self.push_system_error("Port is empty".to_string());
            return None;
        }

        let baudrate = match self.baudrate.trim().parse::<u32>() {
            Ok(b) => b,
            Err(_) => {
                self.push_system_error("Baudrate is not a number".to_string());
                return None;
            }
        };

        let cfg = SerialConfig {
            port: self.port.trim().to_string(),
            baudrate,
        };

        let tx = self.cmd_tx.clone();
        Some(iced::Command::perform(
            async move {
                let _ = tx.send(EngineCommand::Connect(cfg)).await;
            },
            |_| Message::InputChanged(String::new()),
        ))
    }

    fn handle_command(&mut self, raw: String) -> Option<iced::Command<Message>> {
        let mut parts = raw.trim_start_matches('!').split_whitespace();
        let cmd = parts.next()?;

        match cmd {
            "serial" => {
                let sub = parts.next().unwrap_or("connect");
                match sub {
                    "connect" => {
                        if let Some(port_or_baud) = parts.next() {
                            if port_or_baud.chars().all(|c| c.is_ascii_digit()) {
                                self.baudrate = port_or_baud.to_string();
                            } else {
                                self.port = port_or_baud.to_string();
                            }
                        }
                        if let Some(port_or_baud) = parts.next() {
                            if port_or_baud.chars().all(|c| c.is_ascii_digit()) {
                                self.baudrate = port_or_baud.to_string();
                            } else {
                                self.port = port_or_baud.to_string();
                            }
                        }
                        return self.connect_from_fields();
                    }
                    "disconnect" => {
                        let tx = self.cmd_tx.clone();
                        return Some(iced::Command::perform(
                            async move {
                                let _ = tx.send(EngineCommand::Disconnect).await;
                            },
                            |_| Message::InputChanged(String::new()),
                        ));
                    }
                    _ => {
                        self.push_system_error("Invalid serial subcommand".to_string());
                    }
                }
            }
            "connect" => {
                if let Some(port_or_baud) = parts.next() {
                    if port_or_baud.chars().all(|c| c.is_ascii_digit()) {
                        self.baudrate = port_or_baud.to_string();
                    } else {
                        self.port = port_or_baud.to_string();
                    }
                }
                if let Some(port_or_baud) = parts.next() {
                    if port_or_baud.chars().all(|c| c.is_ascii_digit()) {
                        self.baudrate = port_or_baud.to_string();
                    } else {
                        self.port = port_or_baud.to_string();
                    }
                }
                return self.connect_from_fields();
            }
            "disconnect" => {
                let tx = self.cmd_tx.clone();
                return Some(iced::Command::perform(
                    async move {
                        let _ = tx.send(EngineCommand::Disconnect).await;
                    },
                    |_| Message::InputChanged(String::new()),
                ));
            }
            _ => {
                self.push_system_error("Unknown command".to_string());
            }
        }

        None
    }

    fn replace_hex_sequence(command_line: String) -> Vec<u8> {
        let mut output = vec![];
        let mut in_hex_seq = false;
        let valid = "0123456789abcdefABCDEF,_-. ";
        let mut hex_shift = 0;
        let mut hex_val = None;

        for c in command_line.chars() {
            if !in_hex_seq {
                if c == '$' {
                    in_hex_seq = true;
                    hex_shift = 0;
                    hex_val = Some(0);
                    continue;
                }

                output.push(c as u8);
            } else {
                if !valid.contains(c) {
                    in_hex_seq = false;
                    output.push(c as u8);
                    continue;
                }

                match c {
                    '0'..='9' => {
                        *hex_val.get_or_insert(0) <<= hex_shift;
                        *hex_val.get_or_insert(0) |= c as u8 - b'0';
                    }
                    'a'..='f' => {
                        *hex_val.get_or_insert(0) <<= hex_shift;
                        *hex_val.get_or_insert(0) |= c as u8 - b'a' + 0x0a;
                    }
                    'A'..='F' => {
                        *hex_val.get_or_insert(0) <<= hex_shift;
                        *hex_val.get_or_insert(0) |= c as u8 - b'A' + 0x0A;
                    }
                    _ => {
                        if let Some(hex) = hex_val.take() {
                            output.push(hex);
                        }
                        hex_shift = 0;
                        continue;
                    }
                }

                if hex_shift == 0 {
                    hex_shift = 4;
                } else {
                    if let Some(hex) = hex_val.take() {
                        output.push(hex);
                    }
                    hex_shift = 0;
                }
            }
        }

        output
    }
}

impl LogLine {
    fn serialize(&self, timestamp: &chrono::DateTime<Local>) -> String {
        let content = self
            .segments
            .iter()
            .map(|seg| seg.text.clone())
            .collect::<Vec<_>>()
            .join("");

        match self.kind {
            LogKind::Rx => format!("[{}][ <=] {}", timestamp.format("%H:%M:%S.%3f"), content),
            LogKind::Tx => format!("[{}][ =>] {}", timestamp.format("%H:%M:%S.%3f"), content),
            LogKind::Sys => format!("[{}][SYS] {}", timestamp.format("%H:%M:%S.%3f"), content),
            LogKind::Err => format!("[{}][ERR] {}", timestamp.format("%H:%M:%S.%3f"), content),
        }
    }
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

        let now_str = Local::now().format("%Y%m%d_%H%M%S");
        let storage_base_filename = format!("{}.txt", now_str);

        (
            Self {
                cmd_tx: handle.cmd_tx,
                evt_rx,
                connection: ConnectionState::Disconnected,
                port: String::new(),
                baudrate: "115200".to_string(),
                input: String::new(),
                history: vec![],
                history_index: None,
                history_backup: String::new(),
                append_crlf: true,
                log: vec![LogLine {
                    timestamp: Local::now().format("%H:%M:%S.%3f").to_string(),
                    prefix: "[SYS]".to_string(),
                    kind: LogKind::Sys,
                    segments: vec![LogSegment {
                        text: "Scope (GUI) started".to_string(),
                        kind: SegmentKind::Plain,
                        color: AnsiColor::Reset,
                    }],
                }],
                log_scroll_id: scrollable::Id::unique(),
                auto_scroll: true,
                scroll_x: 0.0,
                scroll_y: 0.0,
                typewriter: TypeWriter::new(storage_base_filename.clone()),
                recorder: Recorder::new(storage_base_filename.clone())
                    .expect("Cannot create Recorder"),
            },
            iced::Command::none(),
        )
    }

    fn title(&self) -> String {
        "Scope (GUI)".to_string()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch([
            self.engine_subscription(),
            gui_keyboard::subscription().map(shortcut_to_message),
        ])
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        match message {
            Message::PortChanged(s) => self.port = s,
            Message::BaudChanged(s) => self.baudrate = s,
            Message::InputChanged(s) => self.input = s,
            Message::AppendCrlfToggled(enabled) => self.append_crlf = enabled,

            Message::ConnectClicked => {
                if let Some(cmd) = self.connect_from_fields() {
                    return cmd;
                }
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
                let raw = self.input.trim().to_string();
                if raw.is_empty() {
                    return iced::Command::none();
                }

                self.input.clear();
                self.history_index = None;

                if self.history.last().map(|s| s.as_str()) != Some(raw.as_str()) {
                    self.history.push(raw.clone());
                }

                if raw.starts_with('!') {
                    if let Some(cmd) = self.handle_command(raw) {
                        return cmd;
                    }
                    return iced::Command::none();
                }

                let mut bytes = Self::replace_hex_sequence(raw);
                if self.append_crlf {
                    bytes.extend_from_slice(b"\r\n");
                }

                let tx = self.cmd_tx.clone();
                return iced::Command::perform(
                    async move {
                        let _ = tx.send(EngineCommand::SendBytes(bytes)).await;
                    },
                    |_| Message::InputChanged(String::new()),
                );
            }

            Message::JumpToEnd => {
                self.auto_scroll = true;
                return self.snap_to_end();
            }

            Message::JumpToStart => {
                self.auto_scroll = false;
                return self.snap_to_start();
            }

            Message::AutoScrollToggled(enabled) => {
                self.auto_scroll = enabled;
                if self.auto_scroll {
                    return self.snap_to_end();
                }
            }

            Message::LogScrolled(viewport) => {
                let rel = viewport.relative_offset();
                self.scroll_x = rel.x;
                self.scroll_y = rel.y;

                self.auto_scroll = rel.y >= 0.999;
            }

            Message::ScrollPageUp => {
                self.auto_scroll = false;
                let next = (self.scroll_y - 0.15).max(0.0);
                self.scroll_y = next;
                return self.snap_to_relative(next);
            }

            Message::ScrollPageDown => {
                let next = (self.scroll_y + 0.15).min(1.0);
                self.scroll_y = next;
                self.auto_scroll = next >= 0.999;
                return self.snap_to_relative(next);
            }

            Message::HistoryPrev => self.apply_history_prev(),
            Message::HistoryNext => self.apply_history_next(),

            Message::SaveHistory => {
                if self.recorder.is_recording() {
                    self.push_system_error("Cannot save while recording".to_string());
                    return iced::Command::none();
                }

                let filename = self.typewriter.get_filename();
                match self.typewriter.flush() {
                    Ok(_) => self.push_system_info(format!("Content saved on \"{}\"", filename)),
                    Err(err) => {
                        self.push_system_error(format!("Cannot save on \"{}\": {}", filename, err));
                    }
                }
            }

            Message::ToggleRecord => {
                let filename = self.recorder.get_filename();
                if self.recorder.is_recording() {
                    self.recorder.stop_record();
                    self.push_system_info(format!("Content recorded on \"{}\"", filename));
                } else {
                    match self.recorder.start_record() {
                        Ok(_) => self
                            .push_system_info(format!("Recording content on \"{}\"...", filename)),
                        Err(err) => self.push_system_error(format!(
                            "Cannot start record on \"{}\": {}",
                            filename, err
                        )),
                    }
                }
            }

            Message::ClearLog => {
                self.log.clear();
                self.auto_scroll = true;
                self.scroll_x = 0.0;
                self.scroll_y = 1.0;
                return self.snap_to_end();
            }

            Message::EngineEvent(evt) => match evt {
                EngineEvent::ConnectionState(s) => {
                    self.connection = s;
                }
                EngineEvent::Message(m) => {
                    let (kind, prefix) = match m.direction {
                        Direction::Rx => (LogKind::Rx, "[RX]"),
                        Direction::Tx => (LogKind::Tx, "[TX]"),
                        Direction::System => (LogKind::Sys, "[SYS]"),
                    };

                    let segments = bytes_to_ansi_segments(&m.bytes)
                        .into_iter()
                        .map(|s| LogSegment {
                            text: s.text,
                            kind: s.kind,
                            color: s.color,
                        })
                        .collect::<Vec<_>>();

                    if let Some(cmd) = self.add_log_line(kind, m.at, prefix, segments) {
                        return cmd;
                    }
                }
                EngineEvent::Error(e) => {
                    let segments = vec![LogSegment {
                        text: e,
                        kind: SegmentKind::Plain,
                        color: AnsiColor::Red,
                    }];
                    if let Some(cmd) =
                        self.add_log_line(LogKind::Err, Local::now(), "[ERR]", segments)
                    {
                        return cmd;
                    }
                }
            },
        }

        iced::Command::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let status = match self.connection {
            ConnectionState::Disconnected => "Disconnected",
            ConnectionState::Connecting => "Connecting...",
            ConnectionState::Connected => "Connected",
        };

        let header = row![
            text("Scope (GUI)").size(22),
            text(format!("Status: {status}")),
            text(format!("File: {}", self.typewriter.get_filename())),
            text(format!("Size: {}", self.typewriter.get_size())),
            text(if self.recorder.is_recording() {
                format!("REC {}", self.recorder.get_size())
            } else {
                "".to_string()
            }),
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
            checkbox("Append CRLF", self.append_crlf).on_toggle(Message::AppendCrlfToggled),
            checkbox("Auto-scroll", self.auto_scroll).on_toggle(Message::AutoScrollToggled),
            button("Jump start").on_press(Message::JumpToStart),
            button("Jump end").on_press(Message::JumpToEnd),
            button("Save").on_press(Message::SaveHistory),
            button("Record").on_press(Message::ToggleRecord),
            button("Clear").on_press(Message::ClearLog),
        ]
        .spacing(12);

        let meta_color = iced::Color::from_rgb8(0x88, 0x88, 0x88);
        let monospace = iced::Font::MONOSPACE;

        let log_column = self.log.iter().fold(column![], |c, line| {
            let segs_row = line.segments.iter().fold(row![].spacing(0), |r, seg| {
                let color = color_for(seg.color, seg.kind);
                r.push(text(&seg.text).font(monospace).style(color))
            });

            let prefix_color = match line.kind {
                LogKind::Rx => iced::Color::from_rgb8(0x7a, 0xd7, 0xf0),
                LogKind::Tx => iced::Color::from_rgb8(0x8a, 0xf7, 0xa6),
                LogKind::Sys => meta_color,
                LogKind::Err => iced::Color::from_rgb8(0xf2, 0x5f, 0x5c),
            };

            let prefix = format!("{:<5}", line.prefix);

            c.push(
                row![
                    text(&line.timestamp).font(monospace).style(meta_color),
                    text(prefix).font(monospace).style(prefix_color),
                    segs_row,
                ]
                .spacing(12)
                .width(Length::Shrink),
            )
        });

        let log_view = scrollable(log_column)
            .id(self.log_scroll_id.clone())
            .on_scroll(Message::LogScrolled)
            .direction(scrollable::Direction::Both {
                vertical: scrollable::Properties::new(),
                horizontal: scrollable::Properties::new(),
            })
            .height(Length::Fill)
            .width(Length::Fill);

        let input = row![
            text_input("Type and press Enter to send...", &self.input)
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

fn color_for(color: AnsiColor, kind: SegmentKind) -> iced::Color {
    match (color, kind) {
        (AnsiColor::Reset, SegmentKind::Plain) => iced::Color::WHITE,
        (AnsiColor::Reset, SegmentKind::Escape) => iced::Color::from_rgb8(0xE6, 0xC2, 0x2E),
        (AnsiColor::Black, _) => iced::Color::BLACK,
        (AnsiColor::Red, _) => iced::Color::from_rgb8(0xF2, 0x5F, 0x5C),
        (AnsiColor::Green, _) => iced::Color::from_rgb8(0x8A, 0xF7, 0xA6),
        (AnsiColor::Yellow, _) => iced::Color::from_rgb8(0xE6, 0xC2, 0x2E),
        (AnsiColor::Blue, _) => iced::Color::from_rgb8(0x70, 0xA1, 0xFF),
        (AnsiColor::Magenta, _) => iced::Color::from_rgb8(0xC7, 0x7D, 0xFF),
        (AnsiColor::Cyan, _) => iced::Color::from_rgb8(0x7A, 0xD7, 0xF0),
        (AnsiColor::White, _) => iced::Color::WHITE,
        (AnsiColor::DarkGray, _) => iced::Color::from_rgb8(0x88, 0x88, 0x88),
        (AnsiColor::LightGreen, _) => iced::Color::from_rgb8(0xB8, 0xF2, 0xA6),
    }
}
