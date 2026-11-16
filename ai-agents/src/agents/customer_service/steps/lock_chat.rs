use std::sync::Arc;

use async_trait::async_trait;

use crate::agents::customer_service::turn::Turn;
use crate::domain::ChatKey;
use crate::pipeline::{Flow, Step};
use crate::shared::error::BoxError;
use crate::shared::keyed_lock::KeyedLocks;

/// Un solo turno a la vez por chat: si el mismo aviso llega dos veces, el
/// segundo espera y luego la regla `not_yet_answered` lo detiene.
pub struct LockChat {
    locks: Arc<KeyedLocks<ChatKey>>,
}

impl LockChat {
    pub fn new(locks: Arc<KeyedLocks<ChatKey>>) -> Self {
        Self { locks }
    }
}

#[async_trait]
impl Step<Turn> for LockChat {
    fn name(&self) -> &'static str {
        "lock_chat"
    }

    async fn run(&self, turn: &mut Turn) -> Result<Flow, BoxError> {
        turn.chat_guard = Some(self.locks.lock(&turn.event.chat).await);
        Ok(Flow::Continue)
    }
}
