use std::collections::HashSet;

use crate::agents::customer_service::policy::{ReplyRule, RuleInput};

/// Lista blanca de teléfonos para probar en producción con pocos números.
/// Vacía = todos.
pub struct Allowlist {
    phones: HashSet<String>,
}

impl Allowlist {
    pub fn new(phones: HashSet<String>) -> Self {
        Self { phones }
    }
}

impl ReplyRule for Allowlist {
    fn name(&self) -> &'static str {
        "allowlist"
    }

    fn check(&self, input: &RuleInput<'_>) -> Result<(), String> {
        if self.phones.is_empty() || self.phones.contains(&input.event.chat.user_phone) {
            Ok(())
        } else {
            Err("el teléfono no está en CUSTOMER_AGENT_ALLOWED_PHONES".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::customer_service::policy::fixtures::{event, snapshot};

    #[test]
    fn empty_list_allows_everyone() {
        let (event, snapshot) = (event(), snapshot());
        let input = RuleInput { event: &event, snapshot: &snapshot };
        assert!(Allowlist::new(HashSet::new()).check(&input).is_ok());
        assert!(Allowlist::new(HashSet::from(["573003579384".to_string()])).check(&input).is_ok());
        assert!(Allowlist::new(HashSet::from(["50241571896".to_string()])).check(&input).is_err());
    }
}
