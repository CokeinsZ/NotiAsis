use std::sync::Arc;

use async_trait::async_trait;

use crate::chats::repository::ChatRepositoryTrait;
use crate::guides::dtos::{Guide, GuideRegistration, RegisterGuideDto};
use crate::guides::repository::GuideRepositoryTrait;
use crate::tools::phones::normalize_phone;
use crate::users::repository::UserRepositoryTrait;

#[async_trait]
pub trait GuideServiceTrait: Send + Sync {
    /// Registra una guía recibida. `created == false` significa que la
    /// guía ya existía y no se debe volver a notificar al usuario.
    async fn register_guide(&self, dto: RegisterGuideDto) -> Result<GuideRegistration, String>;
    async fn get_guides(&self, user_phone: Option<String>) -> Result<Vec<Guide>, String>;
    /// Consulta una guía puntual (el bot la usa para decidir entre
    /// notificación completa o recordatorio).
    async fn get_guide(&self, number: &str) -> Result<Guide, String>;
    async fn mark_notified(&self, number: &str) -> Result<(), String>;
}

pub struct GuideService {
    guide_repository: Arc<dyn GuideRepositoryTrait>,
    user_repository: Arc<dyn UserRepositoryTrait>,
    chat_repository: Arc<dyn ChatRepositoryTrait>,
}

impl GuideService {
    pub fn new(
        guide_repository: Arc<dyn GuideRepositoryTrait>,
        user_repository: Arc<dyn UserRepositoryTrait>,
        chat_repository: Arc<dyn ChatRepositoryTrait>,
    ) -> Self {
        Self { guide_repository, user_repository, chat_repository }
    }
}

#[async_trait]
impl GuideServiceTrait for GuideService {
    async fn register_guide(&self, dto: RegisterGuideDto) -> Result<GuideRegistration, String> {
        let user_phone = normalize_phone(&dto.user_phone);

        self.user_repository
            .upsert_user(&user_phone, dto.user_name.as_deref().unwrap_or(""))
            .await?;

        if let Some(guide) = self.guide_repository.insert_guide_if_new(&dto.number, &user_phone).await? {
            return Ok(GuideRegistration { guide, created: true });
        }

        let existing = self.guide_repository
            .get_guide_by_number(&dto.number)
            .await?
            .ok_or_else(|| "Guide could not be registered".to_string())?;

        Ok(GuideRegistration { guide: existing, created: false })
    }

    async fn get_guides(&self, user_phone: Option<String>) -> Result<Vec<Guide>, String> {
        self.guide_repository.get_guides(user_phone.as_deref()).await
    }

    async fn get_guide(&self, number: &str) -> Result<Guide, String> {
        match self.guide_repository.get_guide_by_number(number).await? {
            Some(guide) => Ok(guide),
            None => Err("Guide not found".to_string()),
        }
    }

