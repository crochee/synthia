//! Steering mechanism for mid-loop agent control.
//!
//! Provides a channel for injecting steering messages into the ReAct loop
//! with priority-based ordering and overflow handling.

use std::{cmp::Ordering, sync::Mutex, time::Instant};

/// A steering message that can be injected into the agent loop.
#[derive(Clone, Debug)]
pub struct SteeringMessage {
    pub content: String,
    pub priority: u8,
    pub timestamp: Instant,
}

impl SteeringMessage {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            priority: 5,
            timestamp: Instant::now(),
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

impl Default for SteeringMessage {
    fn default() -> Self {
        Self::new("")
    }
}

/// Internal ordering wrapper: higher priority first, then earlier timestamp first.
#[derive(Clone, Debug)]
struct PriorityMsg(SteeringMessage);

impl PartialEq for PriorityMsg {
    fn eq(&self, other: &Self) -> bool {
        self.0.priority == other.0.priority
            && self.0.timestamp == other.0.timestamp
    }
}

impl Eq for PriorityMsg {}

impl PartialOrd for PriorityMsg {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityMsg {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first
        self.0
            .priority
            .cmp(&other.0.priority)
            // Tie-break: earlier timestamp first (lower Instant = higher precedence)
            .then_with(|| other.0.timestamp.cmp(&self.0.timestamp))
    }
}

/// Trait for a steering channel.
#[async_trait::async_trait]
pub trait SteeringChannel: Send + Sync {
    async fn send(&self, msg: SteeringMessage);
    fn try_recv(&self) -> Option<SteeringMessage>;
    fn is_empty(&self) -> bool;
    /// Drains all pending messages, returning them in priority order.
    /// Default implementation repeatedly calls `try_recv()`.
    fn drain(&self) -> Vec<SteeringMessage> {
        let mut drained = Vec::new();
        while let Some(msg) = self.try_recv() {
            drained.push(msg);
        }
        drained
    }
}

/// A bounded steering channel with an internal priority-ordered buffer.
///
/// Uses a Mutex-protected Vec for lock-step priority ordering.
/// When the buffer is full (default capacity 8), the lowest-priority message
/// is dropped to make room. Dequeue always returns the highest-priority first.
pub struct MpscSteeringChannel {
    buffer: Mutex<Vec<PriorityMsg>>,
    capacity: usize,
}

impl Default for MpscSteeringChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl MpscSteeringChannel {
    pub fn new() -> Self {
        Self::with_capacity(8)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Mutex::new(Vec::new()),
            capacity,
        }
    }
}

#[async_trait::async_trait]
impl SteeringChannel for MpscSteeringChannel {
    async fn send(&self, msg: SteeringMessage) {
        let mut buf = self.buffer.lock().unwrap();
        if buf.len() >= self.capacity {
            // Find and remove the lowest-priority message
            let mut min_idx = 0;
            for i in 1..buf.len() {
                if buf[i] < buf[min_idx] {
                    min_idx = i;
                }
            }
            buf.swap_remove(min_idx);
        }
        buf.push(PriorityMsg(msg));
    }

    fn try_recv(&self) -> Option<SteeringMessage> {
        let mut buf = self.buffer.lock().unwrap();
        if buf.is_empty() {
            return None;
        }
        // Find the highest-priority message
        let mut max_idx = 0;
        for i in 1..buf.len() {
            if buf[i] > buf[max_idx] {
                max_idx = i;
            }
        }
        Some(buf.swap_remove(max_idx).0)
    }

