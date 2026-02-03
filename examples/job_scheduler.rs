use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use async_trait::async_trait;
use synthia_job::{Job, TimeWheel, Trigger, every, parse_standard};
use tokio_util::sync::CancellationToken;
struct HelloJob {
    name: String,
    counter: std::sync::atomic::AtomicU32,
}

#[async_trait]
impl Job for HelloJob {
    fn description(&self) -> &str {
        "A simple hello world job"
    }

    fn key(&self) -> &str {
        &self.name
    }

    async fn execute(&self) {
        let count = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        println!(
            "[{}] Hello, world! (execution #{}) Time: {:?}",
            self.name,
            count + 1,
            std::time::SystemTime::now()
        );
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== synthia-job Scheduler Example ===\n");

    let wheel = Arc::new(TimeWheel::new());

    let job1 = Arc::new(HelloJob {
        name: "hello_job".to_string(),
        counter: std::sync::atomic::AtomicU32::new(0),
    });

    let trigger1 = every(Duration::from_secs(2));
    println!("Scheduling job1 with trigger: {}", trigger1.description());
    wheel
        .schedule_async(job1, Arc::new(trigger1))
        .await
        .context("Failed to schedule job1")?;

    println!("\nScheduler started!\n");

    let cancel_token = CancellationToken::new();
    let cancel_clone = cancel_token.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        println!("\nReceived Ctrl+C, shutting down...");
        cancel_clone.cancel();
    });

    let wheel_for_ops = Arc::clone(&wheel);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(8)).await;

        println!("\n===== [View all jobs] =====");
        for job in wheel_for_ops.jobs() {
            println!("  - {} (trigger: {})", job.job.key(), job.trigger_desc);
        }

        tokio::time::sleep(Duration::from_secs(6)).await;

        println!("\n===== [Dynamically add cron_job] =====");
        let job2 = Arc::new(HelloJob {
            name: "cron_job".to_string(),
            counter: std::sync::atomic::AtomicU32::new(0),
        });

        match parse_standard("*/6 * * * * *") {
            Ok(trigger) => {
                let trigger2: Arc<dyn Trigger> = Arc::from(trigger);
                match wheel_for_ops.schedule_async(job2, trigger2).await {
                    Ok(_) => println!("  cron_job added successfully!"),
                    Err(e) => println!("  Failed to add: {e:?}"),
                }
            }
            Err(e) => println!("  Failed to parse cron: {e:?}"),
        }

        tokio::time::sleep(Duration::from_secs(2)).await;

        println!("\n===== [View all jobs after add] =====");
        for job in wheel_for_ops.jobs() {
            println!("  - {} (trigger: {})", job.job.key(), job.trigger_desc);
        }

        tokio::time::sleep(Duration::from_secs(6)).await;

        println!("\n===== [Remove hello_job] =====");
        match wheel_for_ops.remove("hello_job").await {
            Ok(_) => println!("  hello_job removed successfully!"),
            Err(e) => println!("  Failed to remove: {e:?}"),
        }

        tokio::time::sleep(Duration::from_secs(2)).await;

        println!("\n===== [View all jobs after remove] =====");
        for job in wheel_for_ops.jobs() {
            println!("  - {} (trigger: {})", job.job.key(), job.trigger_desc);
        }
    });

    wheel
        .run(cancel_token)
        .await
        .context("Failed to run time wheel")?;

    println!("\nScheduler stopped.");
    Ok(())
}
