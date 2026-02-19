use crate::gui::styles::{
    button_style, container_style, primary_button_style, scrollable_style,
    TEXT_COLOR, TEXT_SECONDARY_COLOR,
};
use crate::gui::message::{Message, PortInfo};
use iced::{
    Element, Length, Padding,
    widget::{button, column, container, row, scrollable, text},
};

#[derive(Debug, Clone, Default)]
pub struct PortListDialog {
    pub is_visible: bool,
    pub ports: Vec<PortInfo>,
    pub selected_port: Option<String>,
    pub is_loading: bool,
}

impl PortListDialog {
    pub fn new() -> Self {
        Self {
            is_visible: false,
            ports: Vec::new(),
            selected_port: None,
            is_loading: false,
        }
    }

    pub fn show(&mut self) {
        self.is_visible = true;
    }

    pub fn hide(&mut self) {
        self.is_visible = false;
    }

    pub fn refresh(&mut self) {
        self.is_loading = true;
        self.ports.clear();
    }

    pub fn set_ports(&mut self, ports: Vec<PortInfo>) {
        self.ports = ports;
        self.is_loading = false;
    }

    pub fn view(&self) -> Element<'_, Message> {
        let title = text("Available Serial Ports")
            .size(18)
            .style(|_theme| text::Style {
                color: Some(TEXT_COLOR),
            });

        let header = row![
            text("Port").width(Length::FillPortion(2)),
            text("Serial Number").width(Length::FillPortion(2)),
            text("PID").width(Length::FillPortion(1)),
            text("VID").width(Length::FillPortion(1)),
            text("Manufacturer").width(Length::FillPortion(2)),
        ]
        .spacing(10)
        .padding(Padding::new(5.0));

        let header_container = container(header)
            .style(|_theme| container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgb(0.2, 0.2, 0.25))),
                ..container::Style::default()
            })
            .width(Length::Fill);

        let ports_list: Element<Message> = if self.is_loading {
            text("Loading ports...")
                .style(|_theme| text::Style {
                    color: Some(TEXT_SECONDARY_COLOR),
                })
                .into()
        } else if self.ports.is_empty() {
            text("No serial ports found. Click 'Refresh' to scan again.")
                .style(|_theme| text::Style {
                    color: Some(TEXT_SECONDARY_COLOR),
                })
                .into()
        } else {
            let mut col = column![];
            
            for port in &self.ports {
                let is_selected = self.selected_port.as_deref() == Some(port.name.as_str());

                let port_row = row![
                    text(&port.name).width(Length::FillPortion(2)),
                    text(port.serial_number.as_deref().unwrap_or("???"))
                        .width(Length::FillPortion(2)),
                    text(format!("0x{:04X}", port.pid)).width(Length::FillPortion(1)),
                    text(format!("0x{:04X}", port.vid)).width(Length::FillPortion(1)),
                    text(port.manufacturer.as_deref().unwrap_or("???"))
                        .width(Length::FillPortion(2)),
                ]
                .spacing(10)
                .padding(Padding::new(5.0));

                let row_container = container(port_row)
                    .style(move |_theme| {
                        if is_selected {
                            container::Style {
                                background: Some(
                                    iced::Background::Color(iced::Color::from_rgb(0.2, 0.4, 0.6)),
                                ),
                                ..container::Style::default()
                            }
                        } else {
                            container::Style::default()
                        }
                    })
                    .width(Length::Fill);

                col = col.push(
                    button(row_container)
                        .on_press(Message::SelectPort(port.name.clone()))
                        .style(|_theme, _status| {
                            button::Style {
                                background: Some(iced::Background::Color(iced::Color::TRANSPARENT)),
                                ..button::Style::default()
                            }
                        })
                        .width(Length::Fill),
                );
            }

            scrollable(col)
                .style(scrollable_style)
                .height(Length::Fixed(200.0))
                .into()
        };

        let use_button = if let Some(ref port) = self.selected_port {
            button(text(format!("Use {}", port)))
                .on_press(Message::SelectPort(port.clone()))
                .style(primary_button_style)
        } else {
            button(text("Select a port")).style(button_style)
        };

        let buttons = row![
            button(text("Refresh"))
                .on_press(Message::RefreshPorts)
                .style(button_style),
            use_button,
            button(text("Cancel"))
                .on_press(Message::HidePortListDialog)
                .style(button_style),
        ]
        .spacing(10);

        let content = column![title, header_container, ports_list, buttons]
            .spacing(15)
            .padding(Padding::new(20.0));

        container(content)
            .style(container_style)
            .width(Length::Fixed(600.0))
            .into()
    }
}
