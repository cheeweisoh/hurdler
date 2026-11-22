use std::time::Duration;

use crossterm::event::{Event as CrosstermEvent, EventStream, KeyEvent, KeyEventKind};
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::{
    app::PickerOption,
    domain::issue::{FieldChange, FieldId, Issue, User},
};

#[derive(Debug)]
pub enum Event {
    Tick,
    SetupTested(Result<String, String>),
    MeFetched(User),
    Key(KeyEvent),
    IssuesFetched {
        issues: Vec<Issue>,
        next_page_token: Option<String>,
        generation: u64,
    },
    IssuesFetchFailed {
        message: String,
        generation: u64,
    },
    MoreIssuesFetched {
        issues: Vec<Issue>,
        next_page_token: Option<String>,
        generation: u64,
    },
    MoreIssuesFailed {
        message: String,
        generation: u64,
    },
    PickerOptionsFetched {
        field: FieldId,
        options: Vec<PickerOption>,
    },
    PickerOptionsFailed(String),
    FieldUpdated(FieldChange),
    FieldUpdateFailed {
        change: FieldChange,
        message: String,
    },
    IssueCreated(Box<Issue>),
    IssueCreateFailed {
        key: Option<String>,
        message: String,
    },
    IssueDeleted(String),
    IssueDeleteFailed {
        key: String,
        message: String,
    },
}

pub struct EventHandler {
    sender: mpsc::UnboundedSender<Event>,
    receiver: mpsc::UnboundedReceiver<Event>,
    tick_rate: Duration,
    input: Option<JoinHandle<()>>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut handler = Self {
            sender,
            receiver,
            tick_rate,
            input: None,
        };
        handler.resume();
        handler
    }

    pub fn resume(&mut self) {
        let tx = self.sender.clone();
        let tick_rate = self.tick_rate;
        self.input = Some(tokio::spawn(async move {
            let mut reader = EventStream::new();
            let mut ticker = tokio::time::interval(tick_rate);

            loop {
                let event = tokio::select! {
                    _ = ticker.tick() => Event::Tick,
                    Some(Ok(evt)) = reader.next() => match evt {
                        CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => Event::Key(key),
                        _ => continue,
                    },
                };
                if tx.send(event).is_err() {
                    break;
                }
            }
        }));
    }

    pub async fn pause(&mut self) {
        if let Some(handle) = self.input.take() {
            handle.abort();
            let _ = handle.await;
        }
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<Event> {
        self.sender.clone()
    }

    pub async fn next(&mut self) -> Option<Event> {
        self.receiver.recv().await
    }
}
