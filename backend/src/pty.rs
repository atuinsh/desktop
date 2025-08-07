use std::{
    collections::HashMap,
    io::Write,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use bytes::Bytes;
use eyre::{eyre, Result};
use portable_pty::{CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::runtime::pty_store::PtyLike;

#[derive(Clone, Deserialize, Serialize, Debug, TS)]
#[ts(export)]
pub struct PtyMetadata {
    pub pid: Uuid,
    pub runbook: Uuid,
    pub block: String,
    pub created_at: u64,
}

pub struct Pty {
    tx: tokio::sync::mpsc::Sender<Bytes>,

    pub metadata: PtyMetadata,
    pub master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub reader: Arc<Mutex<Box<dyn std::io::Read + Send>>>,
    pub child: Arc<Mutex<Box<dyn portable_pty::Child + Send>>>,
}

#[async_trait]
impl PtyLike for Pty {
    fn metadata(&self) -> PtyMetadata {
        self.metadata.clone()
    }

    async fn kill_child(&self) -> Result<()> {
        self.kill_child().await
    }

    async fn send_bytes(&self, bytes: Bytes) -> Result<()> {
        self.send_bytes(bytes).await
    }

    async fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.resize(rows, cols).await
    }
}

impl Pty {
    pub async fn open(
        rows: u16,
        cols: u16,
        cwd: Option<String>,
        env: HashMap<String, String>,
        metadata: PtyMetadata,
        shell: Option<String>,
    ) -> Result<Self> {
        let sys = portable_pty::native_pty_system();

        let pair = sys
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| eyre!("Failed to open pty: {}", e))?;

        let mut cmd = match shell {
            Some(shell_path) if !shell_path.is_empty() => {
                let mut cmd = CommandBuilder::new(shell_path);
                cmd.arg("-i"); // Interactive mode
                cmd
            }
            _ => CommandBuilder::new_default_prog(),
        };

        // Flags to our shell integration that this is running within the desktop app
        cmd.env("ATUIN_DESKTOP_PTY", "true");
        cmd.env("TERM", "xterm-256color");

        if let Some(cwd) = cwd {
            cmd.cwd(cwd);
        }

        for (key, value) in env {
            cmd.env(key, value);
        }

        let child = match pair.slave.spawn_command(cmd) {
            Ok(child) => child,
            Err(e) => return Err(eyre!("Failed to spawn shell process: {}", e)),
        };
        drop(pair.slave);

        // Handle input -> write to master writer
        let (master_tx, mut master_rx) = tokio::sync::mpsc::channel::<Bytes>(32);

        let mut writer = pair.master.take_writer().unwrap();
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| e.to_string())
            .expect("Failed to clone reader");

        tokio::spawn(async move {
            while let Some(bytes) = master_rx.recv().await {
                writer.write_all(&bytes).unwrap();
                writer.flush().unwrap();
            }

            // When the channel has been closed, we won't be getting any more input. Close the
            // writer and the master.
            // This will also close the writer, which sends EOF to the underlying shell. Ensuring
            // that is also closed.
            drop(writer);
        });

        Ok(Pty {
            metadata,
            tx: master_tx,
            master: Arc::new(Mutex::new(pair.master)),
            reader: Arc::new(Mutex::new(reader)),
            child: Arc::new(Mutex::new(child)),
        })
    }

    pub async fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        let master = self
            .master
            .lock()
            .map_err(|e| eyre!("Failed to lock pty master: {e}"))?;

        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| eyre!("Failed to resize terminal: {e}"))?;

        Ok(())
    }

    pub async fn send_bytes(&self, bytes: Bytes) -> Result<()> {
        self.tx
            .send(bytes)
            .await
            .map_err(|e| eyre!("Failed to write to master tx: {}", e))
    }

    #[allow(dead_code)]
    pub async fn send_string(&self, cmd: &str) -> Result<()> {
        let bytes: Vec<u8> = cmd.bytes().collect();
        let bytes = Bytes::from(bytes);

        self.send_bytes(bytes).await
    }

    #[allow(dead_code)]
    pub async fn send_single_string(&self, cmd: &str) -> Result<()> {
        let mut bytes: Vec<u8> = cmd.bytes().collect();
        bytes.push(0x04);

        let bytes = Bytes::from(bytes);

        self.send_bytes(bytes).await
    }

    pub async fn kill_child(&self) -> Result<()> {
        let mut child = self
            .child
            .lock()
            .map_err(|e| eyre!("Failed to lock pty child: {e}"))?;

        child
            .kill()
            .map_err(|e| eyre!("Failed to kill child: {e}"))?;

        Ok(())
    }
}
