use chrono::{DateTime, Local};
use egui::{Color32, Key, RichText, ScrollArea, TextStyle};
use serialport::{DataBits, FlowControl, Parity, StopBits};
use std::time::{Duration, Instant};

use super::serial_worker::{
    SerialCommand, SerialConfig, SerialEvent, SerialHandle, spawn_serial_worker,
};

const COMMON_BAUD_RATES: &[u32] = &[
    300, 1200, 2400, 4800, 9600, 14400, 19200, 28800, 38400, 57600, 115200, 230400, 460800, 921600,
];

const MAX_ENTRIES: usize = 5000;
const PORT_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Copy, Clone, PartialEq, Eq)]
enum SendMode {
    Ascii,
    Hex,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum LineEnding {
    None,
    Cr,
    Lf,
    CrLf,
}

impl LineEnding {
    fn bytes(self) -> &'static [u8] {
        match self {
            LineEnding::None => b"",
            LineEnding::Cr => b"\r",
            LineEnding::Lf => b"\n",
            LineEnding::CrLf => b"\r\n",
        }
    }

    fn label(self) -> &'static str {
        match self {
            LineEnding::None => "None",
            LineEnding::Cr => "CR (\\r)",
            LineEnding::Lf => "LF (\\n)",
            LineEnding::CrLf => "CRLF (\\r\\n)",
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum DisplayMode {
    Ascii,
    Hex,
    HexDump,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum EntryKind {
    Rx,
    Tx,
    System,
    SystemError,
}

struct LogEntry {
    timestamp: DateTime<Local>,
    kind: EntryKind,
    bytes: Vec<u8>,
    message: Option<String>,
}

pub struct GuiApp {
    available_ports: Vec<String>,
    last_port_refresh: Instant,

    selected_port: String,
    baud_rate: u32,
    custom_baud_str: String,
    use_custom_baud: bool,
    data_bits: DataBits,
    stop_bits: StopBits,
    parity: Parity,
    flow_control: FlowControl,

    connected: bool,
    connecting: bool,

    send_input: String,
    send_mode: SendMode,
    line_ending: LineEnding,

    entries: Vec<LogEntry>,
    display_mode: DisplayMode,
    show_timestamps: bool,
    auto_scroll: bool,
    show_tx_in_log: bool,

    bytes_rx: u64,
    bytes_tx: u64,

    status: String,

    serial: SerialHandle,
}

impl GuiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let serial = spawn_serial_worker(cc.egui_ctx.clone());
        let available_ports = list_ports();
        let selected_port = available_ports.first().cloned().unwrap_or_default();

        Self {
            available_ports,
            last_port_refresh: Instant::now(),
            selected_port,
            baud_rate: 115200,
            custom_baud_str: "115200".to_string(),
            use_custom_baud: false,
            data_bits: DataBits::Eight,
            stop_bits: StopBits::One,
            parity: Parity::None,
            flow_control: FlowControl::None,
            connected: false,
            connecting: false,
            send_input: String::new(),
            send_mode: SendMode::Ascii,
            line_ending: LineEnding::CrLf,
            entries: Vec::new(),
            display_mode: DisplayMode::Ascii,
            show_timestamps: true,
            auto_scroll: true,
            show_tx_in_log: true,
            bytes_rx: 0,
            bytes_tx: 0,
            status: "Idle".to_string(),
            serial,
        }
    }

    fn refresh_ports(&mut self) {
        self.available_ports = list_ports();
        if !self.available_ports.contains(&self.selected_port) {
            self.selected_port = self.available_ports.first().cloned().unwrap_or_default();
        }
    }

    fn drain_serial_events(&mut self) {
        while let Ok(evt) = self.serial.event_rx.try_recv() {
            match evt {
                SerialEvent::Connected { port, baud_rate } => {
                    self.connected = true;
                    self.connecting = false;
                    self.status = format!("Connected to {} @ {}bps", port, baud_rate);
                    self.push_system(format!("Connected to {} @ {}bps", port, baud_rate));
                }
                SerialEvent::Disconnected => {
                    self.connected = false;
                    self.connecting = false;
                    self.status = "Disconnected".to_string();
                    self.push_system("Disconnected".to_string());
                }
                SerialEvent::Error(msg) => {
                    self.connecting = false;
                    self.status = format!("Error: {}", msg);
                    self.push_system_error(msg);
                }
                SerialEvent::RxLine { timestamp, bytes } => {
                    self.bytes_rx += bytes.len() as u64;
                    self.entries.push(LogEntry {
                        timestamp,
                        kind: EntryKind::Rx,
                        bytes,
                        message: None,
                    });
                    self.trim_log();
                }
                SerialEvent::TxEcho { timestamp, bytes } => {
                    self.bytes_tx += bytes.len() as u64;
                    if self.show_tx_in_log {
                        self.entries.push(LogEntry {
                            timestamp,
                            kind: EntryKind::Tx,
                            bytes,
                            message: None,
                        });
                        self.trim_log();
                    }
                }
            }
        }
    }

    fn push_system(&mut self, msg: String) {
        self.entries.push(LogEntry {
            timestamp: Local::now(),
            kind: EntryKind::System,
            bytes: Vec::new(),
            message: Some(msg),
        });
        self.trim_log();
    }

    fn push_system_error(&mut self, msg: String) {
        self.entries.push(LogEntry {
            timestamp: Local::now(),
            kind: EntryKind::SystemError,
            bytes: Vec::new(),
            message: Some(msg),
        });
        self.trim_log();
    }

    fn trim_log(&mut self) {
        if self.entries.len() > MAX_ENTRIES {
            let excess = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(..excess);
        }
    }

    fn build_send_payload(&self) -> Result<Vec<u8>, String> {
        let mut payload = match self.send_mode {
            SendMode::Ascii => self.send_input.as_bytes().to_vec(),
            SendMode::Hex => parse_hex_input(&self.send_input)?,
        };
        payload.extend_from_slice(self.line_ending.bytes());
        Ok(payload)
    }

    fn try_send(&mut self) {
        if !self.connected {
            self.push_system_error("Cannot send: not connected".to_string());
            return;
        }
        match self.build_send_payload() {
            Ok(payload) if payload.is_empty() => {}
            Ok(payload) => {
                let _ = self.serial.cmd_tx.send(SerialCommand::Send(payload));
                self.send_input.clear();
            }
            Err(e) => {
                self.push_system_error(format!("Send error: {}", e));
            }
        }
    }

    fn current_config(&self) -> SerialConfig {
        SerialConfig {
            port: self.selected_port.clone(),
            baud_rate: self.baud_rate,
            data_bits: self.data_bits,
            stop_bits: self.stop_bits,
            parity: self.parity,
            flow_control: self.flow_control,
        }
    }

    fn settings_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("Port:");
            let port_label = if self.selected_port.is_empty() {
                "<no ports>".to_string()
            } else {
                self.selected_port.clone()
            };
            egui::ComboBox::from_id_salt("port_combo")
                .selected_text(port_label)
                .show_ui(ui, |ui| {
                    if self.available_ports.is_empty() {
                        ui.label("No serial ports detected");
                    }
                    for p in self.available_ports.clone() {
                        ui.selectable_value(&mut self.selected_port, p.clone(), p);
                    }
                });
            if ui.button("Refresh").clicked() {
                self.refresh_ports();
            }

            ui.separator();

            ui.label("Baud:");
            let baud_text = if self.use_custom_baud {
                "Custom".to_string()
            } else {
                self.baud_rate.to_string()
            };
            egui::ComboBox::from_id_salt("baud_combo")
                .selected_text(baud_text)
                .show_ui(ui, |ui| {
                    for &b in COMMON_BAUD_RATES {
                        if ui
                            .selectable_label(!self.use_custom_baud && self.baud_rate == b, b.to_string())
                            .clicked()
                        {
                            self.baud_rate = b;
                            self.custom_baud_str = b.to_string();
                            self.use_custom_baud = false;
                        }
                    }
                    if ui
                        .selectable_label(self.use_custom_baud, "Custom...")
                        .clicked()
                    {
                        self.use_custom_baud = true;
                    }
                });
            if self.use_custom_baud {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.custom_baud_str)
                        .desired_width(80.0)
                        .hint_text("baud"),
                );
                if resp.changed() {
                    if let Ok(b) = self.custom_baud_str.parse::<u32>()
                        && b > 0
                    {
                        self.baud_rate = b;
                    }
                }
            }

            ui.separator();

            let connect_label = if self.connected {
                "Disconnect"
            } else if self.connecting {
                "Connecting..."
            } else {
                "Connect"
            };
            let can_act = !self.selected_port.is_empty() && self.baud_rate > 0;
            let btn = ui.add_enabled(can_act && !self.connecting, egui::Button::new(connect_label));
            if btn.clicked() {
                if self.connected {
                    let _ = self.serial.cmd_tx.send(SerialCommand::Disconnect);
                } else {
                    self.connecting = true;
                    self.status = format!("Opening {}...", self.selected_port);
                    let cfg = self.current_config();
                    let _ = self.serial.cmd_tx.send(SerialCommand::Connect(cfg));
                }
            }

            ui.separator();
            let (dot, color) = if self.connected {
                ("●", Color32::from_rgb(80, 200, 80))
            } else if self.connecting {
                ("●", Color32::from_rgb(220, 180, 60))
            } else {
                ("●", Color32::from_rgb(200, 80, 80))
            };
            ui.label(RichText::new(dot).color(color));
        });

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("Data bits:");
            egui::ComboBox::from_id_salt("databits_combo")
                .selected_text(data_bits_label(self.data_bits))
                .show_ui(ui, |ui| {
                    for v in [DataBits::Five, DataBits::Six, DataBits::Seven, DataBits::Eight] {
                        ui.selectable_value(&mut self.data_bits, v, data_bits_label(v));
                    }
                });

            ui.separator();
            ui.label("Stop bits:");
            egui::ComboBox::from_id_salt("stopbits_combo")
                .selected_text(stop_bits_label(self.stop_bits))
                .show_ui(ui, |ui| {
                    for v in [StopBits::One, StopBits::Two] {
                        ui.selectable_value(&mut self.stop_bits, v, stop_bits_label(v));
                    }
                });

            ui.separator();
            ui.label("Parity:");
            egui::ComboBox::from_id_salt("parity_combo")
                .selected_text(parity_label(self.parity))
                .show_ui(ui, |ui| {
                    for v in [Parity::None, Parity::Odd, Parity::Even] {
                        ui.selectable_value(&mut self.parity, v, parity_label(v));
                    }
                });

            ui.separator();
            ui.label("Flow:");
            egui::ComboBox::from_id_salt("flow_combo")
                .selected_text(flow_label(self.flow_control))
                .show_ui(ui, |ui| {
                    for v in [
                        FlowControl::None,
                        FlowControl::Software,
                        FlowControl::Hardware,
                    ] {
                        ui.selectable_value(&mut self.flow_control, v, flow_label(v));
                    }
                });

            if self.connected {
                ui.separator();
                if ui.button("Apply").clicked() {
                    let cfg = self.current_config();
                    let _ = self.serial.cmd_tx.send(SerialCommand::Disconnect);
                    self.connecting = true;
                    let _ = self.serial.cmd_tx.send(SerialCommand::Connect(cfg));
                }
            }
        });
        ui.add_space(2.0);
    }

    fn log_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("View:");
            ui.selectable_value(&mut self.display_mode, DisplayMode::Ascii, "ASCII");
            ui.selectable_value(&mut self.display_mode, DisplayMode::Hex, "Hex");
            ui.selectable_value(&mut self.display_mode, DisplayMode::HexDump, "Hex+ASCII");
            ui.separator();
            ui.checkbox(&mut self.show_timestamps, "Timestamps");
            ui.checkbox(&mut self.auto_scroll, "Auto-scroll");
            ui.checkbox(&mut self.show_tx_in_log, "Echo TX");
            ui.separator();
            if ui.button("Clear").clicked() {
                self.entries.clear();
                self.bytes_rx = 0;
                self.bytes_tx = 0;
            }
        });
    }

    fn log_view(&self, ui: &mut egui::Ui) {
        let row_height = ui.text_style_height(&TextStyle::Monospace);
        let mut area = ScrollArea::vertical().auto_shrink([false, false]);
        if self.auto_scroll {
            area = area.stick_to_bottom(true);
        }
        area.show_rows(ui, row_height, self.entries.len(), |ui, range| {
            for entry in &self.entries[range] {
                self.render_entry(ui, entry);
            }
        });
    }

    fn render_entry(&self, ui: &mut egui::Ui, entry: &LogEntry) {
        let ts = if self.show_timestamps {
            format!("[{}] ", entry.timestamp.format("%H:%M:%S%.3f"))
        } else {
            String::new()
        };

        let (prefix, color) = match entry.kind {
            EntryKind::Rx => ("← ", Color32::from_rgb(180, 220, 255)),
            EntryKind::Tx => ("→ ", Color32::from_rgb(180, 255, 180)),
            EntryKind::System => ("· ", Color32::from_rgb(160, 160, 160)),
            EntryKind::SystemError => ("! ", Color32::from_rgb(255, 120, 120)),
        };

        let body = match entry.kind {
            EntryKind::System | EntryKind::SystemError => {
                entry.message.clone().unwrap_or_default()
            }
            EntryKind::Rx | EntryKind::Tx => format_bytes(&entry.bytes, self.display_mode),
        };

        let line = format!("{}{}{}", ts, prefix, body);
        ui.label(RichText::new(line).monospace().color(color));
    }

    fn send_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Mode:");
            ui.selectable_value(&mut self.send_mode, SendMode::Ascii, "ASCII");
            ui.selectable_value(&mut self.send_mode, SendMode::Hex, "Hex");
            ui.separator();
            ui.label("Line ending:");
            egui::ComboBox::from_id_salt("line_ending_combo")
                .selected_text(self.line_ending.label())
                .show_ui(ui, |ui| {
                    for v in [
                        LineEnding::None,
                        LineEnding::Cr,
                        LineEnding::Lf,
                        LineEnding::CrLf,
                    ] {
                        ui.selectable_value(&mut self.line_ending, v, v.label());
                    }
                });
            ui.separator();
            ui.label(
                RichText::new("Ctrl+Enter to send")
                    .small()
                    .color(Color32::GRAY),
            );
        });

        ui.add_space(2.0);

        ui.horizontal(|ui| {
            let hint = match self.send_mode {
                SendMode::Ascii => "Type text to send...",
                SendMode::Hex => "Hex bytes, e.g. A0 B1 0F or A0B10F",
            };
            let resp = ui.add(
                egui::TextEdit::multiline(&mut self.send_input)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text(hint)
                    .font(TextStyle::Monospace),
            );

            let send_with_ctrl_enter = resp.has_focus()
                && ui.input(|i| i.modifiers.ctrl && i.key_pressed(Key::Enter));
            if send_with_ctrl_enter {
                self.try_send();
            }
        });

        ui.horizontal(|ui| {
            let send_btn = ui.add_enabled(
                self.connected && !self.send_input.is_empty(),
                egui::Button::new("Send"),
            );
            if send_btn.clicked() {
                self.try_send();
            }
            if ui.button("Clear input").clicked() {
                self.send_input.clear();
            }
            if self.send_mode == SendMode::Hex {
                match parse_hex_input(&self.send_input) {
                    Ok(bytes) if !bytes.is_empty() => {
                        ui.label(
                            RichText::new(format!("{} byte(s)", bytes.len()))
                                .small()
                                .color(Color32::GRAY),
                        );
                    }
                    Ok(_) => {}
                    Err(e) => {
                        ui.label(RichText::new(e).small().color(Color32::from_rgb(255, 120, 120)));
                    }
                }
            }
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&self.status).small());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("RX: {} B   TX: {} B", self.bytes_rx, self.bytes_tx))
                        .small(),
                );
            });
        });
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.last_port_refresh.elapsed() >= PORT_REFRESH_INTERVAL && !self.connected {
            self.refresh_ports();
            self.last_port_refresh = Instant::now();
        }

        self.drain_serial_events();

        egui::TopBottomPanel::top("settings_panel")
            .resizable(false)
            .show(ctx, |ui| self.settings_panel(ui));

        egui::TopBottomPanel::bottom("status_bar")
            .resizable(false)
            .show(ctx, |ui| self.status_bar(ui));

        egui::TopBottomPanel::bottom("send_panel")
            .resizable(true)
            .min_height(80.0)
            .show(ctx, |ui| {
                ui.add_space(2.0);
                self.send_panel(ui);
                ui.add_space(2.0);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.log_toolbar(ui);
            ui.separator();
            self.log_view(ui);
        });

        if self.connecting || self.connected {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.serial.cmd_tx.send(SerialCommand::Shutdown);
    }
}

