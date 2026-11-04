//! Normalize terminal-native Ctrl+V (unframed key bursts) into atomic pastes.
//! Alt+V already reads one clipboard payload; native terminal paste must have
//! the same semantics before local/remote/streaming handlers see any Enter.
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures::Stream;
use std::{
    collections::VecDeque,
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::time::{Instant, Sleep};

const TYPE_SETTLE: Duration = Duration::from_millis(8);
const ENTER_SETTLE: Duration = Duration::from_millis(35);
const PASTE_SETTLE: Duration = Duration::from_millis(40);
const PASTE_BYTES: usize = 64;
const MAX_CHUNK_BYTES: usize = 64 * 1024;
const MAX_EVENTS_PER_POLL: usize = 4096;

pub(super) type TerminalEventStream = CoalescedEvents<EventStream>;

pub(super) struct CoalescedEvents<S> {
    source: S,
    ready: VecDeque<io::Result<Event>>,
    original: Vec<Event>,
    text: String,
    saw_enter: bool,
    paste: bool,
    timer: Option<Pin<Box<Sleep>>>,
    ended: bool,
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

impl<S> CoalescedEvents<S> {
    pub(super) fn from_source(source: S) -> Self {
        Self {
            source,
            ready: VecDeque::new(),
            original: Vec::new(),
            text: String::new(),
            saw_enter: false,
            paste: false,
            timer: None,
            ended: false,
        }
    }

    fn flush(&mut self) {
        if self.paste && !self.text.is_empty() {
            self.ready
                .push_back(Ok(Event::Paste(std::mem::take(&mut self.text))));
            self.original.clear();
        } else {
            self.ready.extend(self.original.drain(..).map(Ok));
            self.text.clear();
        }
        self.saw_enter = false;
    }

    fn accept(&mut self, event: Event) {
        let character = match &event {
            Event::Key(key)
                if !(key.modifiers.intersects(
                    KeyModifiers::CONTROL
                        | KeyModifiers::ALT
                        | KeyModifiers::SUPER
                        | KeyModifiers::META,
                ) || key.code == KeyCode::Enter
                    && key.modifiers.contains(KeyModifiers::SHIFT)) =>
            {
                match key.code {
                    KeyCode::Char(c) => Some(c),
                    KeyCode::Enter => Some('\n'),
                    KeyCode::Tab if key.modifiers.is_empty() => Some('\t'),
                    _ => None,
                }
            }
            _ => None,
        };
        let release = matches!(&event, Event::Key(key) if key.kind == KeyEventKind::Release);
        if let Some(character) = character {
            if release {
                if self.paste {
                    return;
                }
                if !self.original.is_empty() {
                    self.original.push(event);
                } else {
                    self.ready.push_back(Ok(event));
                }
                return;
            }
            // A short first line is sufficient: never let its Enter escape
            // merely because it did not meet a byte-count heuristic.
            if self.saw_enter
                || (character == '\n' && !self.text.is_empty())
                || self.text.len() + character.len_utf8() >= PASTE_BYTES
            {
                self.paste = true;
                self.original.clear();
            }
            self.saw_enter |= character == '\n';
            self.text.push(character);
            if !self.paste {
                self.original.push(event);
            }
            let settle = if self.paste {
                PASTE_SETTLE
            } else if self.saw_enter {
                ENTER_SETTLE
            } else {
                TYPE_SETTLE
            };
            let deadline = Instant::now() + settle;
            if let Some(timer) = &mut self.timer {
                timer.as_mut().reset(deadline);
            } else {
                self.timer = Some(Box::pin(tokio::time::sleep_until(deadline)));
            }
            if self.text.len() >= MAX_CHUNK_BYTES {
                self.flush();
            }
        } else {
            self.flush();
            self.paste = false;
            self.timer = None;
            self.ready.push_back(Ok(event));
        }
    }
}

impl<S: Stream<Item = io::Result<Event>> + Unpin> Stream for CoalescedEvents<S> {
    type Item = io::Result<Event>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if let Some(event) = this.ready.pop_front() {
            return Poll::Ready(Some(event));
        }
        for _ in 0..MAX_EVENTS_PER_POLL {
            if this.ended {
                this.flush();
                return Poll::Ready(this.ready.pop_front());
            }
            match Pin::new(&mut this.source).poll_next(cx) {
                Poll::Ready(Some(Ok(event))) => this.accept(event),
                Poll::Ready(Some(Err(error))) => {
                    this.flush();
                    this.ready.push_back(Err(error));
                }
                Poll::Ready(None) => {
                    this.ended = true;
                    this.flush();
                }
                Poll::Pending => {
                    if this
                        .timer
                        .as_mut()
                        .is_some_and(|timer| timer.as_mut().poll(cx).is_ready())
                    {
                        this.flush();
                        this.paste = false;
                        this.timer = None;
                    }
                    return this
                        .ready
                        .pop_front()
                        .map_or(Poll::Pending, |event| Poll::Ready(Some(event)));
                }
            }
            if let Some(event) = this.ready.pop_front() {
                return Poll::Ready(Some(event));
            }
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    fn keys(text: &str) -> Vec<io::Result<Event>> {
        text.chars()
            .map(|c| {
                Ok(Event::Key(crossterm::event::KeyEvent::new(
                    if c == '\n' {
                        KeyCode::Enter
                    } else {
                        KeyCode::Char(c)
                    },
                    KeyModifiers::NONE,
                )))
            })
            .collect()
    }
    #[tokio::test]
    async fn native_paste_with_short_first_line_never_exposes_enter_keys() {
        let text = "x\nsecond line\nthird line\n";
        let mut stream = CoalescedEvents::from_source(futures::stream::iter(keys(text)));
        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            Event::Paste(text.into())
        );
        assert!(stream.next().await.is_none());
    }
    #[tokio::test]
    async fn ordinary_typing_and_explicit_submit_are_preserved() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let source = futures::stream::unfold(rx, |mut rx| async {
            rx.recv().await.map(|event| (event, rx))
        });
        let mut stream = CoalescedEvents::from_source(Box::pin(source));
        for event in keys("hello\n") {
            let expected = event.as_ref().unwrap().clone();
            tx.send(event).unwrap();
            assert_eq!(stream.next().await.unwrap().unwrap(), expected);
        }
    }
    #[tokio::test]
    async fn a_single_pasted_line_with_a_trailing_enter_does_not_submit() {
        let mut stream = CoalescedEvents::from_source(futures::stream::iter(keys("hello\n")));
        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            Event::Paste("hello\n".into())
        );
    }
    #[tokio::test]
    async fn large_native_paste_is_bounded_and_preserves_every_byte() {
        let text = "a\n世界 👩‍💻\n".repeat(12_000);
        let mut stream = CoalescedEvents::from_source(futures::stream::iter(keys(&text)));
        let mut actual = String::new();
        while let Some(event) = stream.next().await {
            let Event::Paste(chunk) = event.unwrap() else {
                panic!("paste leaked a key event")
            };
            assert!(chunk.len() <= MAX_CHUNK_BYTES + 3);
            actual.push_str(&chunk);
        }
        assert_eq!(actual, text);
    }
    #[tokio::test]
    async fn bracketed_pastes_and_modified_shortcuts_are_unchanged() {
        let events = vec![
            Ok(Event::Paste("one\ntwo".into())),
            Ok(Event::Key(crossterm::event::KeyEvent::new(
                KeyCode::Char('v'),
                KeyModifiers::ALT,
            ))),
            Ok(Event::Key(crossterm::event::KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::SHIFT,
            ))),
        ];
        let mut stream = CoalescedEvents::from_source(futures::stream::iter(events));
        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            Event::Paste("one\ntwo".into())
        );
        assert!(
            matches!(stream.next().await.unwrap().unwrap(), Event::Key(key) if key.modifiers == KeyModifiers::ALT)
        );
        assert!(
            matches!(stream.next().await.unwrap().unwrap(), Event::Key(key) if key.modifiers == KeyModifiers::SHIFT)
        );
    }
    #[tokio::test]
    async fn enter_waits_for_the_next_paste_chunk_without_reaching_submit() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let source = futures::stream::unfold(rx, |mut rx| async {
            rx.recv().await.map(|event| (event, rx))
        });
        let mut stream = CoalescedEvents::from_source(Box::pin(source));
        for event in keys("x\n") {
            tx.send(event).unwrap();
        }
        let incoming = async move {
            tokio::time::sleep(Duration::from_millis(12)).await;
            for event in keys("next line\n") {
                tx.send(event).unwrap();
            }
        };
        let (first, _) = tokio::join!(stream.next(), incoming);
        assert_eq!(
            first.unwrap().unwrap(),
            Event::Paste("x\nnext line\n".into())
        );
    }
}
