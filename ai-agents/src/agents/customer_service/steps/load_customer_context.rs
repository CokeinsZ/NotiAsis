use std::sync::Arc;

use async_trait::async_trait;

use crate::agents::customer_service::ports::CustomerContextReader;
use crate::agents::customer_service::turn::Turn;
use crate::pipeline::{Flow, Step};
use crate::shared::error::BoxError;

/// Business, cliente y sus guías.
pub struct LoadCustomerContext {
    reader: Arc<dyn CustomerContextReader>,
    guide_limit: i64,
}

impl LoadCustomerContext {
    pub fn new(reader: Arc<dyn CustomerContextReader>, guide_limit: i64) -> Self {
        Self { reader, guide_limit }
    }
}

#[async_trait]
impl Step<Turn> for LoadCustomerContext {
    fn name(&self) -> &'static str {
        "load_customer_context"
    }

    async fn run(&self, turn: &mut Turn) -> Result<Flow, BoxError> {
        match self.reader.load(&turn.event.chat, self.guide_limit).await? {
            Some(context) => {
                turn.context = Some(context);
                Ok(Flow::Continue)
            }
            None => Ok(Flow::Halt("el chat no existe".into())),
        }
    }
}
