// An actor for managing SSH connections

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::time::interval;

use crate::pty::PtyMetadata;
use crate::ssh::pool::Pool;
use crate::ssh::session::{Authentication, Session};
use eyre::Result;
use std::sync::Arc;

use crate::pty::PtyLike;

#[derive(thiserror::Error, Debug)]
enum SshPoolConnectionError {
    #[error("SSH connection cancelled")]
    Cancelled,
    #[error("{0}")]
    ConnectionError(#[from] eyre::Report),
}

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
    HealthCheck {
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

        tokio::spawn(async move { actor.run().await });

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

    pool: Arc<RwLock<Pool>>,
}

impl SshPool {
    pub fn new(
        sender: mpsc::Sender<SshPoolMessage>,
        receiver: mpsc::Receiver<SshPoolMessage>,
    ) -> Self {
        Self {
            sender,
            receiver,
            pool: Arc::new(RwLock::new(Pool::new())),
            channels: HashMap::new(),
        }
    }

    pub fn handle(&self) -> SshPoolHandle {
        SshPoolHandle::new_handle(self.sender.clone())
    }

    async fn run(&mut self) {
        // Start the health check task
        let health_check_handle = self.start_health_check_task();

        while let Some(msg) = self.receiver.recv().await {
            self.handle_message(msg).await;

            log::debug!("SshPool Message handled");
        }

        // Clean up health check task when the pool shuts down
        health_check_handle.abort();
    }

    /// Start the background health checking task
    fn start_health_check_task(&self) -> tokio::task::JoinHandle<()> {
        let sender = self.sender.clone();

        tokio::spawn(async move {
            // Health check every 2 minutes to avoid excessive network traffic
            let mut interval = interval(Duration::from_secs(120));

            loop {
                interval.tick().await;

                // Send health check message to the main actor
                let (reply_tx, recv) = oneshot::channel();
                let msg = SshPoolMessage::HealthCheck { reply_to: reply_tx };

                if sender.send(msg).await.is_err() {
                    log::debug!("SSH pool shut down, stopping health check task");
                    break;
                }

                let resp = recv.await;
                match resp {
                    Ok(Ok(())) => {
                        log::debug!("Scheduled SSH health check successful");
                    }
                    Ok(Err(err)) => {
                        log::error!("Scheduled SSH health check failed: {err}");
                    }
                    Err(_) => {
                        log::error!("Scheduled SSH health check response failure");
                    }
                }
            }
        })
    }