    fn is_empty(&self) -> bool {
        self.buffer.lock().unwrap().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_steering_message_default() {
        let msg = SteeringMessage::new("hello");
        assert_eq!(msg.content, "hello");
        assert_eq!(msg.priority, 5);
    }

    #[test]
    fn test_steering_message_with_priority() {
        let msg = SteeringMessage::new("urgent").with_priority(10);
        assert_eq!(msg.content, "urgent");
        assert_eq!(msg.priority, 10);
    }

    #[tokio::test]
    async fn test_channel_send_recv() {
        let channel = MpscSteeringChannel::new();
        channel.send(SteeringMessage::new("first")).await;
        channel.send(SteeringMessage::new("second")).await;

        let msg1 = channel.try_recv();
        assert!(msg1.is_some());
        assert_eq!(msg1.unwrap().content, "first");

        let msg2 = channel.try_recv();
        assert!(msg2.is_some());
        assert_eq!(msg2.unwrap().content, "second");

        assert!(channel.try_recv().is_none());
        assert!(channel.is_empty());
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        let channel = MpscSteeringChannel::new();
        channel
            .send(SteeringMessage::new("low").with_priority(1))
            .await;
        channel
            .send(SteeringMessage::new("high").with_priority(9))
            .await;
        channel
            .send(SteeringMessage::new("medium").with_priority(5))
            .await;

        // Should dequeue highest priority first
        let first = channel.try_recv().unwrap();
        assert_eq!(first.priority, 9);
        assert_eq!(first.content, "high");

        let second = channel.try_recv().unwrap();
        assert_eq!(second.priority, 5);
        assert_eq!(second.content, "medium");

        let third = channel.try_recv().unwrap();
        assert_eq!(third.priority, 1);
        assert_eq!(third.content, "low");

        assert!(channel.try_recv().is_none());
    }

    #[tokio::test]
    async fn test_overflow_drops_lowest_priority() {
        let channel = MpscSteeringChannel::with_capacity(3);

        channel
            .send(SteeringMessage::new("p1").with_priority(1))
            .await;
        channel
            .send(SteeringMessage::new("p5").with_priority(5))
            .await;
        channel
            .send(SteeringMessage::new("p3").with_priority(3))
            .await;

        // Now send a high-priority message, should drop the lowest (p1)
        channel
            .send(SteeringMessage::new("p9").with_priority(9))
            .await;

        let mut contents: Vec<(String, u8)> = Vec::new();
        while let Some(msg) = channel.try_recv() {
            contents.push((msg.content.clone(), msg.priority));
        }

        // Should have 3 messages: p9, p5, p3 (p1 was dropped)
        assert_eq!(contents.len(), 3);
        assert_eq!(contents[0], ("p9".to_string(), 9));
        assert_eq!(contents[1], ("p5".to_string(), 5));
        assert_eq!(contents[2], ("p3".to_string(), 3));
    }

    #[tokio::test]
    async fn test_overflow_drops_oldest_when_same_priority() {
        let channel = MpscSteeringChannel::with_capacity(2);

        channel
            .send(SteeringMessage::new("old").with_priority(5))
            .await;
        channel
            .send(SteeringMessage::new("newer").with_priority(5))
            .await;

        // Same priority, send another should drop the lowest-ord element.
        // Since Ord gives higher precedence to earlier timestamps, the "newer"
        // message has lower ord and gets dropped.
        channel
            .send(SteeringMessage::new("newest").with_priority(5))
            .await;

        let mut contents: Vec<String> = Vec::new();
        while let Some(msg) = channel.try_recv() {
            contents.push(msg.content);
        }

        // Should have 2 messages; "newer" was dropped (latest timestamp = lowest ord)
        assert_eq!(contents.len(), 2);
        assert!(!contents.contains(&"newer".to_string()));
    }

    #[test]
    fn test_is_empty() {
        let channel = MpscSteeringChannel::new();
        assert!(channel.is_empty());
    }

    #[test]
    fn test_priority_msg_ord() {
        let early = SteeringMessage::new("a").with_priority(5);
        std::thread::sleep(std::time::Duration::from_millis(1));
        let late = SteeringMessage::new("b").with_priority(5);

        let a = PriorityMsg(early);
        let b = PriorityMsg(late);

        // Same priority: earlier timestamp should have higher ord (come out of heap first)
        assert!(a > b);

        let high = PriorityMsg(SteeringMessage::new("hi").with_priority(9));
        let low = PriorityMsg(SteeringMessage::new("lo").with_priority(1));

        assert!(high > low);
    }
}
