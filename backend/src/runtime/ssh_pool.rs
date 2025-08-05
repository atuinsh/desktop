// An actor for managing SSH connections

use std::collections::HashMap;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::{mpsc, oneshot};

use crate::pty::PtyMetadata;
use crate::runtime::ssh::pool::Pool;
use crate::runtime::ssh::session::{Authentication, Session};
use eyre::Result;
use std::sync::Arc;

use super::pty_store::PtyLike;

pub struct SshPty {
    pub tx: mpsc::Sender<Bytes>,
    pub resize_tx: mpsc::Sender<(u16, u16)>,

    pub metadata: PtyMetadata,
    pub ssh_pool: SshPoolHandle,
}

#[async_trait]
impl PtyLike for SshPty {
    fn metadata(&self) -> PtyMetadata {
        self.metadata.clone()
    }

    async fn kill_child(&self) -> Result<()> {
        self.ssh_pool
            .close_pty(&self.metadata.pid.to_string())
            .await?;
        Ok(())
    }

    async fn send_bytes(&self, bytes: Bytes) -> Result<()> {
        self.tx.send(bytes).await?;
        Ok(())
    }

    async fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.resize_tx.send((rows, cols)).await?;
        Ok(())
    }
}

#[allow(dead_code)]
pub enum SshPoolMessage {
    Connect {
        host: String,
        username: Option<String>,
        auth: Option<Authentication>,
        reply_to: oneshot::Sender<Result<Arc<Session>>>,
    },
    Disconnect {
        host: String,
        username: String,
        reply_to: oneshot::Sender<Result<()>>,
    },
    ListConnections {
        reply_to: oneshot::Sender<Vec<String>>,
    },
    Len {
        reply_to: oneshot::Sender<usize>,
    },
    Exec {
        host: String,
        username: Option<String>,
        interpreter: String,
        command: String,
        channel: String,

        // The stream of output from the exec command
        output_stream: mpsc::Sender<String>,

        // The actual result of the exec command
        reply_to: oneshot::Sender<Result<()>>,

        // Stored internally and used for the corresponding exec_finished message
        result_tx: oneshot::Sender<()>,
    },
    ExecFinished {
        channel: String,
        reply_to: oneshot::Sender<Result<()>>,
    },
    ExecCancel {
        channel: String,
    },
    OpenPty {
        host: String,
        username: Option<String>,
        channel: String,
        width: u16,
        height: u16,
        // Stream to receive output from the pty
        output_stream: mpsc::Sender<String>,

        // The actual result of the open_pty command
        // returns a channel to send input to the pty
        #[allow(clippy::type_complexity)]
        reply_to: oneshot::Sender<Result<(mpsc::Sender<Bytes>, mpsc::Sender<(u16, u16)>)>>,
    },
    ClosePty {
        channel: String,
    },
    PtyWrite {
        channel: String,
        input: Bytes,
        reply_to: oneshot::Sender<Result<()>>,
    },
}

#[derive(Clone, Debug)]
pub struct SshPoolHandle {
    sender: mpsc::Sender<SshPoolMessage>,
}

impl SshPoolHandle {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(8);
        let mut actor = SshPool::new(sender.clone(), receiver);

        tauri::async_runtime::spawn(async move { actor.run().await });

