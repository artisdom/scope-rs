use super::config_panel::ConfigPanel;
use super::message::{Message, PortInfo};
use super::port_list_dialog::PortListDialog;
use super::styles::{
    button_style, container_style, menu_button_style, success_button_style,
    danger_button_style, BACKGROUND_COLOR, ERROR_COLOR, SUCCESS_COLOR,
    TEXT_SECONDARY_COLOR,
};
use super::terminal_view::TerminalView;
use crate::infra::logger::Logger;
use crate::infra::messages::TimedBytes;
use crate::infra::mpmc::{Channel, Consumer};
use crate::plugin::engine::{PluginEngine, PluginEngineConnections, PluginEngineCommand};
use crate::serial::serial_if::{
    SerialCommand, SerialConnections, SerialInterface, SerialMode, SerialSetup, SerialShared,
};
use crate::infra::task::Shared;
use chrono::Local;
use iced::{
    Background, Element, Length, Padding, Result, Subscription, Theme,
    task::Task,
    widget::{button, column, container, row, text, Space},
};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::Duration;

pub struct ScopeApp {
    // UI State
    config_panel: ConfigPanel,
    port_list_dialog: PortListDialog,
    terminal_view: TerminalView,
    
    // Connection State
    is_connected: bool,
    connection_status: ConnectionStatus,
    
    // Application Settings
    capacity: usize,
    tag_file: PathBuf,
    latency: u64,
    
    // Backend Components
    serial_if: Option<SerialInterface>,
    serial_shared: Option<Shared<SerialShared>>,
    tx_channel: Option<Arc<Channel<Arc<TimedBytes>>>>,
    rx_channel: Option<Arc<Channel<Arc<TimedBytes>>>>,
    rx_consumer: Option<Consumer<Arc<TimedBytes>>>,
    plugin_engine: Option<PluginEngine>,
    
    // Channels for communication
    serial_cmd_sender: Option<std::sync::mpsc::Sender<SerialCommand>>,
    plugin_cmd_sender: Option<std::sync::mpsc::Sender<PluginEngineCommand>>,
    
    // Logger
    logger: Logger,
    
    // Status
    status_message: String,
    history_len: usize,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Error(String),
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        ConnectionStatus::Disconnected
    }
}

impl ScopeApp {
    pub fn new() -> Self {
        Self {
            config_panel: ConfigPanel::new(),
            port_list_dialog: PortListDialog::new(),
            terminal_view: TerminalView::new(),
            is_connected: false,
            connection_status: ConnectionStatus::Disconnected,
            capacity: 2000,
            tag_file: PathBuf::from("tags.yml"),
            latency: 500,
            serial_if: None,
            serial_shared: None,
            tx_channel: None,
            rx_channel: None,
            rx_consumer: None,
            plugin_engine: None,
            serial_cmd_sender: None,
            plugin_cmd_sender: None,
            logger: Logger::new("gui".to_string()).0,
            status_message: "Ready".to_string(),
            history_len: 0,
        }
    }
    
    pub fn with_settings(mut self, settings: SerialSetup, capacity: usize, tag_file: PathBuf, latency: u64) -> Self {
        self.config_panel = ConfigPanel::from_setup(settings);
        self.capacity = capacity;
        self.tag_file = tag_file;
        self.latency = latency;
        self
    }
    
