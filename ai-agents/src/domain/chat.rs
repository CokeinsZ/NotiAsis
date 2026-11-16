use std::fmt;

use crate::shared::text::mask_phone;

/// Un chat de NotiAsis: todos los business comparten el número de
/// WhatsApp, así que un chat es la pareja (business, cliente).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChatKey {
    pub business_id: i32,
    /// Teléfono normalizado (solo dígitos, sin '+').
    pub user_phone: String,
}

impl ChatKey {
    pub fn new(business_id: i32, user_phone: impl Into<String>) -> Self {
        Self { business_id, user_phone: user_phone.into() }
    }
}

/// Para logs: el teléfono va enmascarado.
impl fmt::Display for ChatKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.business_id, mask_phone(&self.user_phone))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_masks_the_phone() {
        assert_eq!(ChatKey::new(1, "573003579384").to_string(), "1:5730****9384");
    }
}