fn list_ports() -> Vec<String> {
    serialport::available_ports()
        .map(|ports| ports.into_iter().map(|p| p.port_name).collect())
        .unwrap_or_default()
}

fn data_bits_label(d: DataBits) -> &'static str {
    match d {
        DataBits::Five => "5",
        DataBits::Six => "6",
        DataBits::Seven => "7",
        DataBits::Eight => "8",
    }
}

fn stop_bits_label(s: StopBits) -> &'static str {
    match s {
        StopBits::One => "1",
        StopBits::Two => "2",
    }
}

fn parity_label(p: Parity) -> &'static str {
    match p {
        Parity::None => "None",
        Parity::Odd => "Odd",
        Parity::Even => "Even",
    }
}

fn flow_label(f: FlowControl) -> &'static str {
    match f {
        FlowControl::None => "None",
        FlowControl::Software => "XON/XOFF",
        FlowControl::Hardware => "RTS/CTS",
    }
}

fn parse_hex_input(s: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != ':' && *c != '-')
        .collect();
    let cleaned = cleaned
        .strip_prefix("0x")
        .or_else(|| cleaned.strip_prefix("0X"))
        .unwrap_or(&cleaned)
        .to_string();

    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
    if cleaned.len() % 2 != 0 {
        return Err("Hex input must have an even number of digits".to_string());
    }
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    let bytes = cleaned.as_bytes();
    for chunk in bytes.chunks(2) {
        let pair = std::str::from_utf8(chunk).map_err(|_| "Invalid characters in hex".to_string())?;
        let b = u8::from_str_radix(pair, 16)
            .map_err(|_| format!("Invalid hex byte: {}", pair))?;
        out.push(b);
    }
    Ok(out)
}

fn format_bytes(bytes: &[u8], mode: DisplayMode) -> String {
    match mode {
        DisplayMode::Ascii => format_ascii(bytes),
        DisplayMode::Hex => format_hex(bytes),
        DisplayMode::HexDump => format_hex_dump(bytes),
    }
}

fn format_ascii(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            b'\r' => out.push_str("\\r"),
            b'\n' => {}
            0x09 => out.push('\t'),
            0x20..=0x7E => out.push(b as char),
            _ => out.push_str(&format!("\\x{:02X}", b)),
        }
    }
    out
}

fn format_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{:02X}", b));
    }
    out
}

fn format_hex_dump(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 3);
    let mut ascii = String::with_capacity(bytes.len());
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            hex.push(' ');
        }
        hex.push_str(&format!("{:02X}", b));
        ascii.push(if (0x20..=0x7E).contains(b) {
            *b as char
        } else {
            '.'
        });
    }
    format!("{}  |{}|", hex, ascii)
}