    fn initialize_backend(&mut self) {
        let mut tx_channel = Channel::default();
        let mut rx_channel = Channel::default();
        
        let tx_consumer = tx_channel.new_consumer();
        let tx_consumer2 = tx_channel.new_consumer();
        let _tx_consumer3 = tx_channel.new_consumer();
        let rx_consumer = rx_channel.new_consumer();
        let rx_consumer_gui = rx_channel.new_consumer();  // GUI consumer for receiving data
        
        let tx_channel = Arc::new(tx_channel);
        let rx_channel = Arc::new(rx_channel);
        
        let tx_producer = tx_channel.clone().new_producer();
        let rx_producer = rx_channel.clone().new_producer();
        
        let (serial_cmd_sender, serial_cmd_receiver) = channel();
        let (plugin_cmd_sender, plugin_cmd_receiver) = channel();
        
        // Initialize serial interface
        let serial_connections = SerialConnections::new(
            self.logger.clone().with_source("serial".to_string()),
            tx_consumer,
            rx_producer,
            plugin_cmd_sender.clone(),
            self.latency,
        );
        
        let serial_if = SerialInterface::spawn_serial_interface(
            serial_connections,
            serial_cmd_sender.clone(),
            serial_cmd_receiver,
            SerialSetup::default(),
        );
        
        let serial_shared = serial_if.shared_ref();
        let serial_shared_for_plugin = serial_if.shared_ref();
        
        // Initialize plugin engine
        let plugin_connections = PluginEngineConnections::new(
            self.logger.clone().with_source("plugin".to_string()),
            tx_producer,
            tx_consumer2,
            rx_consumer,
            serial_shared_for_plugin,
            self.latency,
        );
        
        let plugin_engine = PluginEngine::spawn_plugin_engine(
            plugin_connections,
            plugin_cmd_sender.clone(),
            plugin_cmd_receiver,
        );
        
        self.serial_if = Some(serial_if);
        self.serial_shared = Some(serial_shared);
        self.tx_channel = Some(tx_channel);
        self.rx_channel = Some(rx_channel);
        self.rx_consumer = Some(rx_consumer_gui);  // Store GUI consumer
        self.plugin_engine = Some(plugin_engine);
        self.serial_cmd_sender = Some(serial_cmd_sender);
        self.plugin_cmd_sender = Some(plugin_cmd_sender);
    }
    
    fn connect_serial(&mut self) {
        if self.serial_cmd_sender.is_none() {
            self.initialize_backend();
        }
        
        // Send setup command
        if let Some(ref sender) = self.serial_cmd_sender {
            let setup = self.config_panel.to_setup();
            let _ = sender.send(SerialCommand::Setup(setup));
            let _ = sender.send(SerialCommand::Connect);
        }
        
        self.connection_status = ConnectionStatus::Connecting;
        self.status_message = "Connecting...".to_string();
    }
    
    fn disconnect_serial(&mut self) {
        if let Some(ref sender) = self.serial_cmd_sender {
            let _ = sender.send(SerialCommand::Disconnect);
        }
        
        self.is_connected = false;
        self.connection_status = ConnectionStatus::Disconnected;
        self.status_message = "Disconnected".to_string();
    }
    
    fn refresh_ports(&mut self) -> Task<Message> {
        self.port_list_dialog.refresh();
        
        Task::perform(
            async {
                match serialport::available_ports() {
                    Ok(ports) => {
                        let port_infos: Vec<PortInfo> = ports
                            .into_iter()
                            .filter(|p| matches!(p.port_type, serialport::SerialPortType::UsbPort(_)))
                            .map(PortInfo::from)
                            .collect();
                        port_infos
                    }
                    Err(_) => Vec::new(),
                }
            },
            Message::PortsRefreshed,
        )
    }
    
    fn send_command(&mut self) {
        match self.terminal_view.input_mode {
            super::terminal_view::InputMode::Ascii => {
                let command = self.terminal_view.input_buffer.clone();
                if command.is_empty() {
                    return;
                }
                
                // Prepare data to send
                let data = format!("{}\n", command).into_bytes();
                let send_data = if self.terminal_view.mux_mode {
                    super::terminal_view::encode_mux_frame(&data, self.terminal_view.mux_link_id)
                } else {
                    data
                };
                
                // Add to terminal view
                self.terminal_view.add_sent_data(
                    &format!("{}\n", command),
                    Some(Local::now().format("%H:%M:%S").to_string()),
                );
                
                // Send to serial
                if let Some(ref tx) = self.tx_channel {
                    let producer = Arc::clone(tx).new_producer();
                    producer.produce(Arc::new(TimedBytes {
                        timestamp: Local::now(),
                        message: send_data,
                    }));
                }
                
                self.terminal_view.input_buffer.clear();
            }
            super::terminal_view::InputMode::Hex => {
                // Parse hex input
                let bytes = self.terminal_view.get_hex_bytes();
                if bytes.is_empty() {
                    return;
                }
                
                // Prepare data to send
                let send_data = if self.terminal_view.mux_mode {
                    super::terminal_view::encode_mux_frame(&bytes, self.terminal_view.mux_link_id)
                } else {
                    bytes.clone()
                };
                
                // Add to terminal view
                self.terminal_view.add_sent_bytes(
                    &bytes,
                    Some(Local::now().format("%H:%M:%S").to_string()),
                );
                
                // Send to serial
                if let Some(ref tx) = self.tx_channel {
                    let producer = Arc::clone(tx).new_producer();
                    producer.produce(Arc::new(TimedBytes {
                        timestamp: Local::now(),
                        message: send_data,
                    }));
                }
                
                self.terminal_view.clear_hex();
            }
        }
        
        self.history_len += 1;
    }
    
