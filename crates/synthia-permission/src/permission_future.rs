use std::{
    pin::Pin,
    task::{Context, Poll},
};

use tokio::sync::oneshot;

use crate::types::PermissionOutcome;

pub struct PermissionFuture {
    rx: oneshot::Receiver<Result<PermissionOutcome, PermissionFutureError>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionFutureError {
    Cancelled,
    Denied,
    Dropped,
}

impl Future for PermissionFuture {
    type Output = Result<PermissionOutcome, PermissionFutureError>;

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        match Pin::new(&mut self.rx).poll(cx) {
            Poll::Ready(Ok(Ok(outcome))) => Poll::Ready(Ok(outcome)),
            Poll::Ready(Ok(Err(e))) => Poll::Ready(Err(e)),
            Poll::Ready(Err(_)) => {
                Poll::Ready(Err(PermissionFutureError::Dropped))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl PermissionFuture {
    pub async fn await_with_cancellation(
        self,
        token: &tokio_util::sync::CancellationToken,
    ) -> Result<PermissionOutcome, PermissionFutureError> {
        tokio::select! {
            result = self => result,
            _ = token.cancelled() => Err(PermissionFutureError::Cancelled),
        }
    }

    pub fn immediate_denied() -> Self {
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(Err(PermissionFutureError::Denied));
        Self { rx }
    }

    pub fn immediate_granted(outcome: PermissionOutcome) -> Self {
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(Ok(outcome));
        Self { rx }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn permission_future_immediate_denied() {
        let future = PermissionFuture::immediate_denied();
        let result = future.await;
        assert!(matches!(result, Err(PermissionFutureError::Denied)));
    }

    #[tokio::test]
    async fn permission_future_immediate_granted() {
        let outcome = PermissionOutcome {
            tool_name: "test".to_string(),
            outcome: crate::level::Permission::AutoApprove,
        };
        let future = PermissionFuture::immediate_granted(outcome.clone());
        let result = future.await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn permission_future_cancelled() {
        let token = tokio_util::sync::CancellationToken::new();
        let (tx, rx) = oneshot::channel();
        let future = PermissionFuture { rx };
        token.cancel();
        let result = future.await_with_cancellation(&token).await;
        assert!(matches!(result, Err(PermissionFutureError::Cancelled)));
        drop(tx);
    }
}
