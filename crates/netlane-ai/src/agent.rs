//! Typed conversation contract adapted from termIAX ACP; capture context replaces SSH context.
//! Original and adaptation: Copyright (c) 2026 Paulo Gomes. All rights reserved.
use tokio::sync::mpsc;
pub use tokio::sync::mpsc::error::TryRecvError;
pub type EventReceiver = mpsc::Receiver<Event>;
pub fn events() -> (mpsc::Sender<Event>, EventReceiver) {
    mpsc::channel(64)
}
pub type ConversationId = u64;
#[derive(Debug)]
pub enum Command {
    Login,
    CancelLogin,
    Logout,
    Prompt {
        conversation: ConversationId,
        text: String,
    },
    Cancel {
        conversation: ConversationId,
    },
    Reset {
        conversation: ConversationId,
    },
    SetModel(ModelChoice),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelOption {
    pub id: String,
    pub label: String,
    pub efforts: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChoice {
    pub model: String,
    pub effort: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    LoginRequired,
    LoggedOut,
    LoginUrl {
        url: String,
    },
    Ready {
        agent: String,
    },
    Text {
        conversation: ConversationId,
        text: String,
    },
    Blocked {
        conversation: ConversationId,
        what: String,
    },
    Done {
        conversation: ConversationId,
        note: Option<String>,
    },
    Failed {
        conversation: Option<ConversationId>,
        message: String,
    },
    Models {
        options: Vec<ModelOption>,
        selected: ModelChoice,
    },
    Closed {
        message: String,
    },
}
pub struct AgentHandle {
    pub(crate) commands: mpsc::Sender<Command>,
}
impl AgentHandle {
    pub fn send(&self, command: Command) -> Result<(), AcpError> {
        self.commands
            .try_send(command)
            .map_err(|_| AcpError::Closed)
    }
}
#[derive(Debug)]
pub enum AcpError {
    Start(String),
    Closed,
}
impl std::fmt::Display for AcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start(s) => f.write_str(s),
            Self::Closed => f.write_str("Conexão indisponível ou fila ocupada."),
        }
    }
}
impl std::error::Error for AcpError {}