    async fn mark_notified(&self, number: &str) -> Result<(), String> {
        let now = chrono::Utc::now().naive_utc();

        let guide = self.guide_repository
            .get_guide_by_number(number)
            .await?
            .ok_or_else(|| "Guide not found".to_string())?;

        self.guide_repository.mark_notified(number, now).await?;

        // Reflejar la notificación en los chats del usuario (criterio de
        // orden de la bandeja).
        self.chat_repository
            .touch_guide_notification(&guide.user_id, now)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chats::repository::ChatRepositoryTrait;
    use crate::users::dtos::User;
    use chrono::NaiveDateTime;
    use std::sync::Mutex;

    struct FakeGuideRepository {
        guides: Mutex<Vec<Guide>>,
    }

    struct FakeChatRepository {
        touched: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl ChatRepositoryTrait for FakeChatRepository {
        async fn get_chats_by_business(&self, _: i32) -> Result<Vec<crate::chats::dtos::ChatSummary>, String> {
            Ok(Vec::new())
        }

        async fn get_chat(&self, _: i32, _: &str) -> Result<Option<crate::chats::dtos::Chat>, String> {
            Ok(None)
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

        async fn set_importance(&self, _: i32, _: &str, _: bool) -> Result<bool, String> {
            Ok(true)
        }

        async fn touch_guide_notification(&self, user_id: &str, _: chrono::NaiveDateTime) -> Result<u64, String> {
            self.touched.lock().unwrap().push(user_id.to_string());
            Ok(1)
        }
    }

    impl FakeGuideRepository {
        fn new() -> Self {
            Self { guides: Mutex::new(Vec::new()) }
        }
    }

    #[async_trait]
    impl GuideRepositoryTrait for FakeGuideRepository {
        async fn insert_guide_if_new(&self, number: &str, user_id: &str) -> Result<Option<Guide>, String> {
            let mut guides = self.guides.lock().unwrap();
            if guides.iter().any(|g| g.number == number) {
                return Ok(None);
            }
            let guide = Guide {
                number: number.to_string(),
                user_id: user_id.to_string(),
                last_notification_timestamp: None,
                notification_count: 0,
            };
            guides.push(guide.clone());
            Ok(Some(guide))
        }

        async fn get_guide_by_number(&self, number: &str) -> Result<Option<Guide>, String> {
            Ok(self.guides.lock().unwrap().iter().find(|g| g.number == number).cloned())
        }

        async fn get_guides(&self, user_phone: Option<&str>) -> Result<Vec<Guide>, String> {
            Ok(self.guides.lock().unwrap().iter()
                .filter(|g| user_phone.is_none() || Some(g.user_id.as_str()) == user_phone)
                .cloned()
                .collect())
        }

        async fn mark_notified(&self, number: &str, timestamp: NaiveDateTime) -> Result<bool, String> {
            let mut guides = self.guides.lock().unwrap();
            match guides.iter_mut().find(|g| g.number == number) {
                Some(guide) => {
                    guide.last_notification_timestamp = Some(timestamp);
                    Ok(true)
                }
                None => Ok(false),
            }
        }
    }

    struct FakeUserRepository;

    #[async_trait]
    impl UserRepositoryTrait for FakeUserRepository {
        async fn upsert_user(&self, phone_number: &str, full_name: &str) -> Result<User, String> {
            Ok(User { phone_number: phone_number.to_string(), full_name: full_name.to_string() })
        }

        async fn get_user_by_phone(&self, _: &str) -> Result<Option<User>, String> {
            Ok(None)
        }

        async fn get_users(&self) -> Result<Vec<User>, String> {
            Ok(Vec::new())
        }
    }

    fn build_service() -> (GuideService, Arc<FakeGuideRepository>, Arc<FakeChatRepository>) {
        let guide_repo = Arc::new(FakeGuideRepository::new());
        let chat_repo = Arc::new(FakeChatRepository { touched: Mutex::new(Vec::new()) });
        (
            GuideService::new(guide_repo.clone(), Arc::new(FakeUserRepository), chat_repo.clone()),
            guide_repo,
            chat_repo,
        )
    }

    fn dto(number: &str) -> RegisterGuideDto {
        RegisterGuideDto {
            number: number.to_string(),
            user_phone: "573003579384".to_string(),
            user_name: Some("Stiven".to_string()),
        }
    }

    #[tokio::test]
    async fn new_guide_is_created_once() {
        let (service, _, _) = build_service();

        let first = service.register_guide(dto("GUIA123")).await.unwrap();
        assert!(first.created);

        let second = service.register_guide(dto("GUIA123")).await.unwrap();
        assert!(!second.created); // duplicada: no notificar de nuevo
    }

    #[tokio::test]
    async fn different_guides_are_all_created() {
        let (service, _, _) = build_service();
        assert!(service.register_guide(dto("A")).await.unwrap().created);
        assert!(service.register_guide(dto("B")).await.unwrap().created);
    }

    #[tokio::test]
    async fn mark_notified_sets_timestamp_and_touches_chat() {
        let (service, repository, chat_repo) = build_service();
        service.register_guide(dto("GUIA123")).await.unwrap();

        service.mark_notified("GUIA123").await.unwrap();

        let guide = repository.guides.lock().unwrap()[0].clone();
        assert!(guide.last_notification_timestamp.is_some());
        // La notificación también se refleja en los chats del usuario
        assert_eq!(chat_repo.touched.lock().unwrap().as_slice(), ["573003579384"]);

        assert_eq!(service.mark_notified("NOEXISTE").await.unwrap_err(), "Guide not found");
    }
}
