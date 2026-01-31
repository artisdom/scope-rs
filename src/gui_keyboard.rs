use iced::Event;
use iced::Subscription;
use iced::keyboard;
use iced::keyboard::Key;
use iced_futures::subscription::{EventStream, Recipe};
use iced_futures::BoxStream;
use std::hash::Hash;
use iced_core::Hasher;

#[derive(Debug, Clone)]
pub enum Shortcut {
    JumpToEnd,
    JumpToStart,
    ScrollPageUp,
    ScrollPageDown,
    HistoryPrev,
    HistoryNext,
    SaveHistory,
    ToggleRecord,
    ClearLog,
}

struct ShortcutRecipe;

impl Recipe for ShortcutRecipe {
    type Output = Shortcut;

    fn hash(&self, state: &mut Hasher) {
        std::any::TypeId::of::<Self>().hash(state);
    }

    fn stream(self: Box<Self>, input: EventStream) -> BoxStream<Self::Output> {
        use iced::futures::StreamExt;

        Box::pin(input.filter_map(|(event, _status)| {
            iced::futures::future::ready(match event {
                Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    let key = key.as_ref();
                    if matches!(key, Key::Named(keyboard::key::Named::End)) && modifiers.control() {
                        return Some(Shortcut::JumpToEnd);
                    }
                    if matches!(key, Key::Named(keyboard::key::Named::Home)) && modifiers.control() {
                        return Some(Shortcut::JumpToStart);
                    }
                    if matches!(key, Key::Named(keyboard::key::Named::PageUp)) {
                        return Some(Shortcut::ScrollPageUp);
                    }
                    if matches!(key, Key::Named(keyboard::key::Named::PageDown)) {
                        return Some(Shortcut::ScrollPageDown);
                    }
                    if matches!(key, Key::Named(keyboard::key::Named::ArrowUp)) {
                        return Some(Shortcut::HistoryPrev);
                    }
                    if matches!(key, Key::Named(keyboard::key::Named::ArrowDown)) {
                        return Some(Shortcut::HistoryNext);
                    }
                    if matches!(key, Key::Character("s")) && modifiers.control() {
                        return Some(Shortcut::SaveHistory);
                    }
                    if matches!(key, Key::Character("r")) && modifiers.control() {
                        return Some(Shortcut::ToggleRecord);
                    }
                    if matches!(key, Key::Character("l")) && modifiers.control() {
                        return Some(Shortcut::ClearLog);
                    }
                    None
                }
                _ => None,
            })
        }))
    }
}

pub fn subscription() -> Subscription<Shortcut> {
    Subscription::from_recipe(ShortcutRecipe)
}
