use std::sync::Arc;

use tokio::sync::{mpsc, watch};

#[derive(Debug, Clone)]
pub enum MailboxMessage {
    Text(String),
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MailboxDeliveryPhase {
    /// Currently processing this turn's messages.
    #[default]
    CurrentTurn,
    /// Messages queued for next turn.
    NextTurn,
    /// Mailbox suspended (Ask pending) — no messages delivered until resumed.
    Suspended,
}

impl MailboxDeliveryPhase {
    pub fn is_suspended(&self) -> bool {
        matches!(self, MailboxDeliveryPhase::Suspended)
    }

    pub fn suspend(&mut self) {
        *self = MailboxDeliveryPhase::Suspended;
    }

    pub fn resume(&mut self) {
        *self = MailboxDeliveryPhase::NextTurn;
    }
}

pub struct Mailbox {
    sender: mpsc::Sender<MailboxMessage>,
    sequence_rx: watch::Receiver<u64>,
    _sequence_tx: Arc<watch::Sender<u64>>,
}

impl Mailbox {
    pub fn new() -> Self {
        let (sequence_tx, sequence_rx) = watch::channel(0u64);
        let sequence_tx = Arc::new(sequence_tx);
        let (sender, mut receiver) = mpsc::channel::<MailboxMessage>(100);

        let sequence_tx_clone = Arc::clone(&sequence_tx);
        tokio::spawn(async move {
            let mut seq = 0u64;
            while receiver.recv().await.is_some() {
                seq += 1;
                let _ = sequence_tx_clone.send(seq);
            }
        });

        Self {
            sender,
            sequence_rx,
            _sequence_tx: sequence_tx,
        }
    }

    pub async fn send(&self, msg: MailboxMessage) -> Result<(), String> {
        self.sender
            .send(msg)
            .await
            .map_err(|e| format!("mailbox send failed: {}", e))
    }

    pub fn sequence(&self) -> u64 {
        *self.sequence_rx.borrow()
    }
}

impl Default for Mailbox {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mailbox_send_increments_sequence() {
        let mailbox = Mailbox::new();
        let initial = mailbox.sequence();
        mailbox
            .send(MailboxMessage::Text("hello".into()))
            .await
            .unwrap();
        tokio::task::yield_now().await;
        assert!(mailbox.sequence() > initial);
    }

    #[test]
    fn test_phase_transitions() {
        let mut phase = MailboxDeliveryPhase::default();
        assert_eq!(phase, MailboxDeliveryPhase::CurrentTurn);

        phase.suspend();
        assert!(phase.is_suspended());

        phase.resume();
        assert_eq!(phase, MailboxDeliveryPhase::NextTurn);
    }
}
