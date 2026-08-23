use std::sync::Arc;

use async_trait::async_trait;

use crate::businesses::dtos::{
    Business, BusinessAssociate, BusinessUsersSheet, CreateAssociateDto, CreateBusinessDto,
};
use crate::businesses::repository::BusinessRepositoryTrait;
use crate::tools::phones::normalize_phone;

#[async_trait]
pub trait BusinessServiceTrait: Send + Sync {
    async fn create_business(&self, dto: CreateBusinessDto) -> Result<Business, String>;
    async fn get_businesses(&self) -> Result<Vec<Business>, String>;
    async fn get_business(&self, id: i32) -> Result<Business, String>;
    async fn get_business_sheet(&self, business_id: i32) -> Result<BusinessUsersSheet, String>;
    async fn create_associate(&self, business_id: i32, dto: CreateAssociateDto) -> Result<BusinessAssociate, String>;
    async fn get_associates(&self, business_id: i32) -> Result<Vec<BusinessAssociate>, String>;
    async fn get_all_associates(&self) -> Result<Vec<BusinessAssociate>, String>;
    async fn get_associate_phones(&self) -> Result<Vec<String>, String>;
}

pub struct BusinessService {
    repository: Arc<dyn BusinessRepositoryTrait>,
}

impl BusinessService {
    pub fn new(repository: Arc<dyn BusinessRepositoryTrait>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl BusinessServiceTrait for BusinessService {
    async fn create_business(&self, dto: CreateBusinessDto) -> Result<Business, String> {
        self.repository.save_business(&dto).await
    }

    async fn get_businesses(&self) -> Result<Vec<Business>, String> {
        self.repository.get_businesses().await
    }

    async fn get_business(&self, id: i32) -> Result<Business, String> {
        match self.repository.get_business_by_id(id).await? {
            Some(business) => Ok(business),
            None => Err("Business not found".to_string()),
        }
    }

    async fn get_business_sheet(&self, business_id: i32) -> Result<BusinessUsersSheet, String> {
        match self.repository.get_sheet_by_business(business_id).await? {
            Some(sheet) => Ok(sheet),
            None => Err("Sheet config not found for business".to_string()),
        }
    }

    async fn create_associate(&self, business_id: i32, dto: CreateAssociateDto) -> Result<BusinessAssociate, String> {
        if self.repository.get_business_by_id(business_id).await?.is_none() {
            return Err("Business not found".to_string());
        }

        if self.repository.username_exists(&dto.username).await? {
            return Err("Username already exists".to_string());
        }

        let password_hash = bcrypt::hash(&dto.password, bcrypt::DEFAULT_COST)
            .map_err(|e| e.to_string())?;

        let dto = CreateAssociateDto {
            phone_number: normalize_phone(&dto.phone_number),
            ..dto
        };
        self.repository.save_associate(business_id, &dto, &password_hash).await
    }

    async fn get_associates(&self, business_id: i32) -> Result<Vec<BusinessAssociate>, String> {
        if self.repository.get_business_by_id(business_id).await?.is_none() {
            return Err("Business not found".to_string());
        }
        self.repository.get_associates_by_business(business_id).await
    }

    async fn get_all_associates(&self) -> Result<Vec<BusinessAssociate>, String> {
        self.repository.get_all_associates().await
    }

    async fn get_associate_phones(&self) -> Result<Vec<String>, String> {
        self.repository.get_all_associate_phones().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeBusinessRepository {
        businesses: Mutex<Vec<Business>>,
        associates: Mutex<Vec<(i32, CreateAssociateDto, String)>>,
    }

    impl FakeBusinessRepository {
        fn new() -> Self {
            Self {
                businesses: Mutex::new(Vec::new()),
                associates: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl BusinessRepositoryTrait for FakeBusinessRepository {
        async fn save_business(&self, dto: &CreateBusinessDto) -> Result<Business, String> {
            let mut businesses = self.businesses.lock().unwrap();
            let business = Business {
                id: businesses.len() as i32 + 1,
                name: dto.name.clone(),
                state: crate::businesses::dtos::EntityState::Active,
                created_at: chrono::Utc::now().naive_utc(),
                updated_at: chrono::Utc::now().naive_utc(),
            };
            businesses.push(business.clone());
            Ok(business)
        }

        async fn get_businesses(&self) -> Result<Vec<Business>, String> {
            Ok(self.businesses.lock().unwrap().clone())
        }

        async fn get_business_by_id(&self, id: i32) -> Result<Option<Business>, String> {
            Ok(self.businesses.lock().unwrap().iter().find(|b| b.id == id).cloned())
        }

        async fn get_sheet_by_business(&self, _: i32) -> Result<Option<BusinessUsersSheet>, String> {
            Ok(None)
        }

        async fn save_associate(&self, business_id: i32, dto: &CreateAssociateDto, password_hash: &str) -> Result<BusinessAssociate, String> {
            self.associates.lock().unwrap().push((business_id, CreateAssociateDto {
                phone_number: dto.phone_number.clone(),
                username: dto.username.clone(),
                password: dto.password.clone(),
            }, password_hash.to_string()));
            Ok(BusinessAssociate {
                id: 1,
                business_id,
                phone_number: dto.phone_number.clone(),
                username: dto.username.clone(),
            })
        }

        async fn get_associates_by_business(&self, business_id: i32) -> Result<Vec<BusinessAssociate>, String> {
            Ok(self.associates.lock().unwrap().iter()
                .filter(|(bid, _, _)| *bid == business_id)
                .enumerate()
                .map(|(i, (bid, dto, _))| BusinessAssociate {
                    id: i as i32 + 1,
                    business_id: *bid,
                    phone_number: dto.phone_number.clone(),
                    username: dto.username.clone(),
                })
                .collect())
        }

        async fn get_all_associates(&self) -> Result<Vec<BusinessAssociate>, String> {
            Ok(self.associates.lock().unwrap().iter()
                .enumerate()
                .map(|(i, (bid, dto, _))| BusinessAssociate {
                    id: i as i32 + 1,
                    business_id: *bid,
                    phone_number: dto.phone_number.clone(),
                    username: dto.username.clone(),
                })
                .collect())
        }

        async fn get_all_associate_phones(&self) -> Result<Vec<String>, String> {
            Ok(self.associates.lock().unwrap().iter().map(|(_, dto, _)| dto.phone_number.clone()).collect())
        }

        async fn username_exists(&self, username: &str) -> Result<bool, String> {
            Ok(self.associates.lock().unwrap().iter().any(|(_, dto, _)| dto.username == username))
        }
    }

    fn build_service() -> (BusinessService, Arc<FakeBusinessRepository>) {
        let repository = Arc::new(FakeBusinessRepository::new());
        (BusinessService::new(repository.clone()), repository)
    }

    #[tokio::test]
    async fn create_and_get_business() {
        let (service, _) = build_service();
        let business = service.create_business(CreateBusinessDto { name: "Mi Empresa".into() }).await.unwrap();
        assert_eq!(business.id, 1);
        assert!(service.get_business(1).await.is_ok());
        assert!(service.get_business(999).await.is_err());
    }

    #[tokio::test]
    async fn associate_requires_existing_business() {
        let (service, _) = build_service();
        let dto = CreateAssociateDto {
            phone_number: "573003579384".into(),
            username: "stiven".into(),
            password: "Passw0rd".into(),
        };
        assert_eq!(service.create_associate(42, dto).await.unwrap_err(), "Business not found");
    }

    #[tokio::test]
    async fn associate_password_is_hashed() {
        let (service, repository) = build_service();
        service.create_business(CreateBusinessDto { name: "Mi Empresa".into() }).await.unwrap();

        let dto = CreateAssociateDto {
            phone_number: "573003579384".into(),
            username: "stiven".into(),
            password: "Passw0rd".into(),
        };
        service.create_associate(1, dto).await.unwrap();

        let stored_hash = &repository.associates.lock().unwrap()[0].2;
        assert_ne!(stored_hash, "Passw0rd");
        assert!(bcrypt::verify("Passw0rd", stored_hash).unwrap());
    }

    #[tokio::test]
    async fn duplicate_username_is_rejected() {
        let (service, _) = build_service();
        service.create_business(CreateBusinessDto { name: "Mi Empresa".into() }).await.unwrap();

        let dto = CreateAssociateDto {
            phone_number: "573003579384".into(),
            username: "stiven".into(),
            password: "Passw0rd".into(),
        };
        service.create_associate(1, CreateAssociateDto { phone_number: dto.phone_number.clone(), username: dto.username.clone(), password: dto.password.clone() }).await.unwrap();

        assert_eq!(service.create_associate(1, dto).await.unwrap_err(), "Username already exists");
    }
}
