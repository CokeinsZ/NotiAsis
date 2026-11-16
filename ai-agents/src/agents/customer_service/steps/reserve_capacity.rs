use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Semaphore;

use crate::agents::customer_service::turn::Turn;
use crate::pipeline::{Flow, Step};
use crate::shared::error::BoxError;

/// Limita los turnos que usan el modelo al mismo tiempo. Va después de la
/// espera y de las reglas, para que esperar no ocupe cupo.
pub struct ReserveCapacity {
    permits: Arc<Semaphore>,
}

impl ReserveCapacity {
    pub fn new(permits: Arc<Semaphore>) -> Self {
        Self { permits }
    }
}

#[async_trait]
impl Step<Turn> for ReserveCapacity {
    fn name(&self) -> &'static str {
        "reserve_capacity"
    }

    async fn run(&self, turn: &mut Turn) -> Result<Flow, BoxError> {
        turn.capacity = Some(self.permits.clone().acquire_owned().await?);
        Ok(Flow::Continue)
    }
}
