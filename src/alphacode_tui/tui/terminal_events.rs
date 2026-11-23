//! Forward terminal events immediately, without inferring paste from typing speed.
//!
//! Ctrl+V/Alt+V are handled by the shared asynchronous clipboard action. Native
//! bracketed paste arrives as one Event::Paste. Ordinary keys, including Enter,
//! must never wait for another character or a paste-settling timer.
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
use crossterm::event::EventStream;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::{FutureExt, Stream, StreamExt, stream::Fuse};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub(super) type TerminalEventStream = ImmediateEvents<EventStream>;
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
pub(super) type TerminalEventStream = ImmediateEvents<QueuedInput>;

pub(super) struct ImmediateEvents<S> {
    source: Fuse<S>,
    buffered: Option<ReadEvent>,
    eof_reported: bool,
}

pub(super) struct ReadEvent {
    read_at: Instant,
    event: io::Result<Event>,
}

impl From<io::Result<Event>> for ReadEvent {
    fn from(event: io::Result<Event>) -> Self {
        Self {
            read_at: Instant::now(),
            event,
        }
    }
}

impl ReadEvent {
    fn dispatch(self) -> io::Result<Event> {
        if matches!(&self.event, Ok(Event::Key(key)) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat))
        {
            super::ui::note_key_event_read_at(self.read_at);
        }
        self.event
    }
}

impl TerminalEventStream {
    pub(super) fn new() -> Self {
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        let source = QueuedInput::new();
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        let source = EventStream::new();
        Self::from_source(source)
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
pub(super) struct QueuedInput {
    events: tokio::sync::mpsc::Receiver<ReadEvent>,
    #[cfg(not(target_os = "linux"))]
    reader: Option<std::thread::JoinHandle<()>>,
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
impl QueuedInput {
    fn new() -> Self {
        use super::app::terminal_liveness;
        use crossterm::event;
        use std::time::Duration;

        terminal_liveness::capture_initial_terminal();
        // Keep reading while the UI paints or handles a daemon event. A
        // bounded queue preserves ordering and caps memory during key bursts.
        let (sender, events) = tokio::sync::mpsc::channel(128);
        let poll_timeout = if cfg!(windows) {
            Duration::from_millis(25)
        } else if cfg!(target_os = "macos") {
            // macOS Terminal / iTerm2 can deliver a burst of key events
            // immediately after a window resize or focus gain. A shorter
            // poll keeps the worker responsive without starving the UI.
            Duration::from_millis(50)
        } else {
            Duration::from_millis(100)
        };
        let reader = std::thread::spawn(move || {
            while !sender.is_closed() {
                let event = match event::poll(poll_timeout) {
                    Ok(false) => continue,
                    Ok(true) if !sender.is_closed() => event::read(),
                    Ok(true) => break,
                    Err(error) => Err(error),
                };
                let failed = event.is_err();
                if failed && terminal_liveness::terminal_abandoned() {
                    // Drop the sender after queued inputs are delivered. It
                    // reports EOF once instead of waking the UI with retries.
                    break;
                }
                if sender.blocking_send(ReadEvent::from(event)).is_err() {
                    break;
                }
                if failed {
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        });
        // On Linux an incomplete escape sequence or paste can leave crossterm
        // blocked inside the TTY read. Closing the queue lets the worker exit
        // after that read returns without blocking terminal teardown. On
        // macOS and Windows the reader handle is kept so Drop can join it.
        #[cfg(not(target_os = "linux"))]
        let _ = reader;
        Self {
            events,
            #[cfg(not(target_os = "linux"))]
            reader: Some(reader),
        }
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
impl Stream for QueuedInput {
    type Item = ReadEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.events.poll_recv(cx)
    }
}

#[cfg(not(target_os = "linux"))]
impl Drop for QueuedInput {
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

impl<S: Stream> ImmediateEvents<S> {
    pub(super) fn from_source(source: S) -> Self {
        Self {
            source: source.fuse(),
            buffered: None,
            eof_reported: false,
        }
    }
}

pub(super) fn is_plain_text_key(key: KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        && matches!(key.code, KeyCode::Char(c) if !c.is_control())
        && (key.modifiers - KeyModifiers::SHIFT).is_empty()
}

impl<S: Stream + Unpin> ImmediateEvents<S>
where
    S::Item: Into<ReadEvent>,
{
    /// Deliver EOF once. Keeping it ready would starve the other branches in
    /// input-first select loops after the reader has stopped.
    pub(super) async fn next_live(&mut self) -> Option<io::Result<Event>> {
        if self.eof_reported {
            return std::future::pending().await;
        }
        let event = self.next().await;
        self.eof_reported = event.is_none();
        event
    }

    pub(super) fn input_closed(&self) -> bool {
        self.eof_reported
    }

    fn poll_read_event(&mut self, cx: &mut Context<'_>) -> Poll<Option<ReadEvent>> {
        if let Some(event) = self.buffered.take() {
            return Poll::Ready(Some(event));
        }
        Pin::new(&mut self.source)
            .poll_next(cx)
            .map(|event| event.map(Into::into))
    }

    /// Read only an already-ready printable key or character release. Leave
    /// other events in order for the caller's regular dispatch path.
    pub(super) fn next_ready_text_key(&mut self) -> Option<KeyEvent> {
        let event = std::future::poll_fn(|cx| self.poll_read_event(cx)).now_or_never()??;
        match &event.event {
            Ok(Event::Key(key))
                if is_plain_text_key(*key)
                    || (key.kind == KeyEventKind::Release
                        && matches!(key.code, KeyCode::Char(_))) =>
            {
                let key = *key;
                let _ = event.dispatch();
                Some(key)
            }
            _ => {
                self.buffered = Some(event);
                None
            }
        }
    }
}

impl<S: Stream + Unpin> Stream for ImmediateEvents<S>
where
    S::Item: Into<ReadEvent>,
{
    type Item = io::Result<Event>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.poll_read_event(cx)
            .map(|event| event.map(ReadEvent::dispatch))
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
    async fn closed_input_cannot_starve_other_work_after_burst_lookahead() {
        let mut stream =
            ImmediateEvents::from_source(futures::stream::empty::<io::Result<Event>>());
        assert_eq!(stream.next_ready_text_key(), None);
        assert!(stream.next_live().await.is_none());
        tokio::select! {
            biased;
            _ = stream.next_live() => panic!("EOF must only be reported once"),
            _ = std::future::ready(()) => {}
        }
    }

    #[tokio::test]
    async fn lookahead_preserves_the_reader_timestamp_for_buffered_keys() {
        let read_at = Instant::now();
        let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut stream = ImmediateEvents::from_source(futures::stream::iter([ReadEvent {
            read_at,
            event: Ok(enter.clone()),
        }]));
        assert_eq!(stream.next_ready_text_key(), None);
        assert_eq!(stream.buffered.as_ref().unwrap().read_at, read_at);
        assert_eq!(stream.next_live().await.unwrap().unwrap(), enter);
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
