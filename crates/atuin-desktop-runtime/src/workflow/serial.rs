use crate::blocks::Block;
use crate::workflow::event::{WorkflowCommand, WorkflowEvent};
use tokio::sync::broadcast;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;

pub async fn serial_execute(
    workflow: Vec<Block>,
    cancel_channel: oneshot::Receiver<()>,
    send_command: Sender<WorkflowCommand>,
    mut recv_event: broadcast::Receiver<WorkflowEvent>,
) {
    // 1. Kick off the first block in the workflow
    // 2. When we receive the finish event for it, kick off the next block
    // 3. Repeat until the workflow is complete
    // 4. Listen on the cancel channel for a stop event. Terminate the current block and exit.
    let mut iter = workflow.iter();
    println!("serial workflow: {workflow:?}");

    if let Some(block) = iter.next() {
        println!("running block: {block:?}");
        send_command
            .send(WorkflowCommand::RunBlock { id: block.id() })
            .await
            .expect("Failed to send run block event");
    }

    let mut cancel_fut = cancel_channel;

    loop {
        tokio::select! {
            Ok(()) = &mut cancel_fut => {
                println!("Workflow cancelled");

                // Send stop command to all blocks
                for block in workflow.iter() {
                    send_command.send(WorkflowCommand::StopBlock { id: block.id() }).await
                        .expect("Failed to send stop block event");
                }

                // Terminate the workflow
                return;
            }

            Ok(event) = recv_event.recv() => {
                match event {
                    WorkflowEvent::BlockStarted { id } => {
                        println!("block {id} started");
                    }
                    WorkflowEvent::BlockFinished { id } => {
                        println!("block {id} finished");

                        // Get the next block in the workflow
                        if let Some(block) = iter.next() {
                            send_command.send(WorkflowCommand::RunBlock { id: block.id() }).await
                                .expect("Failed to send run block event");
                        } else {
                            // The workflow is complete
                            break;
                        }
                    }

                    // Not relevant in this function
                    _ => {}
                }
            }
        }
    }
}
