//! `fan_out_with_group_join` — orchestration lego.
//!
//! The pattern a multi-agent host uses so a fan-out does not
//! interrupt the main agent once per completion:
//!
//! 1. register the group of agent ids **before** launching them,
//! 2. poll each detached agent as it finishes,
//! 3. route every completion through [`GroupJoin`],
//! 4. emit **one** consolidated notification — on full
//!    completion, or on a timeout so one straggler cannot stall
//!    the batch forever.
//!
//! The workers here are simulated (this example is about the
//! coordination seam, not a model call), but the sequence is
//! exactly the one a host writes around
//! `synthia_harness::AgentHandle::spawn_detached` +
//! `DetachedAgent::join`/`try_event`.
//!
//! ```bash
//! cargo run --example fan_out_with_group_join -p synthia-tool-task
//! ```

use chrono::{DateTime, TimeDelta, Utc};
use synthia_tool_task::{Delivery, GroupJoin, GroupOutcome};

/// One simulated background agent.
struct Worker {
    id: &'static str,
    /// When this worker reports, relative to the start.
    finishes_after: TimeDelta,
}

fn main() {
    println!("== synthia: fan-out + group join ==\n");

    let start = DateTime::<Utc>::from_timestamp(1_700_000_000, 0)
        .expect("valid timestamp");

    let workers = [
        Worker {
            id: "researcher",
            finishes_after: TimeDelta::seconds(2),
        },
        Worker {
            id: "critic",
            finishes_after: TimeDelta::seconds(5),
        },
        // Misses the 30 s window only if it is slow — here it
        // reports last, inside the straggler window.
        Worker {
            id: "synthesizer",
            finishes_after: TimeDelta::seconds(9),
        },
    ];

    // 1. Register before launching: a completion reported for an
    //    id the coordinator does not know can only be delivered
    //    individually.
    let mut join = GroupJoin::new();
    join.register_group(
        "review-panel",
        workers.iter().map(|w| w.id.to_string()),
    );
    println!("group 'review-panel': {} members", workers.len());

    // 2/3. Drive completions in time order and route them.
    let mut notifications: Vec<Delivery> = Vec::new();
    let mut ordered: Vec<&Worker> = workers.iter().collect();
    ordered.sort_by_key(|w| w.finishes_after);

    for worker in ordered {
        let now = start + worker.finishes_after;
        match join.on_complete(worker.id, now) {
            GroupOutcome::Deliver(delivery) => notifications.push(delivery),
            GroupOutcome::Held => {
                println!("  {:<12} finished — held for the batch", worker.id);
            }
            GroupOutcome::Pass => {
                println!("  {:<12} finished — notified individually", worker.id)
            }
        }
    }

    // A host arms a timer from `next_deadline()` instead of
    // assuming every member finishes in time. Show the
    // timeout path on a second group with one straggler.
    let mut slow = GroupJoin::with_timeouts(
        TimeDelta::seconds(30),
        TimeDelta::seconds(15),
    );
    slow.register_group(
        "slow-batch",
        ["quick".to_string(), "glacial".to_string()],
    );
    slow.on_complete("quick", start);
    println!(
        "\nslow-batch window expires at +{}s",
        (slow.next_deadline().expect("armed") - start).num_seconds()
    );
    // The window elapses: deliver what is finished rather than
    // block the notification on the straggler.
    let expired_at = start + TimeDelta::seconds(30);
    for delivery in slow.on_tick(expired_at) {
        println!(
            "  window expired — partial={} members={:?}",
            delivery.partial, delivery.agent_ids
        );
        notifications.push(delivery);
    }
    println!(
        "  straggler still grouped: {} (re-batched on a {}s window)",
        slow.is_grouped("glacial"),
        (slow.next_deadline().expect("re-armed") - expired_at).num_seconds()
    );
    if let GroupOutcome::Deliver(delivery) =
        slow.on_complete("glacial", expired_at)
    {
        notifications.push(delivery);
    }

    // 4. Report.
    println!("\nnotifications: {}", notifications.len());
    for d in &notifications {
        println!(
            "  group={:<11} partial={:<5} members={:?}",
            d.group_id, d.partial, d.agent_ids
        );
    }

    // The invariant that makes batching safe: no member's result
    // is ever reported twice.
    let mut seen = std::collections::HashSet::new();
    for d in &notifications {
        for id in &d.agent_ids {
            assert!(seen.insert(id.clone()), "{id} was delivered twice");
        }
    }
    assert!(join.is_empty() && slow.is_empty(), "groups must retire");
    println!("\nno member delivered twice; both groups retired");
    println!("\n== done ==");
}
