use crate::gui::styles::{
    button_style, container_style, pick_list_style, primary_button_style, text_input_style,
    TEXT_COLOR, TEXT_SECONDARY_COLOR,
};
use crate::serial::serial_if::SerialSetup;
use iced::{
    Element, Length, Padding,
    widget::{button, column, container, pick_list, row, text, text_input},
};
use serialport::{DataBits, FlowControl, Parity, StopBits};

use super::message::Message;

const BAUDRATES: [u32; 14] = [
    300, 1200, 2400, 4800, 9600, 14400, 19200, 38400, 57600, 115200, 230400, 460800, 921600,
    1000000,
];

const DATA_BITS: [DataBits; 4] = [
    DataBits::Five,
    DataBits::Six,
    DataBits::Seven,
    DataBits::Eight,
];

const PARITIES: [Parity; 3] = [Parity::None, Parity::Odd, Parity::Even];

const STOP_BITS: [StopBits; 2] = [StopBits::One, StopBits::Two];

const FLOW_CONTROLS: [FlowControl; 3] = [FlowControl::None, FlowControl::Software, FlowControl::Hardware];

#[derive(Debug, Clone)]
pub struct ConfigPanel {
    pub port: String,
    pub baudrate: u32,
    pub data_bits: DataBits,
    pub parity: Parity,
    pub stop_bits: StopBits,
    pub flow_control: FlowControl,
    pub capacity: usize,
    pub tag_file: String,
    pub latency: u64,
    pub is_visible: bool,
    // Temporary edit values
    pub baudrate_input: String,
    pub capacity_input: String,
    pub latency_input: String,
}

impl Default for ConfigPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigPanel {
    pub fn new() -> Self {
        Self {
            port: String::new(),
            baudrate: 115200,
            data_bits: DataBits::Eight,
            parity: Parity::None,
            stop_bits: StopBits::One,
            flow_control: FlowControl::None,
            capacity: 2000,
            tag_file: "tags.yml".to_string(),
            latency: 500,
            is_visible: false,
            baudrate_input: "115200".to_string(),
            capacity_input: "2000".to_string(),
            latency_input: "500".to_string(),
        }
    }

    pub fn from_setup(setup: SerialSetup) -> Self {
        Self {
            port: setup.port.unwrap_or_default(),
            baudrate: setup.baudrate.unwrap_or(115200),
            data_bits: setup.data_bits.unwrap_or(DataBits::Eight),
            parity: setup.parity.unwrap_or(Parity::None),
            stop_bits: setup.stop_bits.unwrap_or(StopBits::One),
            flow_control: setup.flow_control.unwrap_or(FlowControl::None),
            capacity: 2000,
            tag_file: "tags.yml".to_string(),
            latency: 500,
            is_visible: false,
            baudrate_input: setup.baudrate.unwrap_or(115200).to_string(),
            capacity_input: "2000".to_string(),
            latency_input: "500".to_string(),
        }
    }

    pub fn to_setup(&self) -> SerialSetup {
        SerialSetup {
            port: if self.port.is_empty() { None } else { Some(self.port.clone()) },
            baudrate: Some(self.baudrate),
            data_bits: Some(self.data_bits),
            parity: Some(self.parity),
            stop_bits: Some(self.stop_bits),
            flow_control: Some(self.flow_control),
        }
    }

    pub fn show(&mut self) {
        self.is_visible = true;
    }

    pub fn hide(&mut self) {
        self.is_visible = false;
    }

    #[allow(dead_code)]
    pub fn toggle(&mut self) {
        self.is_visible = !self.is_visible;
    }

    pub fn view(&self, is_connected: bool) -> Element<'_, Message> {
        let title = text("Serial Configuration")
            .size(18)
            .style(|_theme| text::Style {
                color: Some(TEXT_COLOR),
            });

        // Port selection
        let port_row = row![
            text("Port:").width(Length::Fixed(100.0)),
            text_input("/dev/ttyUSB0 or COM1", &self.port)
                .on_input(Message::PortChanged)
                .style(text_input_style)
                .width(Length::Fill),
            button("List Ports")
                .on_press(Message::ShowPortListDialog)
                .style(button_style),
        ]
        .spacing(10);

