use crate::gui::styles::{terminal_container_style, text_input_style};
use iced::{
    Element, Length, Padding,
    widget::{button, column, container, row, scrollable, text, text_input, Column, toggler},
};
use std::collections::VecDeque;

use super::message::Message;
use super::styles::{button_style, primary_button_style, ACCENT_COLOR, ERROR_COLOR, SUCCESS_COLOR};

const MAX_LINES: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum InputMode {
    #[default]
    Ascii,
    Hex,
}

#[derive(Debug, Clone)]
pub struct TerminalLine {
    pub content: String,
    pub timestamp: Option<String>,
    pub is_tx: bool, // true if sent, false if received
}

#[derive(Debug, Clone)]
pub struct HexByte {
    pub high: Option<char>,
    pub low: Option<char>,
}

#[allow(dead_code)]
impl HexByte {
    pub fn new() -> Self {
        Self { high: None, low: None }
    }
    
    pub fn is_complete(&self) -> bool {
        self.high.is_some() && self.low.is_some()
    }
    
    pub fn to_byte(&self) -> Option<u8> {
        match (self.high, self.low) {
            (Some(h), Some(l)) => {
                let high_val = h.to_digit(16)? as u8;
                let low_val = l.to_digit(16)? as u8;
                Some((high_val << 4) | low_val)
            }
            _ => None,
        }
    }
    
    pub fn display(&self) -> String {
        match (self.high, self.low) {
            (Some(h), Some(l)) => format!("{}{}", h, l),
            (Some(h), None) => format!("{}_", h),
            (None, Some(l)) => format!("_{}", l),
            (None, None) => "__".to_string(),
        }
    }
    
    pub fn clear(&mut self) {
        self.high = None;
        self.low = None;
    }
}

#[derive(Debug, Clone, Default)]
pub struct TerminalView {
    pub lines: VecDeque<TerminalLine>,
    pub input_buffer: String,
    pub search_buffer: String,
    pub is_search_mode: bool,
    pub is_case_sensitive: bool,
    pub search_index: usize,
    pub search_results: Vec<usize>,
    #[allow(dead_code)]
    pub scroll_offset: f32,
    
    // Hex input mode
    pub input_mode: InputMode,
    pub hex_bytes: Vec<HexByte>,
    pub hex_input_buffer: String,
    pub hex_error: Option<String>,
}

impl TerminalView {
    pub fn new() -> Self {
        Self {
            lines: VecDeque::with_capacity(MAX_LINES),
            input_buffer: String::new(),
            search_buffer: String::new(),
            is_search_mode: false,
            is_case_sensitive: false,
            search_index: 0,
            search_results: Vec::new(),
            scroll_offset: 0.0,
            input_mode: InputMode::Ascii,
            hex_bytes: Vec::new(),
            hex_input_buffer: String::new(),
            hex_error: None,
        }
    }

