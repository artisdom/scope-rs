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
                    // Ctrl+End is a common "jump to bottom" gesture.
                    if matches!(key, Key::Named(keyboard::key::Named::End)) && modifiers.control() {
                        Some(Shortcut::JumpToEnd)
                    } else {
                        None
                    }
                }
                _ => None,
            })
        }))
    }
}

pub fn subscription() -> Subscription<Shortcut> {
    Subscription::from_recipe(ShortcutRecipe)
}
