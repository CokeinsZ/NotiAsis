use std::time::Duration;

use async_trait::async_trait;

use crate::agents::customer_service::turn::Turn;
use crate::pipeline::{Flow, Step};
use crate::shared::error::BoxError;

/// Espera unos segundos: los clientes suelen mandar varios mensajes
/// seguidos. Si llega otro, la regla `latest_message` deja que responda el
/// turno de ese último, con todos juntos.
pub struct WaitForSilence {
    quiet_period: Duration,
}

impl WaitForSilence {
    pub fn new(quiet_period: Duration) -> Self {
        Self { quiet_period }
    }
}

#[async_trait]
impl Step<Turn> for WaitForSilence {
    fn name(&self) -> &'static str {
        "wait_for_silence"
    }

    async fn run(&self, _: &mut Turn) -> Result<Flow, BoxError> {
        tokio::time::sleep(self.quiet_period).await;
        Ok(Flow::Continue)
    }
}
