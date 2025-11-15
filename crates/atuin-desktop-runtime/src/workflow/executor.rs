/// Currently we only support a single, serial workflow
/// Suporting multiple won't be much of a change here, we will just need to
/// switch to broadcast channels in a few places.
/// I've done ~half the work for multiple workflows here :)
use std::collections::HashMap;

use crate::blocks::Block;
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use super::{
    event::{WorkflowCommand, WorkflowEvent},
    serial::serial_execute,
};

pub enum ExecutorMessage {
    RunWorkflow { id: Uuid, workflow: Vec<Block> },
    StopWorkflow { id: Uuid },
}

#[derive(Clone)]
pub struct ExecutorHandle {
    pub sender: mpsc::Sender<ExecutorMessage>,
}

impl ExecutorHandle {
    pub fn new(
        event_sender: broadcast::Sender<WorkflowEvent>,
        cmd_sender: mpsc::Sender<WorkflowCommand>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(8);
        let mut actor = Executor::new(receiver, event_sender, cmd_sender);

        tokio::spawn(async move { actor.run().await });

        Self { sender }
    }

    pub async fn run_workflow(&self, id: Uuid, workflow: Vec<Block>) {
        println!("running workflow: {workflow:?}");
        self.sender
            .send(ExecutorMessage::RunWorkflow { id, workflow })
            .await
            .expect("Failed to send run workflow message");
    }

    pub async fn stop_workflow(&self, id: Uuid) {
        self.sender
            .send(ExecutorMessage::StopWorkflow { id })
            .await
            .expect("Failed to send stop workflow message");
    }
}

struct WorkflowData {
    // The channel to send a cancel signal to the executor
    cancel_channel: oneshot::Sender<()>,
}

pub struct Executor {
    pub receiver: mpsc::Receiver<ExecutorMessage>,

    workflow_store: HashMap<Uuid, WorkflowData>,

    // for passing messages back to tauri/eventually other things
    // this can be global! it doesn't really matter
    event_sender: broadcast::Sender<WorkflowEvent>,
    cmd_sender: mpsc::Sender<WorkflowCommand>,
}

impl Executor {
    pub fn new(
        receiver: mpsc::Receiver<ExecutorMessage>,
        event_sender: broadcast::Sender<WorkflowEvent>,
        cmd_sender: mpsc::Sender<WorkflowCommand>,
    ) -> Self {
        Self {
            receiver,
            workflow_store: HashMap::new(),
            event_sender,
            cmd_sender,
        }
    }

    pub async fn run(&mut self) {
        while let Some(message) = self.receiver.recv().await {
            match message {
                ExecutorMessage::RunWorkflow { id, workflow } => {
                    // if the workflow is already running, we don't want to run it again
                    if self.workflow_store.contains_key(&id) {
                        println!("workflow already running, skipping");
                        continue;
                    }

                    let (cancel_channel, cancel_receiver) = oneshot::channel();

                    // we don't want to block here, so we spawn a new task
                    // Currently we assume all workflows are serial, as that is the only type we support
                    let event_receiver = self.event_sender.subscribe();
                    let cmd_sender = self.cmd_sender.clone();

                    let event_sender = self.event_sender.clone();

                    tokio::spawn(async move {
                        serial_execute(workflow, cancel_receiver, cmd_sender, event_receiver).await;

                        event_sender
                            .send(WorkflowEvent::WorkflowFinished { id })
                            .expect("Failed to send finished event");
                    });

                    println!("inserting workflow into store: {id:?}");
                    self.workflow_store
                        .insert(id, WorkflowData { cancel_channel });
                }
                ExecutorMessage::StopWorkflow { id } => {
                    println!("stopping workflow: {id:?}");
                    match self.workflow_store.remove(&id) {
                        Some(handle) => {
                            // If the cancel channel is still open, cancel the workflow. Otherwise, it has already finished
                            if !handle.cancel_channel.is_closed() {
                                if let Err(e) = handle.cancel_channel.send(()) {
                                    println!("error sending cancel signal: {e:?}");
                                }
                            }
                        }
                        _ => {
                            println!("workflow not found, skipping");
                        }
                    }
                }
            }
        }
    }
}
