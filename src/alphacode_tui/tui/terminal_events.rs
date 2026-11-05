//! Forward terminal events immediately, without inferring paste from typing speed.
//!
//! Ctrl+V/Alt+V are handled by the shared asynchronous clipboard action. Native
//! bracketed paste arrives as one Event::Paste. Ordinary keys, including Enter,
//! must never wait for another character or a paste-settling timer.
use crossterm::event::{Event, EventStream};
use futures::Stream;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

pub(super) type TerminalEventStream = ImmediateEvents<EventStream>;

pub(super) struct ImmediateEvents<S> {
    source: S,
}

impl TerminalEventStream {
    pub(super) fn new() -> Self {
        Self::from_source(EventStream::new())
    }
}

impl Default for TerminalEventStream {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> ImmediateEvents<S> {
    pub(super) fn from_source(source: S) -> Self {
        Self { source }
    }
}

impl<S: Stream<Item = io::Result<Event>> + Unpin> Stream for ImmediateEvents<S> {
    type Item = io::Result<Event>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.get_mut().source).poll_next(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use futures::StreamExt;

    #[tokio::test]
    async fn first_key_never_waits_or_reads_ahead_for_paste_detection() {
        for code in [KeyCode::Char('x'), KeyCode::Enter] {
            let expected = Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
            let mut event = Some(expected.clone());
            let source = futures::stream::poll_fn(move |_| {
                Poll::Ready(Some(Ok(event
                    .take()
                    .expect("must not poll ahead for another key"))))
            });
            let mut stream = ImmediateEvents::from_source(source);
            assert_eq!(stream.next().await.unwrap().unwrap(), expected);
        }
    }

    #[tokio::test]
    async fn paste_shortcuts_are_forwarded_without_reading_clipboard_in_the_event_stream() {
        for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
            let expected = Event::Key(KeyEvent::new(KeyCode::Char('v'), modifiers));
            let mut stream =
                ImmediateEvents::from_source(futures::stream::iter(vec![Ok(expected.clone())]));
            assert_eq!(stream.next().await.unwrap().unwrap(), expected);
            assert!(stream.next().await.is_none());
        }
    }

    #[tokio::test]
    async fn large_bracketed_paste_remains_one_atomic_payload() {
        let text = "a\n世界 👩‍💻\n".repeat(12_000);
        let mut stream = ImmediateEvents::from_source(futures::stream::iter(vec![Ok(
            Event::Paste(text.clone()),
        )]));
        assert_eq!(stream.next().await.unwrap().unwrap(), Event::Paste(text));
        assert!(stream.next().await.is_none());
    }
}
