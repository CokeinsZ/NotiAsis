use std::sync::Arc;

use async_trait::async_trait;

use crate::ai_activity::dtos::ChatAiActivity;
use crate::ai_activity::repository::AiActivityRepositoryTrait;
use crate::tools::phones::normalize_phone;

#[async_trait]
pub trait AiActivityServiceTrait: Send + Sync {
    /// Escalaciones y novedades de guías que el agente registró en el chat.
    async fn get_chat_activity(&self, business_id: i32, user_phone: &str) -> Result<ChatAiActivity, String>;
}

pub struct AiActivityService {
    repository: Arc<dyn AiActivityRepositoryTrait>,
}

impl AiActivityService {
    pub fn new(repository: Arc<dyn AiActivityRepositoryTrait>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl AiActivityServiceTrait for AiActivityService {
    async fn get_chat_activity(&self, business_id: i32, user_phone: &str) -> Result<ChatAiActivity, String> {
        let user_id = normalize_phone(user_phone);
        let (escalations, guide_updates) = tokio::try_join!(
            self.repository.get_escalations(business_id, &user_id),
            self.repository.get_guide_updates(business_id, &user_id),
        )?;
        Ok(ChatAiActivity { escalations, guide_updates })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_activity::dtos::{AiEscalation, GuideCustomerUpdate};
    use chrono::NaiveDate;

    /// Responde solo al chat (1, "573003579384").
    struct FakeAiActivityRepository;

    fn at(day: u32, hour: u32) -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap().and_hms_opt(hour, 0, 0).unwrap()
    }

    fn is_known_chat(business_id: i32, user_id: &str) -> bool {
        business_id == 1 && user_id == "573003579384"
    }

    #[async_trait]
    impl AiActivityRepositoryTrait for FakeAiActivityRepository {
        async fn get_escalations(&self, business_id: i32, user_id: &str) -> Result<Vec<AiEscalation>, String> {
            if !is_known_chat(business_id, user_id) {
                return Ok(Vec::new());
            }
            Ok(vec![AiEscalation {
                id: 1,
                category: "cancellation".into(),
                reason: "Quiere cancelar el pedido".into(),
                guide_number: Some("240064842467".into()),
                created_at: at(16, 15),
            }])
        }

        async fn get_guide_updates(&self, business_id: i32, user_id: &str) -> Result<Vec<GuideCustomerUpdate>, String> {
            if !is_known_chat(business_id, user_id) {
                return Ok(Vec::new());
            }
            Ok(vec![GuideCustomerUpdate {
                id: 7,
                guide_number: "240064842467".into(),
                kind: "will_pickup".into(),
                promised_date: NaiveDate::from_ymd_opt(2026, 11, 18),
                note: Some("espera la quincena".into()),
                created_at: at(15, 20),
            }])
        }
    }

    #[tokio::test]
    async fn returns_both_lists_and_normalizes_the_phone() {
        let service = AiActivityService::new(Arc::new(FakeAiActivityRepository));

        // El teléfono llega con '+' desde la URL y se guarda sin él.
        let activity = service.get_chat_activity(1, "+573003579384").await.unwrap();

        assert_eq!(activity.escalations.len(), 1);
        assert_eq!(activity.escalations[0].category, "cancellation");
        assert_eq!(activity.guide_updates.len(), 1);
        assert_eq!(activity.guide_updates[0].promised_date, NaiveDate::from_ymd_opt(2026, 11, 18));
    }

    #[tokio::test]
    async fn a_chat_without_activity_returns_empty_lists() {
        let service = AiActivityService::new(Arc::new(FakeAiActivityRepository));

        let activity = service.get_chat_activity(1, "573009999999").await.unwrap();

        assert_eq!(activity, ChatAiActivity::default());
    }
}
