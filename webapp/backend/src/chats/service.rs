use std::sync::Arc;

use async_trait::async_trait;

use crate::chats::dtos::ChatSummary;
use crate::chats::repository::ChatRepositoryTrait;
use crate::messages::service::is_window_open_at;

#[async_trait]
pub trait ChatServiceTrait: Send + Sync {
    async fn get_chats(&self, business_id: i32) -> Result<Vec<ChatSummary>, String>;
    /// Marca/desmarca un chat como importante.
    async fn set_chat_importance(&self, business_id: i32, user_phone: &str, is_important: bool) -> Result<(), String>;
}

pub struct ChatService {
    repository: Arc<dyn ChatRepositoryTrait>,
}

impl ChatService {
    pub fn new(repository: Arc<dyn ChatRepositoryTrait>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl ChatServiceTrait for ChatService {
    async fn get_chats(&self, business_id: i32) -> Result<Vec<ChatSummary>, String> {
        let mut chats = self.repository.get_chats_by_business(business_id).await?;
        let now = chrono::Utc::now().naive_utc();

        for chat in &mut chats {
            chat.window_open = is_window_open_at(chat.last_user_message_timestamp, now);
        }

        Ok(chats)
    }

    async fn set_chat_importance(&self, business_id: i32, user_phone: &str, is_important: bool) -> Result<(), String> {
        let updated = self.repository
            .set_importance(business_id, &crate::tools::phones::normalize_phone(user_phone), is_important)
            .await?;

        if !updated {
            return Err("Chat not found".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chats::dtos::Chat;
    use std::sync::Mutex;

    struct FakeChatRepository {
        chats: Mutex<Vec<Chat>>,
    }

    #[async_trait]
    impl ChatRepositoryTrait for FakeChatRepository {
        async fn get_chats_by_business(&self, business_id: i32) -> Result<Vec<ChatSummary>, String> {
            Ok(self.chats.lock().unwrap().iter()
                .filter(|c| c.business_id == business_id)
                .map(|c| ChatSummary {
                    business_id: c.business_id,
                    user_id: c.user_id.clone(),
                    user_full_name: "Test User".to_string(),
                    last_user_message: c.last_user_message.clone(),
                    last_user_message_timestamp: c.last_user_message_timestamp,
                    last_activity: c.last_user_message_timestamp,
                    is_important: c.is_important,
                    last_guide_notification_at: c.last_guide_notification_at,
                    window_open: false,
                })
                .collect())
        }

        async fn get_chat(&self, business_id: i32, user_id: &str) -> Result<Option<Chat>, String> {
            Ok(self.chats.lock().unwrap().iter()
                .find(|c| c.business_id == business_id && c.user_id == user_id)
                .cloned())
        }

        async fn upsert_chat(&self, _: i32, _: &str) -> Result<(), String> {
            Ok(())
        }

        async fn update_last_user_message(&self, _: i32, _: &str, _: &str, _: chrono::NaiveDateTime) -> Result<(), String> {
            Ok(())
        }

        async fn find_latest_chat_business(&self, _: &str) -> Result<Option<i32>, String> {
            Ok(None)
        }

        async fn set_importance(&self, business_id: i32, user_id: &str, is_important: bool) -> Result<bool, String> {
            let mut chats = self.chats.lock().unwrap();
            match chats.iter_mut().find(|c| c.business_id == business_id && c.user_id == user_id) {
                Some(chat) => {
                    chat.is_important = is_important;
                    Ok(true)
                }
                None => Ok(false),
            }
        }

        async fn touch_guide_notification(&self, _: &str, _: chrono::NaiveDateTime) -> Result<u64, String> {
            Ok(1)
        }
    }

    #[tokio::test]
    async fn set_importance_marks_and_unmarks() {
        let repository = Arc::new(FakeChatRepository {
            chats: Mutex::new(vec![
                Chat { business_id: 1, user_id: "573003579384".into(), last_user_message_timestamp: None, last_user_message: None, is_important: false, last_guide_notification_at: None },
            ]),
        });
        let service = ChatService::new(repository.clone());

        service.set_chat_importance(1, "573003579384", true).await.unwrap();
        assert!(repository.chats.lock().unwrap()[0].is_important);

        service.set_chat_importance(1, "573003579384", false).await.unwrap();
        assert!(!repository.chats.lock().unwrap()[0].is_important);

        // Chat inexistente -> error controlado
        assert_eq!(
            service.set_chat_importance(1, "57999", true).await.unwrap_err(),
            "Chat not found"
        );
    }

    #[tokio::test]
    async fn window_open_flag_is_computed_per_chat() {
        let now = chrono::Utc::now().naive_utc();
        let repository = Arc::new(FakeChatRepository {
            chats: Mutex::new(vec![
                Chat { business_id: 1, user_id: "recent".into(), last_user_message_timestamp: Some(now - chrono::Duration::hours(1)), last_user_message: None, is_important: false, last_guide_notification_at: None },
                Chat { business_id: 1, user_id: "old".into(), last_user_message_timestamp: Some(now - chrono::Duration::hours(48)), last_user_message: None, is_important: false, last_guide_notification_at: None },
                Chat { business_id: 1, user_id: "never".into(), last_user_message_timestamp: None, last_user_message: None, is_important: false, last_guide_notification_at: None },
            ]),
        });
        let service = ChatService::new(repository);

        let chats = service.get_chats(1).await.unwrap();
        let window_of = |user: &str| chats.iter().find(|c| c.user_id == user).unwrap().window_open;

        assert!(window_of("recent"));
        assert!(!window_of("old"));
        assert!(!window_of("never"));
    }
}
