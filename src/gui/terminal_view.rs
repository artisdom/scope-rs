use crate::gui::styles::{terminal_container_style, text_input_style};
use iced::{
    Element, Length, Padding,
    widget::{button, column, container, row, scrollable, text, text_input, Column},
};
use std::collections::VecDeque;

use super::message::Message;
use super::styles::{button_style, primary_button_style};

const MAX_LINES: usize = 1000;

#[derive(Debug, Clone)]
pub struct TerminalLine {
    pub content: String,
    pub timestamp: Option<String>,
    pub is_tx: bool, // true if sent, false if received
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
        let input_bar = if self.is_search_mode {
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
        } else {
            row![
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
        };

        column![terminal_container, input_bar]
            .spacing(10)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }
}