    fn update_connection_status(&mut self) {
        if let Some(ref shared) = self.serial_shared {
            if let Ok(guard) = shared.read() {
                match guard.mode {
                    SerialMode::Connected => {
                        self.is_connected = true;
                        self.connection_status = ConnectionStatus::Connected;
                        self.status_message = format!("Connected to {} @ {} bps", guard.port, guard.baudrate);
                    }
                    SerialMode::Reconnecting => {
                        self.is_connected = false;
                        self.connection_status = ConnectionStatus::Reconnecting;
                        self.status_message = "Reconnecting...".to_string();
                    }
                    SerialMode::DoNotConnect => {
                        self.is_connected = false;
                        if !matches!(self.connection_status, ConnectionStatus::Disconnected) {
                            self.connection_status = ConnectionStatus::Disconnected;
                            self.status_message = "Disconnected".to_string();
                        }
                    }
                }
            }
        }
    }
}

pub fn update(app: &mut ScopeApp, message: Message) -> Task<Message> {
    match message {
        // Serial connection
        Message::ConnectSerial => {
            app.connect_serial();
        }
        Message::DisconnectSerial => {
            app.disconnect_serial();
        }
        Message::SerialConnected => {
            app.is_connected = true;
            app.connection_status = ConnectionStatus::Connected;
        }
        Message::SerialDisconnected => {
            app.is_connected = false;
            app.connection_status = ConnectionStatus::Disconnected;
        }
        
        // Serial configuration
        Message::PortChanged(port) => {
            app.config_panel.port = port;
        }
        Message::BaudrateChanged(s) => {
            if let Ok(b) = s.parse() {
                app.config_panel.baudrate = b;
            }
            app.config_panel.baudrate_input = s;
        }
        Message::DataBitsChanged(db) => {
            app.config_panel.data_bits = db;
        }
        Message::ParityChanged(p) => {
            app.config_panel.parity = p;
        }
        Message::StopBitsChanged(sb) => {
            app.config_panel.stop_bits = sb;
        }
        Message::FlowControlChanged(fc) => {
            app.config_panel.flow_control = fc;
        }
        
        // Port list dialog
        Message::ShowPortListDialog => {
            app.port_list_dialog.show();
            return app.refresh_ports();
        }
        Message::HidePortListDialog => {
            app.port_list_dialog.hide();
        }
        Message::RefreshPorts => {
            return app.refresh_ports();
        }
        Message::SelectPort(port) => {
            app.config_panel.port = port.clone();
            app.port_list_dialog.selected_port = Some(port);
            app.port_list_dialog.hide();
        }
        Message::PortsRefreshed(ports) => {
            app.port_list_dialog.set_ports(ports);
        }
        
        // Configuration panel
        Message::ShowConfigPanel => {
            app.config_panel.show();
        }
        Message::HideConfigPanel => {
            app.config_panel.hide();
        }
        Message::CapacityChanged(s) => {
            if let Ok(c) = s.parse() {
                app.config_panel.capacity = c;
            }
            app.config_panel.capacity_input = s;
        }
        Message::TagFileChanged(s) => {
            app.config_panel.tag_file = s;
        }
        Message::LatencyChanged(s) => {
            if let Ok(l) = s.parse::<u64>() {
                app.config_panel.latency = l.clamp(0, 100_000);
            }
            app.config_panel.latency_input = s;
        }
        Message::ApplyConfig => {
            app.capacity = app.config_panel.capacity;
            app.tag_file = PathBuf::from(&app.config_panel.tag_file);
            app.latency = app.config_panel.latency;
            
            // Apply serial settings if connected
            if let Some(ref sender) = app.serial_cmd_sender {
                let setup = app.config_panel.to_setup();
                let _ = sender.send(SerialCommand::Setup(setup));
            }
            
            app.status_message = "Settings applied".to_string();
        }
        
        // Terminal
        Message::TerminalInput(s) => {
            app.terminal_view.input_buffer = s;
        }
        Message::SendCommand => {
            app.send_command();
        }
        Message::ClearTerminal => {
            app.terminal_view.clear();
        }
        Message::ScrollUp | Message::ScrollDown | Message::PageUp | Message::PageDown 
        | Message::JumpToStart | Message::JumpToEnd => {
            // Handle scrolling in terminal view
        }
        
        // Input mode switching
        Message::SwitchToAsciiMode => {
            app.terminal_view.input_mode = super::terminal_view::InputMode::Ascii;
            app.terminal_view.clear_hex();
        }
        Message::SwitchToHexMode => {
            app.terminal_view.input_mode = super::terminal_view::InputMode::Hex;
            app.terminal_view.input_buffer.clear();
        }
        Message::HexInput(s) => {
            app.terminal_view.hex_input_buffer = s.clone();
            // Parse and update hex bytes
            if let Some(bytes) = app.terminal_view.parse_hex_string(&s) {
                app.terminal_view.hex_bytes = bytes.iter()
                    .map(|b| super::terminal_view::HexByte { 
                        high: Some(format!("{:X}", b >> 4).chars().next().unwrap()),
                        low: Some(format!("{:X}", b & 0x0F).chars().next().unwrap()),
                    })
                    .collect();
            }
        }
        Message::QuickHex(hex) => {
            // Add quick hex bytes
            app.terminal_view.hex_input_buffer.push_str(&hex);
            let buffer = app.terminal_view.hex_input_buffer.clone();
            if let Some(bytes) = app.terminal_view.parse_hex_string(&buffer) {
                app.terminal_view.hex_bytes = bytes.iter()
                    .map(|b| super::terminal_view::HexByte { 
                        high: Some(format!("{:X}", b >> 4).chars().next().unwrap()),
                        low: Some(format!("{:X}", b & 0x0F).chars().next().unwrap()),
                    })
                    .collect();
            }
        }
        Message::ClearHexInput => {
            app.terminal_view.clear_hex();
        }
        
        // Multiplexing protocol mode
        Message::ToggleMuxMode => {
            app.terminal_view.mux_mode = !app.terminal_view.mux_mode;
        }
        Message::MuxLinkIdChanged(s) => {
            app.terminal_view.mux_link_id_input = s.clone();
            // Parse hex value
            if let Ok(id) = u8::from_str_radix(&s.trim().trim_start_matches("0x"), 16) {
                app.terminal_view.mux_link_id = id;
            }
        }
        Message::CopyMuxFrame(hex) => {
            // Copy to clipboard
            return iced::clipboard::write(hex);
        }
        
        // Search
        Message::ToggleSearchMode => {
            app.terminal_view.is_search_mode = !app.terminal_view.is_search_mode;
            if !app.terminal_view.is_search_mode {
                app.terminal_view.search_buffer.clear();
                app.terminal_view.search_results.clear();
            }
        }
        Message::SearchInput(s) => {
            app.terminal_view.search_buffer = s;
            app.terminal_view.update_search();
        }
        Message::SearchNext => {
            app.terminal_view.next_search_result();
        }
        Message::SearchPrev => {
            app.terminal_view.prev_search_result();
        }
        Message::ToggleCaseSensitive => {
            app.terminal_view.is_case_sensitive = !app.terminal_view.is_case_sensitive;
            app.terminal_view.update_search();
        }
        
        // Data operations
        Message::SaveData => {
            app.status_message = "Data saved".to_string();
        }
        Message::RecordData => {
            app.status_message = "Recording toggled".to_string();
        }
        Message::CopyToClipboard => {
            app.status_message = "Copied to clipboard".to_string();
        }
        
        // Plugin
        Message::ShowPluginDialog => {}
        Message::HidePluginDialog => {}
        Message::LoadPlugin(_) => {}
        Message::UnloadPlugin(_) => {}
        Message::PluginCommand(_, _, _) => {}
        
        // Application
        Message::Exit => {
            if let Some(ref sender) = app.serial_cmd_sender {
                let _ = sender.send(SerialCommand::Exit);
            }
            if let Some(ref sender) = app.plugin_cmd_sender {
                let _ = sender.send(PluginEngineCommand::Exit);
            }
        }
        Message::Tick => {
            app.update_connection_status();
            // Poll for received data
            if let Some(ref consumer) = app.rx_consumer {
                while let Ok(data) = consumer.try_recv() {
                    app.terminal_view.add_received_data(
                        &data.message,
                        Some(data.timestamp.format("%H:%M:%S").to_string()),
                    );
                }
            }
        }
        Message::DataReceived(data) => {
            app.terminal_view.add_received_data(
                &data,
                Some(Local::now().format("%H:%M:%S").to_string()),
            );
        }
        
        // Menu
        Message::MenuFile | Message::MenuSerial | Message::MenuHelp => {}
    }
    
    Task::none()
}

