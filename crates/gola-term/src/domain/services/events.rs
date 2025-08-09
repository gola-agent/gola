use anyhow::Result;
use crossterm::event::Event as CrosstermEvent;
use crossterm::event::EventStream;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::time;
use tui_textarea::Input;
use tui_textarea::Key;

use crate::domain::models::Event;

pub struct EventsService {
    crossterm_events: EventStream,
    events: mpsc::UnboundedReceiver<Event>,
}

impl EventsService {
    pub fn new(events: mpsc::UnboundedReceiver<Event>) -> EventsService {
        return EventsService {
            crossterm_events: EventStream::new(),
            events,
        };
    }

    fn handle_crossterm(&self, event: CrosstermEvent) -> Option<Event> {
        match event {
            CrosstermEvent::Paste(text) => {
                return Some(Event::KeyboardPaste(text));
            }
            CrosstermEvent::Mouse(mouseevent) => match mouseevent.kind {
                crossterm::event::MouseEventKind::ScrollUp => {
                    return Some(Event::UIScrollUp);
                }
                crossterm::event::MouseEventKind::ScrollDown => {
                    return Some(Event::UIScrollDown);
                }
                _ => {
                    return None;
                }
            },
            CrosstermEvent::Key(keyevent) => {
                let key = match keyevent.code {
                    crossterm::event::KeyCode::Char(c) => Key::Char(c),
                    crossterm::event::KeyCode::Enter => Key::Enter,
                    crossterm::event::KeyCode::Left => Key::Left,
                    crossterm::event::KeyCode::Right => Key::Right,
                    crossterm::event::KeyCode::Up => Key::Up,
                    crossterm::event::KeyCode::Down => Key::Down,
                    crossterm::event::KeyCode::Home => Key::Home,
                    crossterm::event::KeyCode::End => Key::End,
                    crossterm::event::KeyCode::PageUp => Key::PageUp,
                    crossterm::event::KeyCode::PageDown => Key::PageDown,
                    crossterm::event::KeyCode::Tab => Key::Tab,
                    crossterm::event::KeyCode::Delete => Key::Delete,
                    crossterm::event::KeyCode::F(n) => Key::F(n),
                    crossterm::event::KeyCode::Backspace => Key::Backspace,
                    crossterm::event::KeyCode::Esc => Key::Esc,
                    _ => return None,
                };

                let input = Input {
                    key,
                    ctrl: keyevent
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL),
                    alt: keyevent
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::ALT),
                    shift: keyevent
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::SHIFT),
                };
                match input {
                    Input { key: Key::Down, .. } => {
                        return Some(Event::UIScrollDown);
                    }
                    Input { key: Key::Up, .. } => {
                        return Some(Event::UIScrollUp);
                    }
                    Input {
                        key: Key::MouseScrollDown,
                        ..
                    } => {
                        return Some(Event::UIScrollDown);
                    }
                    Input {
                        key: Key::MouseScrollUp,
                        ..
                    } => {
                        return Some(Event::UIScrollUp);
                    }
                    Input {
                        key: Key::PageDown, ..
                    } => {
                        return Some(Event::UIScrollPageDown);
                    }
                    Input {
                        key: Key::PageUp, ..
                    } => {
                        return Some(Event::UIScrollPageUp);
                    }
                    Input {
                        key: Key::Char('d'),
                        ctrl: true,
                        ..
                    } => {
                        return Some(Event::UIScrollPageDown);
                    }
                    Input {
                        key: Key::Char('u'),
                        ctrl: true,
                        ..
                    } => {
                        return Some(Event::UIScrollPageUp);
                    }
                    Input {
                        key: Key::Char('c'),
                        ctrl: true,
                        ..
                    } => {
                        return Some(Event::KeyboardCTRLC);
                    }
                    Input {
                        key: Key::Char('o'),
                        ctrl: true,
                        ..
                    } => {
                        return Some(Event::KeyboardCTRLO);
                    }
                    Input {
                        key: Key::Char('r'),
                        ctrl: true,
                        ..
                    } => {
                        return Some(Event::KeyboardCTRLR);
                    }
                    Input {
                        key: Key::Enter, ..
                    } => {
                        return Some(Event::KeyboardEnter);
                    }
                    input => {
                        return Some(Event::KeyboardCharInput(input));
                    }
                }
            }
            _ => return None,
        }
    }

    pub async fn next(&mut self) -> Result<Event> {
        loop {
            let evt = tokio::select! {
                event = self.events.recv() => event,
                event = self.crossterm_events.next() => match event {
                    Some(Ok(input)) => self.handle_crossterm(input),
                    Some(Err(_)) => None,
                    None => None
                },
                _ = time::sleep(time::Duration::from_millis(500)) => Some(Event::UITick)
            };

            if let Some(event) = evt {
                return Ok(event);
            }
        }
    }
}
