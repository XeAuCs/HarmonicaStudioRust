use super::*;

pub(super) fn global_key(info: KeyEventInfo) -> RoutedMessage<Message> {
    if info.modifiers.contains(InputModifiers::CONTROL) {
        match info.key {
            VirtualKey::S => return RoutedMessage::handled(Message::Save),
            VirtualKey::O => return RoutedMessage::handled(Message::Open),
            _ => {}
        }
    }
    RoutedMessage::bubble_without_message()
}
pub(super) fn editor_key(info: KeyEventInfo) -> RoutedMessage<Message> {
    let ctrl = info.modifiers.contains(InputModifiers::CONTROL);
    let shift = info.modifiers.contains(InputModifiers::SHIFT);
    let message = match info.key {
        VirtualKey::Z if ctrl => {
            if shift {
                Message::Redo
            } else {
                Message::Undo
            }
        }
        VirtualKey::Y if ctrl => Message::Redo,
        VirtualKey::S if ctrl => Message::Save,
        VirtualKey::O if ctrl => Message::Open,
        VirtualKey::DELETE => Message::Delete,
        VirtualKey::SPACE => Message::Listen,
        VirtualKey::ESCAPE => Message::PointerCancel,
        VirtualKey::H => Message::Mark,
        VirtualKey::LEFT => {
            if ctrl {
                Message::Pan(-2.0)
            } else if shift {
                Message::Nudge(0.0, 0, -0.05)
            } else {
                Message::Nudge(-0.05, 0, 0.0)
            }
        }
        VirtualKey::RIGHT => {
            if ctrl {
                Message::Pan(2.0)
            } else if shift {
                Message::Nudge(0.0, 0, 0.05)
            } else {
                Message::Nudge(0.05, 0, 0.0)
            }
        }
        VirtualKey::UP => Message::Nudge(0.0, if shift { 12 } else { 1 }, 0.0),
        VirtualKey::DOWN => Message::Nudge(0.0, if shift { -12 } else { -1 }, 0.0),
        _ => return RoutedMessage::bubble_without_message(),
    };
    RoutedMessage::handled(message)
}
