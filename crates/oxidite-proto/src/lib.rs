//! Wire protocol types and MessagePack framing for Oxidite.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;

pub type SessionId = uuid::Uuid;
pub type Offset = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermSize {
    pub rows: u16,
    pub cols: u16,
}

impl Default for TermSize {
    fn default() -> Self {
        Self { rows: 24, cols: 80 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachMode {
    Writer,
    Viewer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplayStart {
    Replay { from: Offset },
    Snapshot { at: Offset },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Multiplexer {
    None,
    Tmux,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthSpec {
    Password(String),
    PublicKey {
        key_path: String,
        passphrase: Option<String>,
    },
    Interactive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellCapability {
    pub id: String,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendSpec {
    LocalPty {
        shell: Option<String>,
        cwd: Option<String>,
    },
    Ssh {
        host: String,
        port: u16,
        user: String,
        auth: AuthSpec,
        multiplexer: Multiplexer,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Starting,
    Running,
    Degraded,
    Reconnecting { attempt: u32 },
    Detached,
    Exited { code: Option<i32> },
    Failed { reason: String },
}

impl SessionState {
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Exited { .. } | Self::Failed { .. })
    }

    #[must_use]
    pub fn can_transition_to(&self, next: &SessionState) -> bool {
        if self.is_terminal() {
            return false;
        }
        matches!(
            (self, next),
            (
                Self::Starting,
                Self::Running | Self::Failed { .. } | Self::Exited { .. }
            ) | (
                Self::Running,
                Self::Degraded | Self::Detached | Self::Exited { .. } | Self::Failed { .. },
            ) | (
                Self::Degraded,
                Self::Running
                    | Self::Reconnecting { .. }
                    | Self::Exited { .. }
                    | Self::Failed { .. },
            ) | (
                Self::Reconnecting { .. },
                Self::Running
                    | Self::Reconnecting { .. }
                    | Self::Failed { .. }
                    | Self::Exited { .. },
            ) | (
                Self::Detached,
                Self::Running | Self::Exited { .. } | Self::Failed { .. }
            )
        )
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Starting => "Starting",
            Self::Running => "Running",
            Self::Degraded => "Degraded",
            Self::Reconnecting { .. } => "Reconnecting",
            Self::Detached => "Detached",
            Self::Exited { .. } => "Exited",
            Self::Failed { .. } => "Failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostKeyStatus {
    Unknown,
    Trusted,
    Mismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    ProtocolMismatch,
    Unauthorized,
    SessionNotFound,
    SessionExited,
    BackendError,
    BufferExceeded,
    InvalidMessage,
    InternalError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientMsg {
    Hello {
        protocol_version: u16,
        token: String,
    },
    Create {
        backend: BackendSpec,
        size: TermSize,
    },
    Attach {
        session: SessionId,
        last_offset: Option<Offset>,
        mode: AttachMode,
    },
    Input(#[serde(with = "serde_bytes")] Vec<u8>),
    Resize(TermSize),
    Ack {
        offset: Offset,
    },
    HostKeyDecision {
        accept: bool,
    },
    AuthResponse {
        answers: Vec<String>,
    },
    Ping {
        nonce: u64,
    },
    TakeoverWriter,
    Detach,
    Kill,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome {
        server_version: String,
    },
    Created {
        session: SessionId,
    },
    Attached {
        session: SessionId,
        mode: AttachMode,
        start: ReplayStart,
        /// This connection's own id, the same value later reported as `Presence::writer_id` when
        /// this connection holds write control. Optional so older peers still decode.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_id: Option<String>,
    },
    Snapshot {
        offset: Offset,
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
    },
    Output {
        offset: Offset,
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
    },
    State {
        session: SessionId,
        state: SessionState,
    },
    HostKeyPrompt {
        host: String,
        fingerprint: String,
        known: HostKeyStatus,
    },
    AuthPrompt {
        prompts: Vec<String>,
    },
    Title(String),
    Exit {
        code: Option<i32>,
    },
    Pong {
        nonce: u64,
    },
    Presence {
        session: SessionId,
        writer_id: Option<String>,
        viewers_count: usize,
    },
    Error {
        code: ErrorCode,
        message: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    #[error("MessagePack encode error: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("MessagePack decode error: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
}

/// Encodes a client message using named MessagePack fields.
pub fn pack_client_msg(msg: &ClientMsg) -> Result<Vec<u8>, ProtoError> {
    rmp_serde::to_vec_named(msg).map_err(ProtoError::Encode)
}

/// Decodes a client message from MessagePack bytes.
pub fn unpack_client_msg(bytes: &[u8]) -> Result<ClientMsg, ProtoError> {
    rmp_serde::from_slice(bytes).map_err(ProtoError::Decode)
}

/// Encodes a server message using named MessagePack fields.
pub fn pack_server_msg(msg: &ServerMsg) -> Result<Vec<u8>, ProtoError> {
    rmp_serde::to_vec_named(msg).map_err(ProtoError::Encode)
}

/// Decodes a server message from MessagePack bytes.
pub fn unpack_server_msg(bytes: &[u8]) -> Result<ServerMsg, ProtoError> {
    rmp_serde::from_slice(bytes).map_err(ProtoError::Decode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_msg_roundtrip() {
        let msg = ClientMsg::Hello {
            protocol_version: PROTOCOL_VERSION,
            token: "secret-token".into(),
        };
        let bytes = pack_client_msg(&msg).unwrap();
        let decoded = unpack_client_msg(&bytes).unwrap();
        assert_eq!(msg, decoded);

        let input_msg = ClientMsg::Input(b"ls -la\n".to_vec());
        let bytes = pack_client_msg(&input_msg).unwrap();
        let decoded = unpack_client_msg(&bytes).unwrap();
        assert_eq!(input_msg, decoded);
    }

    #[test]
    fn test_session_state_transitions() {
        let s0 = SessionState::Starting;
        let s1 = SessionState::Running;
        let s_deg = SessionState::Degraded;
        let s_rec = SessionState::Reconnecting { attempt: 1 };
        let s_det = SessionState::Detached;
        let s_fail = SessionState::Failed {
            reason: "lost".into(),
        };
        let s_exit = SessionState::Exited { code: Some(0) };

        assert!(s0.can_transition_to(&s1));
        assert!(s1.can_transition_to(&s_deg));
        assert!(s_deg.can_transition_to(&s_rec));
        assert!(s_rec.can_transition_to(&s1));
        assert!(s1.can_transition_to(&s_det));
        assert!(s_det.can_transition_to(&s1));
        assert!(s1.can_transition_to(&s_exit));

        // Terminal states cannot transition anywhere
        assert!(!s_exit.can_transition_to(&s1));
        assert!(!s_fail.can_transition_to(&s1));
    }

    #[test]
    fn test_server_msg_roundtrip() {
        let sid = uuid::Uuid::new_v4();
        let msg = ServerMsg::Attached {
            session: sid,
            mode: AttachMode::Writer,
            start: ReplayStart::Snapshot { at: 1024 },
            client_id: Some("7".into()),
        };
        let bytes = pack_server_msg(&msg).unwrap();
        let decoded = unpack_server_msg(&bytes).unwrap();
        assert_eq!(msg, decoded);

        // A peer that predates `client_id` omits the field; it must still decode as None.
        let legacy = ServerMsg::Attached {
            session: sid,
            mode: AttachMode::Viewer,
            start: ReplayStart::Snapshot { at: 1 },
            client_id: None,
        };
        let legacy_bytes = pack_server_msg(&legacy).unwrap();
        assert!(!legacy_bytes.windows(9).any(|w| w == b"client_id"));
        assert_eq!(unpack_server_msg(&legacy_bytes).unwrap(), legacy);

        let out = ServerMsg::Output {
            offset: 42,
            bytes: b"\x1b[32mhello\x1b[0m".to_vec(),
        };
        let bytes = pack_server_msg(&out).unwrap();
        let decoded = unpack_server_msg(&bytes).unwrap();
        assert_eq!(out, decoded);
    }
}