        Self::new_handle(sender)
    }

    pub fn new_handle(sender: mpsc::Sender<SshPoolMessage>) -> Self {
        Self { sender }
    }

    pub async fn connect(
        &self,
        host: &str,
        username: Option<&str>,
        auth: Option<Authentication>,
    ) -> Result<Arc<Session>> {
        let (sender, receiver) = oneshot::channel();
        let msg = SshPoolMessage::Connect {
            host: host.to_string(),
            username: username.map(|s| s.to_string()),
            auth,
            reply_to: sender,
        };

        let _ = self.sender.send(msg).await;

        receiver.await.unwrap()
    }

    pub async fn disconnect(&self, host: &str, username: &str) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        let msg = SshPoolMessage::Disconnect {
            host: host.to_string(),
            username: username.to_string(),
            reply_to: sender,
        };

        let _ = self.sender.send(msg).await;
        receiver.await.unwrap()
    }

    pub async fn list_connections(&self) -> Result<Vec<String>, oneshot::error::RecvError> {
        let (sender, receiver) = oneshot::channel();
        let msg = SshPoolMessage::ListConnections { reply_to: sender };

        let _ = self.sender.send(msg).await;
        receiver.await
    }

    #[allow(dead_code)]
    pub async fn len(&self) -> Result<usize, oneshot::error::RecvError> {
        let (sender, receiver) = oneshot::channel();
        let msg = SshPoolMessage::Len { reply_to: sender };

        let _ = self.sender.send(msg).await;
        receiver.await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn exec(
        &self,
        host: &str,
        username: Option<&str>,
        interpreter: &str,
        command: &str,
        channel: &str,
        output_stream: mpsc::Sender<String>,
        result_tx: oneshot::Sender<()>,
    ) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        let msg = SshPoolMessage::Exec {
            host: host.to_string(),
            username: username.map(|u| u.to_string()),
            interpreter: interpreter.to_string(),
            command: command.to_string(),
            channel: channel.to_string(),
            output_stream,
            reply_to: sender,
            result_tx,
        };

        let _ = self.sender.send(msg).await;
        receiver.await?
    }

    pub async fn exec_finished(&self, channel: &str) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        let msg = SshPoolMessage::ExecFinished {
            channel: channel.to_string(),
            reply_to: sender,
        };

        let _ = self.sender.send(msg).await;
        receiver.await.unwrap()
    }

    pub async fn exec_cancel(&self, channel: &str) -> Result<()> {
        let msg = SshPoolMessage::ExecCancel {
            channel: channel.to_string(),
        };

        let _ = self.sender.send(msg).await;
        Ok(())
    }

    pub async fn open_pty(
        &self,
        host: &str,
        username: Option<&str>,
        channel: &str,
        output_stream: mpsc::Sender<String>,
        width: u16,
        height: u16,
    ) -> Result<(mpsc::Sender<Bytes>, mpsc::Sender<(u16, u16)>)> {
        let (reply_sender, reply_receiver) = oneshot::channel();

        let msg = SshPoolMessage::OpenPty {
            host: host.to_string(),
            username: username.map(|u| u.to_string()),
            channel: channel.to_string(),
            output_stream,
            reply_to: reply_sender,
            width,
            height,
        };

        let _ = self.sender.send(msg).await;
        reply_receiver.await?
    }

    pub async fn pty_write(&self, channel: &str, input: Bytes) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        let msg = SshPoolMessage::PtyWrite {
            channel: channel.to_string(),
            input: input.clone(),
            reply_to: sender,
        };

        let _ = self.sender.send(msg).await;
        receiver.await?
    }

    pub async fn close_pty(&self, channel: &str) -> Result<()> {
        let msg = SshPoolMessage::ClosePty {
            channel: channel.to_string(),
        };
        let _ = self.sender.send(msg).await;
        Ok(())
    }
}

impl Default for SshPoolHandle {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
pub struct ChannelMeta {
    pub host: String,
    pub username: String,
    pub cancel_tx: oneshot::Sender<()>,
    pub result_tx: oneshot::Sender<()>,
    pub pty_input_tx: Option<mpsc::Sender<Bytes>>,
}

pub(crate) struct SshPool {
    pub sender: mpsc::Sender<SshPoolMessage>,
    pub receiver: mpsc::Receiver<SshPoolMessage>,

    pub channels: HashMap<String, ChannelMeta>,

    pool: Pool,
}

impl SshPool {
    pub fn new(
        sender: mpsc::Sender<SshPoolMessage>,
        receiver: mpsc::Receiver<SshPoolMessage>,
    ) -> Self {
        Self {
            sender,
            receiver,
            pool: Pool::new(),
            channels: HashMap::new(),
        }
    }

    pub fn handle(&self) -> SshPoolHandle {
        SshPoolHandle::new_handle(self.sender.clone())
    }

    async fn run(&mut self) {
        while let Some(msg) = self.receiver.recv().await {
            self.handle_message(msg).await;

            log::debug!("SshPool Message handled");
        }
    }