    pub fn add_line(&mut self, line: TerminalLine) {
        if self.lines.len() >= MAX_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    pub fn add_received_data(&mut self, data: &[u8], timestamp: Option<String>) {
        let content = String::from_utf8_lossy(data).to_string();
        for line in content.lines() {
            self.add_line(TerminalLine {
                content: line.to_string(),
                timestamp: timestamp.clone(),
                is_tx: false,
            });
        }
    }

    pub fn add_sent_data(&mut self, data: &str, timestamp: Option<String>) {
        for line in data.lines() {
            self.add_line(TerminalLine {
                content: line.to_string(),
                timestamp: timestamp.clone(),
                is_tx: true,
            });
        }
    }
    
    pub fn add_sent_bytes(&mut self, bytes: &[u8], timestamp: Option<String>) {
        let hex_display: String = bytes.iter()
            .map(|b| format!("{:02X} ", b))
            .collect();
        self.add_line(TerminalLine {
            content: format!("[HEX] {}", hex_display.trim()),
            timestamp,
            is_tx: true,
        });
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.search_results.clear();
        self.search_index = 0;
    }

    pub fn update_search(&mut self) {
        self.search_results.clear();
        self.search_index = 0;

        if self.search_buffer.is_empty() {
            return;
        }

        let search_term = if self.is_case_sensitive {
            self.search_buffer.clone()
        } else {
            self.search_buffer.to_lowercase()
        };

        for (i, line) in self.lines.iter().enumerate() {
            let content = if self.is_case_sensitive {
                line.content.clone()
            } else {
                line.content.to_lowercase()
            };

            if content.contains(&search_term) {
                self.search_results.push(i);
            }
        }
    }

    pub fn next_search_result(&mut self) {
        if self.search_results.is_empty() {
            return;
        }
        self.search_index = (self.search_index + 1) % self.search_results.len();
    }

    pub fn prev_search_result(&mut self) {
        if self.search_results.is_empty() {
            return;
        }
        self.search_index = if self.search_index == 0 {
            self.search_results.len() - 1
        } else {
            self.search_index - 1
        };
    }

    pub fn current_search_position(&self) -> Option<usize> {
        self.search_results.get(self.search_index).copied()
    }
    
    #[allow(dead_code)]
    pub fn toggle_input_mode(&mut self) {
        self.input_mode = match self.input_mode {
            InputMode::Ascii => InputMode::Hex,
            InputMode::Hex => InputMode::Ascii,
        };
        self.hex_bytes.clear();
        self.hex_input_buffer.clear();
        self.hex_error = None;
    }
    
    #[allow(dead_code)]
    pub fn add_hex_char(&mut self, c: char) {
        // Validate hex character
        if !c.is_ascii_hexdigit() {
            return;
        }
        
        // Add to last byte or create new one
        if let Some(last_byte) = self.hex_bytes.last_mut() {
            if last_byte.high.is_none() {
                last_byte.high = Some(c.to_ascii_uppercase());
            } else if last_byte.low.is_none() {
                last_byte.low = Some(c.to_ascii_uppercase());
            } else {
                // Last byte is complete, create new one
                self.hex_bytes.push(HexByte { high: Some(c.to_ascii_uppercase()), low: None });
            }
        } else {
            self.hex_bytes.push(HexByte { high: Some(c.to_ascii_uppercase()), low: None });
        }
        
        self.hex_error = None;
    }
    
    #[allow(dead_code)]
    pub fn backspace_hex(&mut self) {
        if let Some(last_byte) = self.hex_bytes.last_mut() {
            if last_byte.low.is_some() {
                last_byte.low = None;
            } else if last_byte.high.is_some() {
                last_byte.high = None;
            } else {
                self.hex_bytes.pop();
            }
        }
    }
    
    pub fn clear_hex(&mut self) {
        self.hex_bytes.clear();
        self.hex_input_buffer.clear();
        self.hex_error = None;
    }
    
    pub fn get_hex_bytes(&self) -> Vec<u8> {
        self.hex_bytes.iter()
            .filter_map(|hb| hb.to_byte())
            .collect()
    }
    
    pub fn parse_hex_string(&mut self, s: &str) -> Option<Vec<u8>> {
        let clean: String = s.chars()
            .filter(|c| c.is_ascii_hexdigit())
            .collect();
        
        if clean.is_empty() {
            return Some(vec![]);
        }
        
        if clean.len() % 2 != 0 {
            self.hex_error = Some("Odd number of hex digits".to_string());
            return None;
        }
        
        let mut bytes = Vec::new();
        for chunk in clean.as_bytes().chunks(2) {
            let high = chunk[0].to_ascii_uppercase() as char;
            let low = chunk[1].to_ascii_uppercase() as char;
            let byte = (high.to_digit(16)? as u8) << 4 | (low.to_digit(16)? as u8);
            bytes.push(byte);
        }
        
        self.hex_error = None;
        Some(bytes)
    }

    pub fn view(&self) -> Element<'_, Message> {
        let terminal_content: Element<Message> = if self.lines.is_empty() {
            text("No data received yet...")
                .style(|_theme| text::Style {
                    color: Some(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                })
                .into()
        } else {
            let mut col = Column::new();
            
            for (idx, line) in self.lines.iter().enumerate() {
                let is_match = self.search_results.contains(&idx);
                let is_current = self.current_search_position() == Some(idx);

                let line_text = if let Some(ts) = &line.timestamp {
                    format!("[{}] {}", ts, line.content)
                } else {
                    line.content.clone()
                };

                let line_widget = text(line_text).style(move |_theme| {
                    if is_current {
                        text::Style {
                            color: Some(iced::Color::from_rgb(1.0, 1.0, 0.0)),
                        }
                    } else if is_match {
                        text::Style {
                            color: Some(iced::Color::from_rgb(1.0, 0.8, 0.0)),
                        }
                    } else if line.is_tx {
                        text::Style {
                            color: Some(iced::Color::from_rgb(0.4, 0.8, 0.4)),
                        }
                    } else {
                        text::Style {
                            color: Some(iced::Color::from_rgb(0.8, 0.9, 0.8)),
                        }
                    }
                });

                col = col.push(line_widget);
            }

            scrollable(col)
                .height(Length::Fill)
                .width(Length::Fill)
                .into()
        };

        let terminal_container = container(terminal_content)
            .style(terminal_container_style)
            .padding(Padding::new(10.0))
            .height(Length::Fill)
            .width(Length::Fill);

        // Input bar
        let input_bar: Element<Message> = if self.is_search_mode {
            let search_info = if self.search_results.is_empty() {
                "0/0".to_string()
            } else {
                format!("{}/{}", self.search_index + 1, self.search_results.len())
            };

            let case_indicator = if self.is_case_sensitive { "Aa" } else { "--" };

            row![
                text(format!("[{}][{}] Search:", case_indicator, search_info))
                    .style(|_theme| text::Style {
                        color: Some(iced::Color::from_rgb(0.9, 0.7, 0.2)),
                    }),
                text_input("Search...", &self.search_buffer)
                    .on_input(Message::SearchInput)
                    .on_submit(Message::SearchNext)
                    .style(text_input_style)
                    .width(Length::Fill),
                button("Prev")
                    .on_press(Message::SearchPrev)
                    .style(button_style),
                button("Next")
                    .on_press(Message::SearchNext)
                    .style(button_style),
                button("Esc")
                    .on_press(Message::ToggleSearchMode)
                    .style(button_style),
            ]
            .spacing(10)
            .into()
        } else {
            // Mode toggle
            let mode_label = match self.input_mode {
                InputMode::Ascii => "ASCII",
                InputMode::Hex => "HEX",
            };
            
            let mode_color = match self.input_mode {
                InputMode::Ascii => SUCCESS_COLOR,
                InputMode::Hex => ACCENT_COLOR,
            };
            
            let mode_toggle = row![
                text(mode_label)
                    .style(move |_theme| text::Style { color: Some(mode_color) })
                    .size(12),
                toggler(self.input_mode == InputMode::Hex)
                    .on_toggle(|enabled| {
                        if enabled {
                            Message::SwitchToHexMode
                        } else {
                            Message::SwitchToAsciiMode
                        }
                    }),
            ]
            .spacing(5);
            
            match self.input_mode {
                InputMode::Ascii => {
                    column![
                        row![
                            mode_toggle,
                            text_input("Enter command...", &self.input_buffer)
                                .on_input(Message::TerminalInput)
                                .on_submit(Message::SendCommand)
                                .style(text_input_style)
                                .width(Length::Fill),
                            button("Send")
                                .on_press(Message::SendCommand)
                                .style(primary_button_style),
                        ]
                        .spacing(10)
                    ]
                    .into()
                }
                InputMode::Hex => {
                    // Hex input display
                    let hex_display: String = self.hex_bytes.iter()
                        .map(|hb| hb.display())
                        .collect::<Vec<_>>()
                        .join(" ");
                    
                    // Preview of bytes to send
                    let preview = if self.hex_bytes.is_empty() {
                        "No bytes".to_string()
                    } else {
                        let bytes: Vec<u8> = self.get_hex_bytes();
                        format!("{} byte(s): {:?}", bytes.len(), bytes)
                    };
                    
                    // Error display
                    let error_display = if let Some(ref err) = self.hex_error {
                        text(err).style(|_theme| text::Style { color: Some(ERROR_COLOR) })
                    } else {
                        text("")
                    };
                    
                    // Quick hex input buttons
                    let quick_buttons = row![
                        button("00").on_press(Message::QuickHex("00".to_string())).style(button_style),
                        button("0D").on_press(Message::QuickHex("0D".to_string())).style(button_style),
                        button("0A").on_press(Message::QuickHex("0A".to_string())).style(button_style),
                        button("Clear").on_press(Message::ClearHexInput).style(button_style),
                    ]
                    .spacing(5);
                    
                    column![
                        row![
                            mode_toggle,
                            text_input("Type hex (e.g., 48656C6C6F)...", &self.hex_input_buffer)
                                .on_input(Message::HexInput)
                                .on_submit(Message::SendCommand)
                                .style(text_input_style)
                                .width(Length::Fill),
                            button("Send")
                                .on_press(Message::SendCommand)
                                .style(primary_button_style),
                        ]
                        .spacing(10),
                        row![
                            text("Bytes: ").size(12),
                            text(hex_display).size(12).style(|_theme| text::Style { 
                                color: Some(ACCENT_COLOR) 
                            }),
                            text("  |  ").size(12),
                            text(preview).size(12),
                        ]
                        .spacing(5),
                        row![
                            quick_buttons,
                            error_display,
                        ]
                        .spacing(10),
                    ]
                    .spacing(5)
                    .into()
                }
            }
        };

        column![terminal_container, input_bar]
            .spacing(10)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }
}