    async fn handle_message(&mut self, message: SshPoolMessage) {
        match message {
            SshPoolMessage::Connect {
                host,
                username,
                auth,
                reply_to,
            } => {
                let result = self
                    .pool
                    .write()
                    .await
                    .connect(&host, username.as_deref(), auth, None)
                    .await;

                let _ = reply_to.send(result);
            }
            SshPoolMessage::Disconnect {
                host,
                username,
                reply_to,
            } => {
                let result = self.pool.write().await.disconnect(&host, &username).await;
                let _ = reply_to.send(result);
            }
            SshPoolMessage::ListConnections { reply_to } => {
                // Get the keys from the pool's connections
                let connections = self.pool.read().await.connections.keys().cloned().collect();
                let _ = reply_to.send(connections);
            }
            SshPoolMessage::Len { reply_to } => {
                let len = self.pool.read().await.connections.len();
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
                let (cancel_tx, mut cancel_rx) = oneshot::channel();

                let username = username.unwrap_or_else(whoami::username);
                self.channels.insert(
                    channel.clone(),
                    ChannelMeta {
                        host: host.clone(),
                        username: username.clone(),
                        cancel_tx,
                        result_tx,
                        pty_input_tx: None,
                    },
                );

                let (connect_cancel_tx, connect_cancel_rx) = oneshot::channel();

                let pool = self.pool.clone();
                let handle = self.handle();
                // Run the SSH connection in a task to avoid blocking the actor
                tokio::spawn(async move {
                    log::trace!("Connecting to SSH host {host} with username {username}");
                    let mut pool_guard = pool.write().await;
                    let session: Result<Arc<Session>, SshPoolConnectionError> = tokio::select! {
                        result = pool_guard.connect(&host, Some(username.as_str()), None, Some(connect_cancel_rx)) => {
                            log::trace!("SSH connection to {host} with username {username} successful");
                            result.map_err(SshPoolConnectionError::from)
                        }
                        _ = &mut cancel_rx => {
                            log::trace!("SSH connection to {host} with username {username} cancelled");
                            let _ = connect_cancel_tx.send(());
                            let _ = pool_guard.disconnect(&host, &username).await;
                            Err(SshPoolConnectionError::Cancelled)
                        }
                    };
                    drop(pool_guard);

                    let session = match session {
                        Ok(session) => session,
                        Err(e) => {
                            match e {
                                SshPoolConnectionError::Cancelled => {
                                    log::debug!("SSH connection to {host} with username {username} cancelled");
                                    let _ = reply_to
                                        .send(Err(SshPoolConnectionError::Cancelled.into()));
                                    return;
                                }
                                SshPoolConnectionError::ConnectionError(e) => {
                                    log::error!("Failed to connect to SSH host {host}: {e}");
                                    if let Err(e) = reply_to.send(Err(e)) {
                                        log::error!("Failed to send error to reply_to: {e:?}");
                                    }
                                    return;
                                }
                            }
                        }
                    };

                    log::trace!("Executing command on channel {channel}: {command}");
                    let result = session
                        .exec(
                            handle,
                            channel.clone(),
                            output_stream,
                            cancel_rx,
                            interpreter.as_str(),
                            command.as_str(),
                        )
                        .await;

                    // Handle exec failures - if the session failed, remove it from the pool
                    if let Err(ref e) = result {
                        log::error!("SSH exec failed for {host}: {e}");
                        let key = format!("{username}@{host}");

                        // TODO: use a proper error enum
                        let error_str = e.to_string().to_lowercase();
                        if error_str.contains("timeout")
                            || error_str.contains("connection")
                            || error_str.contains("broken pipe")
                        {
                            log::debug!("Removing SSH connection due to connection error: {key}");
                            pool.write().await.connections.remove(&key);
                        } else if let Some(session) = pool.read().await.connections.get(&key) {
                            if !session.send_keepalive().await {
                                log::debug!(
                                    "Removing dead SSH connection after exec failure: {key}"
                                );
                                pool.write().await.connections.remove(&key);
                            }
                        }
                    }

                    if let Err(e) = reply_to.send(result) {
                        log::error!("Failed to send result to reply_to: {e:?}");
                    }
                });
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
                    log::trace!("Sending cancel to channel {channel}");
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
                let username = username.unwrap_or_else(whoami::username);
                let session = self
                    .pool
                    .write()
                    .await
                    .connect(&host, Some(username.as_str()), None, None)
                    .await;

                let session = match session {
                    Ok(session) => session,
                    Err(e) => {
                        log::error!("Failed to connect to SSH host {host}: {e}");
                        if let Err(e) = reply_to.send(Err(e)) {
                            log::error!("Failed to send error to reply_to: {e:?}");
                        }
                        return;
                    }
                };

                // Create a channel to send input to the pty
                let (input_tx, input_rx) = mpsc::channel::<Bytes>(100);
                let (resize_tx, resize_rx) = mpsc::channel::<(u16, u16)>(100);

                // Store the input sender in the channels map
                let (cancel_tx, cancel_rx) = oneshot::channel();
                let (result_tx, _) = oneshot::channel();

                self.channels.insert(
                    channel.clone(),
                    ChannelMeta {
                        host: host.clone(),
                        username: username.clone(),
                        cancel_tx,
                        result_tx,
                        pty_input_tx: Some(input_tx.clone()),
                    },
                );

                log::debug!("Opening PTY for {channel}");
                let pty_result = session
                    .open_pty(
                        channel.clone(),
                        width,
                        height,
                        resize_rx,
                        input_rx,
                        output_stream,
                        cancel_rx,
                    )
                    .await;

                match pty_result {
                    Err(e) => {
                        log::error!("Failed to open PTY: {e:?}");
                        // Check if connection is dead and remove it
                        let key = format!("{username}@{host}");

                        // TODO: use a proper error enum
                        let error_str = e.to_string().to_lowercase();
                        if error_str.contains("timeout")
                            || error_str.contains("connection")
                            || error_str.contains("broken pipe")
                        {
                            log::debug!(
                                "Removing SSH connection due to PTY connection error: {key}"
                            );
                            self.pool.write().await.connections.remove(&key);
                        } else if let Some(session) = self.pool.read().await.connections.get(&key) {
                            if !session.send_keepalive().await {
                                log::debug!(
                                    "Removing dead SSH connection after PTY failure: {key}"
                                );
                                self.pool.write().await.connections.remove(&key);
                            }
                        }

                        let _ = reply_to.send(Err(e));
                    }
                    Ok(_) => {
                        let _ = reply_to.send(Ok((input_tx, resize_tx)));
                    }
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
            SshPoolMessage::HealthCheck { reply_to } => {
                let connection_count = self.pool.read().await.connections.len();
                log::debug!(
                    "Running SSH connection health check with keepalives on {connection_count} connections"
                );
                let mut dead_connections = Vec::new();

                // Check all connections for liveness using actual keepalives
                for (key, session) in &self.pool.read().await.connections {
                    if !session.send_keepalive().await {
                        log::debug!("SSH keepalive failed for connection: {key}");
                        dead_connections.push(key.clone());
                    }
                }

                let dead_count = dead_connections.len();
                for key in &dead_connections {
                    self.pool.write().await.connections.remove(key);
                }
                if dead_count > 0 {
                    log::debug!(
                        "Health check removed {dead_count} dead connections, {} remaining",
                        self.pool.read().await.connections.len()
                    );
                } else if connection_count > 0 {
                    log::debug!(
                        "Health check completed, all {connection_count} connections responded to keepalive"
                    );
                }

                let _ = reply_to.send(Ok(()));
            }
        }
    }
}
