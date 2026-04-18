use std::time::SystemTime;

use crate::domain::ids::NodeId;

#[derive(Clone, Debug)]
pub struct TracerouteResult {
    pub target: NodeId,
    pub route: Vec<NodeId>,
    pub snr_towards_db: Vec<f32>,
    pub route_back: Vec<NodeId>,
    pub snr_back_db: Vec<f32>,
    pub completed_at: SystemTime,
}

#[derive(Clone, Debug)]
pub enum TracerouteState {
    Pending { started_at: SystemTime },
    Done(TracerouteResult),
    Failed { target: NodeId, reason: String },
}

impl TracerouteState {
    pub const fn target(&self) -> NodeId {
        match self {
            Self::Pending { .. } => NodeId(0),
            Self::Done(r) => r.target,
            Self::Failed { target, .. } => *target,
        }
    }
}
