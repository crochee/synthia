//! `MemoryBackgroundTask` struct and its event handler.

use std::{sync::Arc, time::Duration};

use anyhow::Result;
use synthia_memory::types::{
    ColdEntry,
    CompactionReport,
    EpisodicSkill,
    MemoryEvent,
    MemoryStore,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

const DEFAULT_COMPACTION_INTERVAL: Duration = Duration::from_secs(300);

pub struct MemoryBackgroundTask<S: MemoryStore> {
    pub(crate) store: Arc<S>,
    pub(crate) event_receiver: mpsc::Receiver<MemoryEvent>,
    pub(crate) shutdown_signal: CancellationToken,
    pub(crate) compaction_interval: Duration,
}

impl<S: MemoryStore + 'static> MemoryBackgroundTask<S> {
    pub fn new(
        store: Arc<S>,
        event_receiver: mpsc::Receiver<MemoryEvent>,
        shutdown_signal: CancellationToken,
    ) -> Self {
        Self {
            store,
            event_receiver,
            shutdown_signal,
            compaction_interval: DEFAULT_COMPACTION_INTERVAL,
        }
    }

    pub fn with_compaction_interval(mut self, interval: Duration) -> Self {
        self.compaction_interval = interval;
        self
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut compaction_timer =
            tokio::time::interval(self.compaction_interval);
        compaction_timer
            .set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                event = self.event_receiver.recv() => {
                    match event {
                        Some(evt) => {
                            self.handle_event(evt).await;
                        }
                        None => {
                            info!("Memory event channel closed, shutting down");
                            break;
                        }
                    }
                }

                _ = compaction_timer.tick() => {
                    if let Err(e) = self.periodic_compaction().await {
                        error!(error = %e, "Periodic compaction failed");
                    }
                }

                _ = self.shutdown_signal.cancelled() => {
                    info!("Memory background task shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_event(&self, event: MemoryEvent) {
        let store = Arc::clone(&self.store);

        let event_result = tokio::spawn(async move {
            match event {
                MemoryEvent::SessionEnd {
                    session_id,
                    summary,
                    tools_used,
                    outcome,
                } => {
                    let entry = ColdEntry {
                        id: session_id.clone(),
                        content: summary.clone(),
                        metadata: serde_json::json!({
                            "tools_used": tools_used,
                            "outcome": outcome,
                        }),
                        created_at: chrono::Utc::now(),
                        session_id: Some(session_id),
                        summary: Some(summary),
                        outcome: Some(outcome),
                        timestamp: None,
                        tools_used: Some(tools_used),
                        importance_score: 0.5,
                        access_count: 0,
                    };
                    store.write_cold(entry).await
                }
                MemoryEvent::ToolExecuted {
                    session_id: _,
                    tool_name,
                    success,
                } => {
                    if !success {
                        return Ok(());
                    }
                    let skill = EpisodicSkill {
                        task_hint: tool_name.clone(),
                        skill_content: serde_json::json!({
                            "tool": tool_name,
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                        })
                        .to_string(),
                        success_rate: 1.0,
                        used_at: chrono::Utc::now(),
                    };
                    store.save_episodic(skill).await
                }
                MemoryEvent::MemoryFlush { key, content } => {
                    store.write_hot(&key, &content).await
                }
            }
        });

        match event_result.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                error!(error = %e, "Failed to handle memory event");
            }
            Err(join_err) => {
                error!(error = %join_err, "Event handler task failed");
            }
        }
    }

    async fn periodic_compaction(&self) -> Result<()> {
        info!("Starting periodic memory compaction check");
        let report = self
            .store
            .compact_context("default")
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        if report.tokens_before > 0 {
            info!(
                tokens_before = report.tokens_before,
                tokens_after = report.tokens_after,
                reduction = %format!(
                    "{:.1}%",
                    (1.0 - report.tokens_after as f64 / report.tokens_before as f64) * 100.0
                ),
                "Memory compaction completed"
            );
        }
        Ok(())
    }

    pub async fn compact_specific_session(
        &self,
        session_id: &str,
    ) -> Result<CompactionReport> {
        self.store
            .compact_context(session_id)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }
}
