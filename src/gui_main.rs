#![deny(warnings)]

use iced::widget::{column, container, text};
use iced::{Application, Element, Length, Settings, Theme};

fn main() -> iced::Result {
    ScopeGui::run(Settings {
        window: iced::window::Settings {
            size: iced::Size::new(980.0, 640.0),
            ..Default::default()
        },
        ..Default::default()
    })
}

#[derive(Debug, Default)]
struct ScopeGui {
    // Placeholder state; we will wire serial engine next.
}

#[derive(Debug, Clone)]
enum Message {}

impl Application for ScopeGui {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        (Self::default(), iced::Command::none())
    }

    fn title(&self) -> String {
        "Scope (GUI)".to_string()
    }

    fn update(&mut self, _message: Self::Message) -> iced::Command<Self::Message> {
        iced::Command::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let content = column![
            text("Scope (GUI)").size(28),
            text("Status: not yet wired to serial engine"),
            text("Next: connect/disconnect + live log view + send bar").size(16),
        ]
        .spacing(12)
        .padding(16);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }
}
