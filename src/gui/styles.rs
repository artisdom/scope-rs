use iced::{
    Background, Border, Color, Shadow, Theme,
    widget::{
        button, container, pick_list, scrollable, text_input,
    },
};

pub const BACKGROUND_COLOR: Color = Color::from_rgb(0.1, 0.1, 0.12);
pub const SURFACE_COLOR: Color = Color::from_rgb(0.15, 0.15, 0.18);
pub const ACCENT_COLOR: Color = Color::from_rgb(0.2, 0.6, 0.9);
pub const SUCCESS_COLOR: Color = Color::from_rgb(0.2, 0.8, 0.4);
pub const ERROR_COLOR: Color = Color::from_rgb(0.9, 0.3, 0.3);
#[allow(dead_code)]
pub const WARNING_COLOR: Color = Color::from_rgb(0.9, 0.7, 0.2);
pub const TEXT_COLOR: Color = Color::from_rgb(0.9, 0.9, 0.9);
pub const TEXT_SECONDARY_COLOR: Color = Color::from_rgb(0.6, 0.6, 0.6);

pub fn button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: Some(Background::Color(SURFACE_COLOR)),
        text_color: TEXT_COLOR,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        shadow: Shadow::default(),
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(ACCENT_COLOR)),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.15, 0.5, 0.8))),
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: TEXT_SECONDARY_COLOR,
            ..base
        },
    }
}

pub fn primary_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: Some(Background::Color(ACCENT_COLOR)),
        text_color: TEXT_COLOR,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        shadow: Shadow::default(),
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.25, 0.65, 0.95))),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.15, 0.5, 0.8))),
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: TEXT_SECONDARY_COLOR,
            background: Some(Background::Color(SURFACE_COLOR)),
            ..base
        },
    }
}

pub fn success_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: Some(Background::Color(SUCCESS_COLOR)),
        text_color: TEXT_COLOR,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        shadow: Shadow::default(),
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.25, 0.85, 0.45))),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.15, 0.7, 0.35))),
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: TEXT_SECONDARY_COLOR,
            background: Some(Background::Color(SURFACE_COLOR)),
            ..base
        },
    }
}

pub fn danger_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: Some(Background::Color(ERROR_COLOR)),
        text_color: TEXT_COLOR,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        shadow: Shadow::default(),
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.95, 0.35, 0.35))),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.85, 0.25, 0.25))),
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: TEXT_SECONDARY_COLOR,
            background: Some(Background::Color(SURFACE_COLOR)),
            ..base
        },
    }
}

pub fn container_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE_COLOR)),
        text_color: Some(TEXT_COLOR),
        border: Border {
            color: Color::from_rgb(0.3, 0.3, 0.35),
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: Shadow::default(),
    }
}

pub fn terminal_container_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.05, 0.05, 0.07))),
        text_color: Some(Color::from_rgb(0.8, 0.9, 0.8)),
        border: Border {
            color: Color::from_rgb(0.2, 0.2, 0.25),
            width: 1.0,
            radius: 4.0.into(),
        },
        shadow: Shadow::default(),
    }
}

pub fn text_input_style(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let base = text_input::Style {
        background: Background::Color(SURFACE_COLOR),
        border: Border {
            color: Color::from_rgb(0.3, 0.3, 0.35),
            width: 1.0,
            radius: 4.0.into(),
        },
        icon: Color::from_rgb(0.6, 0.6, 0.6),
        placeholder: Color::from_rgb(0.5, 0.5, 0.5),
        value: TEXT_COLOR,
        selection: ACCENT_COLOR,
    };

    match status {
        text_input::Status::Active => text_input::Style {
            border: Border {
                color: ACCENT_COLOR,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..base
        },
        text_input::Status::Focused => text_input::Style {
            border: Border {
                color: ACCENT_COLOR,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..base
        },
        text_input::Status::Hovered => base,
        text_input::Status::Disabled => text_input::Style {
            value: TEXT_SECONDARY_COLOR,
            ..base
        },
    }
}

pub fn scrollable_style(_theme: &Theme, _status: scrollable::Status) -> scrollable::Style {
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: scrollable::Rail {
            background: Some(Background::Color(Color::from_rgb(0.1, 0.1, 0.12))),
            border: Border::default(),
            scroller: scrollable::Scroller {
                color: Color::from_rgb(0.3, 0.3, 0.35),
                border: Border::default(),
            },
        },
        horizontal_rail: scrollable::Rail {
            background: Some(Background::Color(Color::from_rgb(0.1, 0.1, 0.12))),
            border: Border::default(),
            scroller: scrollable::Scroller {
                color: Color::from_rgb(0.3, 0.3, 0.35),
                border: Border::default(),
            },
        },
        gap: None,
    }
}

pub fn pick_list_style(_theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let base = pick_list::Style {
        background: Background::Color(SURFACE_COLOR),
        border: Border {
            color: Color::from_rgb(0.3, 0.3, 0.35),
            width: 1.0,
            radius: 4.0.into(),
        },
        text_color: TEXT_COLOR,
        placeholder_color: TEXT_SECONDARY_COLOR,
        handle_color: TEXT_COLOR,
    };

    match status {
        pick_list::Status::Active => base,
        pick_list::Status::Hovered => pick_list::Style {
            border: Border {
                color: ACCENT_COLOR,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..base
        },
        pick_list::Status::Opened => pick_list::Style {
            border: Border {
                color: ACCENT_COLOR,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..base
        },
    }
}

pub fn menu_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: TEXT_COLOR,
        border: Border::default(),
        shadow: Shadow::default(),
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(SURFACE_COLOR)),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(ACCENT_COLOR)),
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: TEXT_SECONDARY_COLOR,
            ..base
        },
    }
}