    async fn handle_message(&mut self, message: SshPoolMessage) {
        match message {
            SshPoolMessage::Connect {
                host,
                username,
                auth,
                reply_to,
            } => {
                let result = self.pool.connect(&host, username.as_deref(), auth).await;

                let _ = reply_to.send(result);
            }
            SshPoolMessage::Disconnect {
                host,
                username,
                reply_to,
            } => {
                let result = self.pool.disconnect(&host, &username).await;
                let _ = reply_to.send(result);
            }
            SshPoolMessage::ListConnections { reply_to } => {
                // Get the keys from the pool's connections
                let connections = self.pool.connections.keys().cloned().collect();
                let _ = reply_to.send(connections);
            }
            SshPoolMessage::Len { reply_to } => {
                let len = self.pool.connections.len();
                let _ = reply_to.send(len);
            }
            SshPoolMessage::Exec {
                host,
                username,
                interpreter,
                command,
                channel,
                output_stream,
                reply_to,
                result_tx,
            } => {
                let username = username.unwrap_or("root".to_string());
                let session = self
                    .pool
                    .connect(&host, Some(username.as_str()), None)
                    .await;

                if let Err(e) = session {
                    if let Err(e) = reply_to.send(Err(e)) {
                        log::error!("Failed to send error to reply_to: {e:?}");
                    }
                    return;
                }

                let session = session.unwrap();

                let (cancel_tx, cancel_rx) = oneshot::channel();

                self.channels.insert(
                    channel.clone(),
                    ChannelMeta {
                        host,
                        username,
                        cancel_tx,
                        result_tx,
                        pty_input_tx: None,
                    },
                );

                log::debug!("Executing command on channel {channel}: {command}");
                let result = session
                    .exec(
                        self.handle(),
                        channel,
                        output_stream,
                        cancel_rx,
                        interpreter.as_str(),
                        command.as_str(),
                    )
                    .await;
                if let Err(e) = reply_to.send(result) {
                    log::error!("Failed to send result to reply_to: {e:?}");
                }
            }
            SshPoolMessage::ExecFinished { channel, reply_to } => {
                log::debug!("ExecFinished for channel: {channel}");

                if let Some(meta) = self.channels.remove(&channel) {
                    let _ = meta.result_tx.send(());
                }

                let _ = reply_to.send(Ok(()));
            }
            SshPoolMessage::ExecCancel { channel } => {
                log::debug!("ExecCancel for channel: {channel}");

                if let Some(meta) = self.channels.remove(&channel) {
                    let _ = meta.cancel_tx.send(());
                }
            }
            SshPoolMessage::OpenPty {
                host,
                username,
                channel,
                output_stream,
                reply_to,
                width,
                height,
            } => {
                let username = username.unwrap_or("root".to_string());
                let session = self
                    .pool
                    .connect(&host, Some(username.as_str()), None)
                    .await;

                if let Err(e) = session {
                    if let Err(e) = reply_to.send(Err(e)) {
                        log::error!("Failed to send error to reply_to: {e:?}");
                    }
                    return;
                }

                let session = session.unwrap();

                // Create a channel to send input to the pty
                let (input_tx, input_rx) = mpsc::channel::<Bytes>(100);
                let (resize_tx, resize_rx) = mpsc::channel::<(u16, u16)>(100);

                // Store the input sender in the channels map
                let (cancel_tx, cancel_rx) = oneshot::channel();
                let (result_tx, _) = oneshot::channel();

                self.channels.insert(
                    channel.clone(),
                    ChannelMeta {
                        host,
                        username,
                        cancel_tx,
                        result_tx,
                        pty_input_tx: Some(input_tx.clone()),
                    },
                );

                log::debug!("Opening PTY for {channel}");
                if let Err(e) = session
                    .open_pty(
                        channel,
                        width,
                        height,
                        resize_rx,
                        input_rx,
                        output_stream,
                        cancel_rx,
                    )
                    .await
                {
                    log::error!("Failed to open PTY: {e:?}");
                    let _ = reply_to.send(Err(e));
                } else {
                    let _ = reply_to.send(Ok((input_tx, resize_tx)));
                }
            }
            SshPoolMessage::PtyWrite {
                channel,
                input,
                reply_to,
            } => {
                if let Some(meta) = self.channels.get_mut(&channel) {
                    if let Some(pty_input_tx) = meta.pty_input_tx.as_mut() {
                        let _ = pty_input_tx.send(input.clone()).await;
                        let _ = reply_to.send(Ok(()));
                    } else {
                        let _ = reply_to.send(Err(eyre::eyre!(
                            "No pty_input_tx found for channel: {}",
                            channel
                        )));
                    }
                } else {
                    let _ = reply_to.send(Err(eyre::eyre!(
                        "No channel meta found for channel: {}",
                        channel
                    )));
                }
            }
            SshPoolMessage::ClosePty { channel } => {
                log::debug!("Closing PTY for {channel}");
                if let Some(meta) = self.channels.remove(&channel) {
                    let _ = meta.cancel_tx.send(());
                }
            }
        }
    }
}
