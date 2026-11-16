use std::sync::Arc;

use async_trait::async_trait;

use crate::agents::customer_service::eligibility::EligibilityGate;
use crate::agents::customer_service::policy::Verdict;
use crate::agents::customer_service::turn::Turn;
use crate::pipeline::{Flow, Step};
use crate::shared::error::BoxError;

/// Aplica las reglas de `policy/` antes de gastar en el modelo.
pub struct CheckEligibility {
    gate: Arc<EligibilityGate>,
}

impl CheckEligibility {
    pub fn new(gate: Arc<EligibilityGate>) -> Self {
        Self { gate }
    }
}

#[async_trait]
impl Step<Turn> for CheckEligibility {
    fn name(&self) -> &'static str {
        "check_eligibility"
    }

    async fn run(&self, turn: &mut Turn) -> Result<Flow, BoxError> {
        Ok(match self.gate.verdict(&turn.event).await? {
            Verdict::Reply => Flow::Continue,
            Verdict::Skip { rule, reason } => Flow::Halt(format!("{rule}: {reason}")),
        })
    }
}
