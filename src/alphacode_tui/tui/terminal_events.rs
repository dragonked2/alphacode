//! Forward terminal events immediately, without inferring paste from typing speed.
//!
//! Ctrl+V/Alt+V are handled by the shared asynchronous clipboard action. Native
//! bracketed paste arrives as one Event::Paste. Ordinary keys, including Enter,
//! must never wait for another character or a paste-settling timer.
#[cfg(not(windows))]
use crossterm::event::EventStream;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::{FutureExt, Stream, StreamExt};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

#[cfg(not(windows))]
pub(super) type TerminalEventStream = ImmediateEvents<EventStream>;
#[cfg(windows)]
pub(super) type TerminalEventStream = ImmediateEvents<WindowsInput>;

pub(super) struct ImmediateEvents<S> {
    source: S,
    buffered: Option<io::Result<Event>>,
}

impl TerminalEventStream {
    pub(super) fn new() -> Self {
        #[cfg(windows)]
        let source = WindowsInput::new();
        #[cfg(not(windows))]
        let source = EventStream::new();
        Self::from_source(source)
    }
}

#[cfg(windows)]
pub(super) struct WindowsInput {
    events: tokio::sync::mpsc::Receiver<(std::time::Instant, io::Result<Event>)>,
    reader: Option<std::thread::JoinHandle<()>>,
}

#[cfg(windows)]
impl WindowsInput {
    fn new() -> Self {
        use crossterm::event;
        use std::time::{Duration, Instant};

        // Keep reading while the UI paints or handles a daemon event. A
        // bounded queue preserves ordering and caps memory during key bursts.
        let (sender, events) = tokio::sync::mpsc::channel(128);
        let reader = std::thread::spawn(move || {
            while !sender.is_closed() {
                let event = match event::poll(Duration::from_millis(25)) {
                    Ok(false) => continue,
                    Ok(true) if !sender.is_closed() => event::read(),
                    Ok(true) => break,
                    Err(error) => Err(error),
                };
                let failed = event.is_err();
                if sender.blocking_send((Instant::now(), event)).is_err() {
                    break;
                }
                if failed {
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        });
        Self {
            events,
            reader: Some(reader),
        }
    }
}

#[cfg(windows)]
impl Stream for WindowsInput {
    type Item = io::Result<Event>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.events.poll_recv(cx) {
            Poll::Ready(Some((read_at, event))) => {
                if matches!(&event, Ok(Event::Key(key)) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat))
                {
                    super::ui::note_key_event_read_at(read_at);
                }
                Poll::Ready(Some(event))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsInput {
    fn drop(&mut self) {
        self.events.close();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Default for TerminalEventStream {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> ImmediateEvents<S> {
    pub(super) fn from_source(source: S) -> Self {
        Self {
            source,
            buffered: None,
        }
    }
}

pub(super) fn is_plain_text_key(key: KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        && matches!(key.code, KeyCode::Char(c) if !c.is_control())
        && (key.modifiers - KeyModifiers::SHIFT).is_empty()
}

impl<S: Stream<Item = io::Result<Event>> + Unpin> ImmediateEvents<S> {
    /// Read only an already-ready printable key or character release. Leave
    /// other events in order for the caller's regular dispatch path.
    pub(super) fn next_ready_text_key(&mut self) -> Option<KeyEvent> {
        match self.next().now_or_never()?? {
            Ok(Event::Key(key))
                if is_plain_text_key(key)
                    || (key.kind == KeyEventKind::Release
                        && matches!(key.code, KeyCode::Char(_))) =>
            {
                Some(key)
            }
            other => {
                self.buffered = Some(other);
                None
            }
        }
    }
}

impl<S: Stream<Item = io::Result<Event>> + Unpin> Stream for ImmediateEvents<S> {
    type Item = io::Result<Event>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let stream = self.get_mut();
        if let Some(event) = stream.buffered.take() {
            return Poll::Ready(Some(event));
        }
        Pin::new(&mut stream.source).poll_next(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn text_burst_leaves_submit_and_following_text_in_order() {
        let text = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut stream = ImmediateEvents::from_source(futures::stream::iter([
            Ok(Event::Key(text)),
            Ok(enter.clone()),
            Ok(Event::Key(text)),
        ]));
        assert_eq!(stream.next_ready_text_key(), Some(text));
        assert_eq!(stream.next_ready_text_key(), None);
        assert_eq!(stream.next().await.unwrap().unwrap(), enter);
        assert_eq!(stream.next_ready_text_key(), Some(text));
    }

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
