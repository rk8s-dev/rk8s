use std::{borrow::Cow, collections::BTreeMap, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    DgBld0001NodeNotFound,
    DgBld0002DuplicateEdge,
    DgBld0003DuplicateNodeId,
    DgBld0004GraphLoopDetected,
    DgBld0005ConcurrentBuildMutation,

    DgRun0001TaskPanicked,
    DgRun0002TaskJoinFailed,
    DgRun0003LoopLimitExceeded,
    DgRun0004GraphNotActive,
    DgRun0005Aborted,
    DgRun0006NodeExecutionFailed,

    DgChk0001StoreNotConfigured,
    DgChk0002CheckpointNotFound,
    DgChk0003InvalidCheckpoint,
    DgChk0004CheckpointIo,

    DgChn0001NoSuchChannel,
    DgChn0002Closed,
    DgChn0003Lagged,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DgBld0001NodeNotFound => "DgBld0001",
            Self::DgBld0002DuplicateEdge => "DgBld0002",
            Self::DgBld0003DuplicateNodeId => "DgBld0003",
            Self::DgBld0004GraphLoopDetected => "DgBld0004",
            Self::DgBld0005ConcurrentBuildMutation => "DgBld0005",
            Self::DgRun0001TaskPanicked => "DgRun0001",
            Self::DgRun0002TaskJoinFailed => "DgRun0002",
            Self::DgRun0003LoopLimitExceeded => "DgRun0003",
            Self::DgRun0004GraphNotActive => "DgRun0004",
            Self::DgRun0005Aborted => "DgRun0005",
            Self::DgRun0006NodeExecutionFailed => "DgRun0006",
            Self::DgChk0001StoreNotConfigured => "DgChk0001",
            Self::DgChk0002CheckpointNotFound => "DgChk0002",
            Self::DgChk0003InvalidCheckpoint => "DgChk0003",
            Self::DgChk0004CheckpointIo => "DgChk0004",
            Self::DgChn0001NoSuchChannel => "DgChn0001",
            Self::DgChn0002Closed => "DgChn0002",
            Self::DgChn0003Lagged => "DgChn0003",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ErrorContext {
    pub node_id: Option<usize>,
    pub node_name: Option<String>,
    pub channel_id: Option<usize>,
    pub checkpoint_id: Option<String>,
    pub details: BTreeMap<Cow<'static, str>, String>,
}

impl ErrorContext {
    pub fn is_empty(&self) -> bool {
        self.node_id.is_none()
            && self.node_name.is_none()
            && self.channel_id.is_none()
            && self.checkpoint_id.is_none()
            && self.details.is_empty()
    }
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(node_id) = self.node_id {
            parts.push(format!("node_id={node_id}"));
        }
        if let Some(node_name) = &self.node_name {
            parts.push(format!("node_name={node_name}"));
        }
        if let Some(channel_id) = self.channel_id {
            parts.push(format!("channel_id={channel_id}"));
        }
        if let Some(checkpoint_id) = &self.checkpoint_id {
            parts.push(format!("checkpoint_id={checkpoint_id}"));
        }
        for (key, value) in &self.details {
            parts.push(format!("{key}={value}"));
        }
        f.write_str(&parts.join(", "))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DagrsError {
    pub code: ErrorCode,
    pub message: Cow<'static, str>,
    pub context: Box<ErrorContext>,
}

pub type DagrsResult<T> = Result<T, DagrsError>;

impl DagrsError {
    pub fn new(code: ErrorCode, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            code,
            message: message.into(),
            context: Box::default(),
        }
    }

    pub fn with_node(mut self, node_id: usize, node_name: impl Into<String>) -> Self {
        self.context.node_id = Some(node_id);
        self.context.node_name = Some(node_name.into());
        self
    }

    pub fn with_node_id(mut self, node_id: usize) -> Self {
        self.context.node_id = Some(node_id);
        self
    }

    pub fn with_channel(mut self, channel_id: usize) -> Self {
        self.context.channel_id = Some(channel_id);
        self
    }

    pub fn with_checkpoint(mut self, checkpoint_id: impl Into<String>) -> Self {
        self.context.checkpoint_id = Some(checkpoint_id.into());
        self
    }

    pub fn with_detail(
        mut self,
        key: impl Into<Cow<'static, str>>,
        value: impl Into<String>,
    ) -> Self {
        self.context.details.insert(key.into(), value.into());
        self
    }

    pub fn aborted() -> Self {
        Self::new(
            ErrorCode::DgRun0005Aborted,
            "graph execution was aborted by flow control",
        )
    }
}

impl fmt::Display for DagrsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.context.is_empty() {
            write!(f, "[{}] {}", self.code, self.message)
        } else {
            write!(f, "[{}] {} ({})", self.code, self.message, self.context)
        }
    }
}

impl std::error::Error for DagrsError {}
