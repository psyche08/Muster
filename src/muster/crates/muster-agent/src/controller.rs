//! A controller drives one session: sends prompts, answers approvals,
//! interrupts. Only the NativeApp Accessibility controller is attached by
//! musterd from the platform layer. Provider CLI and terminal control are absent.

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::Error;
use crate::model::{ControllerKind, PendingInteraction, SessionState};

/// What a controller reports back to the service.
#[derive(Clone, Debug)]
pub enum ControllerEvent {
    State(SessionState),
    Interaction(PendingInteraction),
    Resolved {
        interaction: String,
    },
    /// The controller is gone (process exited, app closed).
    Detached {
        process: ProcessIdentity,
    },
}

/// PID plus launch generation: PID reuse must never keep an old session alive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessState {
    Running,
    Ended,
    Unknown,
}

impl ProcessState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Ended => "ended",
            Self::Unknown => "unknown",
        }
    }
}

impl ProcessIdentity {
    /// OS adapter reports current launch time, confirmed absence, or read failure.
    pub fn observe(self, current: Result<Option<u64>, ()>) -> ProcessState {
        match current {
            Ok(Some(start)) if start == self.start => ProcessState::Running,
            Ok(_) => ProcessState::Ended,
            Err(()) => ProcessState::Unknown,
        }
    }
}

pub type ControllerEvents = mpsc::Sender<(String, ControllerEvent)>;

#[async_trait]
pub trait Controller: Send + Sync {
    fn kind(&self) -> ControllerKind;
    fn session_id(&self) -> String;
    fn state(&self) -> SessionState;
    fn process_identity(&self) -> ProcessIdentity;
    fn process_state(&self) -> ProcessState;
    fn pending(&self) -> Vec<PendingInteraction>;
    /// Whether every press that may deliver a mutation is preceded by
    /// `control_guard::delivery_attempt`. Only then can an interrupted
    /// operation be reported as not started instead of unknown.
    fn reports_delivery(&self) -> bool {
        false
    }
    /// Read-only check of the native question controls. Never chooses/submits.
    fn question_diagnostic(&self) -> Result<String, Error> {
        Err(Error::Unavailable(
            "Native question diagnostics unavailable".into(),
        ))
    }
    async fn model_options(
        &self,
        _set: Option<(&str, &str)>,
    ) -> Result<muster_proto::v1::AgentChatOptions, Error> {
        Err(Error::Unavailable(
            "Model and effort choices are unavailable for this controller".into(),
        ))
    }
    async fn complete(&self) -> Result<String, Error> {
        Err(Error::Unavailable(
            "Native prompt completion is unavailable".into(),
        ))
    }
    async fn send(&self, text: &str) -> Result<(), Error>;
    /// The user checked the desktop app and found the unconfirmed earlier
    /// write `operation` names not delivered: stop it blocking this session.
    /// Controllers without such a latch have nothing to release.
    fn release_unconfirmed(&self, _operation: &str) -> Result<(), Error> {
        Ok(())
    }
    /// Sends, naming the session the text landed in when that is another
    /// one (the first send of a new desktop chat becomes its conversation).
    async fn send_to(&self, text: &str) -> Result<Option<String>, Error> {
        self.send(text).await.map(|()| None)
    }
    /// `option` is one of the interaction's options; `answer` is free text
    /// for questions.
    async fn respond(&self, interaction: &str, option: &str, answer: &str) -> Result<(), Error>;
    async fn answer_questions(
        &self,
        _interaction: &str,
        _answers: &[muster_proto::v1::QuestionAnswer],
    ) -> Result<(), Error> {
        Err(Error::Unavailable(
            "Question forms are not supported by this controller".into(),
        ))
    }
    async fn send_queued(&self, _interaction: &str) -> Result<(), Error> {
        Err(Error::Unavailable(
            "Queued messages are unavailable for this controller".into(),
        ))
    }
    async fn interrupt(&self) -> Result<(), Error>;
    async fn stop(&self);
}
