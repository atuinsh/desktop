use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum WorkflowCommand {
    RunBlock { id: Uuid },
    StopBlock { id: Uuid },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum WorkflowEvent {
    BlockStarted { id: Uuid },
    BlockFinished { id: Uuid },

    WorkflowStarted { id: Uuid },
    WorkflowFinished { id: Uuid },
}