pub fn view(app: &ScopeApp) -> Element<'_, Message> {
    // Menu bar
    let menu_bar = row![
        button(text("File"))
            .on_press(Message::MenuFile)
            .style(menu_button_style),
        button(text("Serial"))
            .on_press(Message::MenuSerial)
            .style(menu_button_style),
        button(text("Help"))
            .on_press(Message::MenuHelp)
            .style(menu_button_style),
    ]
    .spacing(5)
    .padding(Padding::new(5.0));

    // Toolbar
    let toolbar = row![
        if app.is_connected {
            button(text("Disconnect"))
                .on_press(Message::DisconnectSerial)
                .style(danger_button_style)
        } else {
            button(text("Connect"))
                .on_press(Message::ConnectSerial)
                .style(success_button_style)
        },
        button(text("Settings"))
            .on_press(Message::ShowConfigPanel)
            .style(button_style),
        button(text("Clear"))
            .on_press(Message::ClearTerminal)
            .style(button_style),
        button(text("Search"))
            .on_press(Message::ToggleSearchMode)
            .style(button_style),
        Space::with_width(Length::Fill),
        button(text("Save"))
            .on_press(Message::SaveData)
            .style(button_style),
        button(text("Record"))
            .on_press(Message::RecordData)
            .style(button_style),
    ]
    .spacing(10)
    .padding(Padding::new(5.0));

    // Status bar
    let status_color = match &app.connection_status {
        ConnectionStatus::Connected => SUCCESS_COLOR,
        ConnectionStatus::Disconnected => TEXT_SECONDARY_COLOR,
        ConnectionStatus::Connecting => iced::Color::from_rgb(0.9, 0.7, 0.2),
        ConnectionStatus::Reconnecting => iced::Color::from_rgb(0.9, 0.7, 0.2),
        ConnectionStatus::Error(_) => ERROR_COLOR,
    };

    let status_bar = row![
        text(&app.status_message).style(move |_theme| text::Style {
            color: Some(status_color),
        }),
        Space::with_width(Length::Fill),
        text(format!("History: {}", app.history_len)).style(|_theme| text::Style {
            color: Some(TEXT_SECONDARY_COLOR),
        }),
    ]
    .spacing(10)
    .padding(Padding::new(5.0));

    // Main content
    let main_content = column![
        menu_bar,
        toolbar,
        container(app.terminal_view.view())
            .style(container_style)
            .padding(Padding::new(10.0))
            .height(Length::Fill)
            .width(Length::Fill),
        status_bar,
    ]
    .spacing(5)
    .height(Length::Fill)
    .width(Length::Fill);

    // Overlay dialogs
    let content: Element<Message> = if app.config_panel.is_visible {
        container(
            column![
                main_content,
                container(app.config_panel.view(app.is_connected))
                    .center_x(Length::Fill)
                    .padding(Padding::new(50.0)),
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .style(|_theme| container::Style {
            background: Some(Background::Color(BACKGROUND_COLOR)),
            ..container::Style::default()
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if app.port_list_dialog.is_visible {
        container(
            column![
                main_content,
                container(app.port_list_dialog.view())
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .padding(Padding::new(50.0)),
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .style(|_theme| container::Style {
            background: Some(Background::Color(BACKGROUND_COLOR)),
            ..container::Style::default()
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        main_content.into()
    };

    content
}

pub fn subscription(_app: &ScopeApp) -> Subscription<Message> {
    Subscription::batch(vec![
        iced::time::every(Duration::from_millis(100)).map(|_| Message::Tick),
    ])
}

pub fn run_gui(setup: SerialSetup, capacity: usize, tag_file: PathBuf, latency: u64) -> Result {
    iced::application("Scope Monitor", update, view)
        .subscription(subscription)
        .theme(|_| Theme::Dark)
        .window_size(iced::Size::new(1200.0, 800.0))
        .run_with(move || {
            let app = ScopeApp::new().with_settings(setup, capacity, tag_file, latency);
            (app, Task::none())
        })
}