        // Baudrate selection
        let baudrate_row = row![
            text("Baudrate:").width(Length::Fixed(100.0)),
            pick_list(&BAUDRATES[..], Some(self.baudrate), |b| {
                Message::BaudrateChanged(b.to_string())
            })
            .style(pick_list_style)
            .width(Length::Fill),
            text_input("Custom", &self.baudrate_input)
                .on_input(Message::BaudrateChanged)
                .style(text_input_style)
                .width(Length::Fixed(100.0)),
        ]
        .spacing(10);

        // Data bits selection
        let data_bits_row = row![
            text("Data Bits:").width(Length::Fixed(100.0)),
            pick_list(&DATA_BITS[..], Some(self.data_bits), Message::DataBitsChanged)
                .style(pick_list_style)
                .width(Length::Fill),
        ]
        .spacing(10);

        // Parity selection
        let parity_row = row![
            text("Parity:").width(Length::Fixed(100.0)),
            pick_list(&PARITIES[..], Some(self.parity), Message::ParityChanged)
                .style(pick_list_style)
                .width(Length::Fill),
        ]
        .spacing(10);

        // Stop bits selection
        let stop_bits_row = row![
            text("Stop Bits:").width(Length::Fixed(100.0)),
            pick_list(&STOP_BITS[..], Some(self.stop_bits), Message::StopBitsChanged)
                .style(pick_list_style)
                .width(Length::Fill),
        ]
        .spacing(10);

        // Flow control selection
        let flow_control_row = row![
            text("Flow Control:").width(Length::Fixed(100.0)),
            pick_list(&FLOW_CONTROLS[..], Some(self.flow_control), Message::FlowControlChanged)
                .style(pick_list_style)
                .width(Length::Fill),
        ]
        .spacing(10);

        // Separator
        let separator = text("─".repeat(50))
            .style(|_theme| text::Style {
                color: Some(TEXT_SECONDARY_COLOR),
            });

        // Application settings
        let settings_title = text("Application Settings")
            .size(16)
            .style(|_theme| text::Style {
                color: Some(TEXT_COLOR),
            });

        // Capacity
        let capacity_row = row![
            text("Buffer Capacity:").width(Length::Fixed(120.0)),
            text_input("2000", &self.capacity_input)
                .on_input(Message::CapacityChanged)
                .style(text_input_style)
                .width(Length::Fill),
        ]
        .spacing(10);

        // Tag file
        let tag_file_row = row![
            text("Tag File:").width(Length::Fixed(120.0)),
            text_input("tags.yml", &self.tag_file)
                .on_input(Message::TagFileChanged)
                .style(text_input_style)
                .width(Length::Fill),
        ]
        .spacing(10);

        // Latency
        let latency_row = row![
            text("Latency (μs):").width(Length::Fixed(120.0)),
            text_input("500", &self.latency_input)
                .on_input(Message::LatencyChanged)
                .style(text_input_style)
                .width(Length::Fill),
        ]
        .spacing(10);

        // Action buttons
        let action_buttons = row![
            if is_connected {
                button("Disconnect")
                    .on_press(Message::DisconnectSerial)
                    .style(crate::gui::styles::danger_button_style)
            } else {
                button("Connect")
                    .on_press(Message::ConnectSerial)
                    .style(crate::gui::styles::success_button_style)
            },
            button("Apply Settings")
                .on_press(Message::ApplyConfig)
                .style(primary_button_style),
            button("Close")
                .on_press(Message::HideConfigPanel)
                .style(button_style),
        ]
        .spacing(10);

        let content = column![
            title,
            port_row,
            baudrate_row,
            data_bits_row,
            parity_row,
            stop_bits_row,
            flow_control_row,
            separator,
            settings_title,
            capacity_row,
            tag_file_row,
            latency_row,
            action_buttons,
        ]
        .spacing(15)
        .padding(Padding::new(20.0));

        container(content)
            .style(container_style)
            .width(Length::Fixed(500.0))
            .into()
    }
}
