//! Cron scheduler for running prompts on a schedule.
//!
//! Jobs are stored in `~/.agentbridge/cron_jobs.json`. A background task
//! checks every 60 seconds and fires matching jobs by injecting synthetic
//! messages into the engine's message handler.

use anyhow::Result;
use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::core::message::IncomingMessage;
use crate::core::platform::{MessageHandler, PlatformCapabilities, ReplyCtx};
use crate::platforms::discord::types::DiscordReplyCtx;
use crate::platforms::telegram::types::TelegramReplyCtx;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub cron_expr: String,
    pub prompt: String,
    pub description: String,
    pub session_key: String,
    #[serde(default)]
    pub agent_name: String,
    pub enabled: bool,
    #[serde(default)]
    pub last_fired: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_error: Option<String>,
}

impl CronJob {
    #[allow(dead_code)]
    pub fn effective_agent_name(&self) -> &str {
        if self.agent_name.is_empty() {
            "claude"
        } else {
            &self.agent_name
        }
    }
}

pub struct CronScheduler {
    jobs: Arc<Mutex<Vec<CronJob>>>,
    data_dir: PathBuf,
}

impl CronScheduler {
    pub fn new(data_dir: &Path) -> Self {
        let jobs_path = data_dir.join("cron_jobs.json");
        let jobs = if jobs_path.exists() {
            fs::read_to_string(&jobs_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        Self {
            jobs: Arc::new(Mutex::new(jobs)),
            data_dir: data_dir.to_path_buf(),
        }
    }

    async fn persist(&self) {
        let jobs = self.jobs.lock().await;
        let path = self.data_dir.join("cron_jobs.json");
        if let Ok(data) = serde_json::to_string_pretty(&*jobs) {
            let _ = fs::write(path, data);
        }
    }

    pub async fn add_job(
        &self,
        cron_expr: String,
        prompt: String,
        description: String,
        session_key: String,
        agent_name: String,
    ) -> Result<CronJob> {
        Schedule::from_str(&cron_expr)
            .map_err(|e| anyhow::anyhow!("Invalid cron expression: {}", e))?;

        let job = CronJob {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            cron_expr,
            prompt,
            description,
            session_key,
            agent_name,
            enabled: true,
            last_fired: None,
            last_error: None,
        };

        let mut jobs = self.jobs.lock().await;
        jobs.push(job.clone());
        drop(jobs);
        self.persist().await;
        Ok(job)
    }

    pub async fn list_jobs(&self) -> Vec<CronJob> {
        self.jobs.lock().await.clone()
    }

    pub async fn delete_job(&self, id: &str) -> bool {
        let mut jobs = self.jobs.lock().await;
        let before = jobs.len();
        jobs.retain(|j| j.id != id);
        let removed = jobs.len() < before;
        drop(jobs);
        if removed {
            self.persist().await;
        }
        removed
    }

    /// Mark a job as fired (update last_fired and optionally last_error).
    async fn mark_run(&self, id: &str, error: Option<String>) {
        let mut jobs = self.jobs.lock().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            job.last_fired = Some(Utc::now());
            job.last_error = error;
        }
        drop(jobs);
        self.persist().await;
    }

    /// Get jobs that should fire now (enabled + cron matches + not fired in last 60s).
    async fn due_jobs(&self) -> Vec<CronJob> {
        let jobs = self.jobs.lock().await;
        let now = Utc::now();
        let mut due = Vec::new();

        for job in jobs.iter() {
            if !job.enabled {
                continue;
            }

            let schedule = match Schedule::from_str(&job.cron_expr) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Check if any scheduled time falls within the last 60 seconds
            let check_from = job
                .last_fired
                .unwrap_or(now - chrono::Duration::seconds(61));

            // Find next occurrence after last fire
            if let Some(next) = schedule.after(&check_from).next() {
                if next <= now {
                    due.push(job.clone());
                }
            }
        }

        due
    }

    /// Start the background scheduler loop. Checks every 60 seconds.
    ///
    /// `handler` is the engine's message handler closure.
    /// `platforms` maps platform names to their Arc (used to dispatch cron messages).
    pub fn start(
        self: &Arc<Self>,
        handler: MessageHandler,
        platforms: HashMap<String, Arc<dyn PlatformCapabilities>>,
        active_agents: Arc<tokio::sync::Mutex<std::collections::HashMap<String, String>>>,
    ) {
        let scheduler = Arc::clone(self);
        let handler = handler;
        let platforms = Arc::new(platforms);

        tokio::spawn(async move {
            tracing::info!("cron: scheduler started");

            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;

                let due = scheduler.due_jobs().await;
                for job in due {
                    let job_id = job.id.clone();
                    let job_desc = if job.description.is_empty() {
                        job.prompt.chars().take(40).collect::<String>()
                    } else {
                        job.description.clone()
                    };

                    tracing::info!(
                        job_id = %job_id,
                        description = %job_desc,
                        "cron: firing job"
                    );

                    // Parse platform name from session_key (e.g., "discord:channelId")
                    let platform_name = job
                        .session_key
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .to_string();

                    let platform = match platforms.get(&platform_name) {
                        Some(p) => Arc::clone(p),
                        None => {
                            let err = format!("platform '{}' not found", platform_name);
                            tracing::error!(job_id = %job_id, error = %err, "cron: skip job");
                            scheduler.mark_run(&job_id, Some(err)).await;
                            continue;
                        }
                    };

                    // Build platform-specific ReplyCtx from session_key
                    // session_key format: "platform:channel_id"
                    let channel_id = job
                        .session_key.split_once(':').map(|x| x.1)
                        .unwrap_or("")
                        .to_string();

                    let reply_ctx: Box<dyn ReplyCtx> = match platform_name.as_str() {
                        "discord" => Box::new(DiscordReplyCtx {
                            channel_id: channel_id.clone(),
                            message_id: None,
                            thread_id: Some(channel_id.clone()),
                        }),
                        "telegram" => {
                            let chat_id = channel_id.parse::<i64>().unwrap_or(0);
                            Box::new(TelegramReplyCtx {
                                chat_id,
                                thread_id: None,
                                message_id: None,
                            })
                        }
                        _ => Box::new(CronReplyCtx {
                            session_key: job.session_key.clone(),
                        }),
                    };

                    let msg = IncomingMessage {
                        id: format!("cron-{}-{}", job_id, Utc::now().timestamp()),
                        from: "cron".to_string(),
                        from_name: Some("cron".to_string()),
                        text: job.prompt.clone(),
                        images: vec![],
                        files: vec![],
                        voice: None,
                        is_group: false,
                        channel_id: Some(channel_id),
                        channel_name: None,
                        reply_ctx,
                    };

                    // Set the active agent to the job's pinned agent so the
                    // engine dispatch routes to the correct backend.
                    {
                        let agent = job.effective_agent_name().to_string();
                        let mut map = active_agents.lock().await;
                        map.insert(job.session_key.clone(), agent);
                    }

                    // Send notification
                    let _ = platform
                        .reply(msg.reply_ctx.as_ref(), &format!("⏰ {}", job_desc))
                        .await;

                    // Dispatch through the engine's message handler
                    handler(platform, msg);

                    scheduler.mark_run(&job_id, None).await;
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// CronReplyCtx — minimal ReplyCtx for cron-triggered messages
// ---------------------------------------------------------------------------

/// Reply context for cron jobs. Uses the session_key to route replies
/// to the correct channel/thread.
#[derive(Debug, Clone)]
pub struct CronReplyCtx {
    session_key: String,
}

impl CronReplyCtx {
    pub fn new(session_key: String) -> Self {
        Self { session_key }
    }
}

impl ReplyCtx for CronReplyCtx {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn session_key_hint(&self) -> String {
        self.session_key.clone()
    }

    fn clone_box(&self) -> Box<dyn ReplyCtx> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn cron_job_empty_agent_name_falls_back_claude() {
        let job = CronJob {
            id: "1".into(),
            cron_expr: "0 0 * * * *".into(),
            prompt: "p".into(),
            description: "d".into(),
            session_key: "k".into(),
            agent_name: String::new(),
            enabled: true,
            last_fired: None,
            last_error: None,
        };
        assert_eq!(job.effective_agent_name(), "claude");
    }

    #[test]
    fn cron_job_explicit_agent_name() {
        let job = CronJob {
            id: "1".into(),
            cron_expr: "0 0 * * * *".into(),
            prompt: "p".into(),
            description: "d".into(),
            session_key: "k".into(),
            agent_name: "kiro".into(),
            enabled: true,
            last_fired: None,
            last_error: None,
        };
        assert_eq!(job.effective_agent_name(), "kiro");
    }

    #[tokio::test]
    async fn add_job_stores_agent_name() {
        let tmp = TempDir::new().unwrap();
        let scheduler = CronScheduler::new(tmp.path());
        let job = scheduler
            .add_job(
                "0 0 * * * *".into(),
                "prompt".into(),
                "desc".into(),
                "telegram:123".into(),
                "kiro".into(),
            )
            .await
            .unwrap();
        assert_eq!(job.agent_name, "kiro");
    }

    #[tokio::test]
    async fn old_cron_file_without_agent_name_loads_as_empty() {
        let tmp = TempDir::new().unwrap();
        let old_format = serde_json::json!([{
            "id": "abc",
            "cron_expr": "0 0 * * * *",
            "prompt": "p",
            "description": "d",
            "session_key": "k",
            "enabled": true
        }]);
        std::fs::write(
            tmp.path().join("cron_jobs.json"),
            serde_json::to_string(&old_format).unwrap(),
        )
        .unwrap();

        let scheduler = CronScheduler::new(tmp.path());
        let jobs = scheduler.list_jobs().await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].agent_name, "");
        assert_eq!(jobs[0].effective_agent_name(), "claude");
    }

    #[tokio::test]
    async fn new_cron_roundtrips_through_disk_with_agent_name() {
        let tmp = TempDir::new().unwrap();
        {
            let scheduler = CronScheduler::new(tmp.path());
            scheduler
                .add_job(
                    "0 0 * * * *".into(),
                    "p".into(),
                    "d".into(),
                    "telegram:1".into(),
                    "kiro".into(),
                )
                .await
                .unwrap();
        }
        let scheduler2 = CronScheduler::new(tmp.path());
        let jobs = scheduler2.list_jobs().await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].agent_name, "kiro");
        assert_eq!(jobs[0].effective_agent_name(), "kiro");
    }
}
