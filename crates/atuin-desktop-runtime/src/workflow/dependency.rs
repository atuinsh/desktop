use eyre::Result;
use serde::{Deserialize, Serialize};

use crate::{blocks::Block, exec_log::ExecLogHandle};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencySpec {
    pub parents: Vec<String>,
    pub within: i64,
    pub auto_run_parents: bool,
}

impl DependencySpec {
    pub async fn can_run(&self, block: &Block, exec_log: ExecLogHandle) -> Result<bool> {
        if self.parents.is_empty() {
            return Ok(true);
        }

        let parent_id = self.parents.first().unwrap();

        match self.within {
            -1 => {
                let parent_uuid = match uuid::Uuid::parse_str(parent_id) {
                    Ok(uuid) => uuid,
                    Err(_) => return Ok(false), // Invalid parent UUID
                };

                // Parent must have been run at least once
                let parent_last_run = exec_log.get_last_execution_time(parent_uuid).await?;
                println!("got run once ever, parent_last_run: {parent_last_run:?}");

                Ok(parent_last_run.is_some())
            }

            0 => {
                let parent_uuid = match uuid::Uuid::parse_str(parent_id) {
                    Ok(uuid) => uuid,
                    Err(_) => return Ok(false), // Invalid parent UUID
                };

                // We have when the parent last ran. Now check when this block last ran. So long as it was before the parent last ran, we're good.
                let parent_last_run = exec_log.get_last_execution_time(parent_uuid).await?;
                let block_last_run = exec_log.get_last_execution_time(block.id()).await?;

                match (parent_last_run, block_last_run) {
                    (Some(parent_last_run), Some(block_last_run)) => {
                        Ok(block_last_run < parent_last_run)
                    }

                    (Some(_), None) => Ok(true),

                    _ => Ok(false),
                }
            }

            num => {
                let parent_uuid = match uuid::Uuid::parse_str(parent_id) {
                    Ok(uuid) => uuid,
                    Err(_) => return Ok(false), // Invalid parent UUID
                };

                // Parent must have been run within the specified time window
                let parent_last_run = exec_log.get_last_execution_time(parent_uuid).await?;

                println!("got run within, parent_last_run: {parent_last_run:?}");

                if let Some(parent_last_run) = parent_last_run {
                    let now = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
                    let window = num * 1_000_000_000;
                    let time_since_parent_run = now - parent_last_run as i128;

                    return Ok(time_since_parent_run <= window as i128);
                }

                Ok(false)
            }
        }
    }
}
