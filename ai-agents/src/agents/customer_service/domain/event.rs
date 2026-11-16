use crate::domain::ChatKey;

/// Un cliente escribió: el bot ya registró el mensaje (`messages.id`) en el
/// chat que el backend eligió.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingMessageEvent {
    pub message_id: i32,
    pub chat: ChatKey,
}
