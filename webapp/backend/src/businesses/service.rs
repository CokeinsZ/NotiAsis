use std::sync::Arc;

use async_trait::async_trait;

use crate::businesses::dtos::{
    Business, BusinessAssociate, BusinessUsersSheet, BusinessWithCountry, Country, CreateAssociateDto,
    CreateBusinessDto,
};
use crate::businesses::repository::{BusinessRepositoryTrait, PHONE_TAKEN, USERNAME_TAKEN};

#[async_trait]
pub trait BusinessServiceTrait: Send + Sync {
    async fn create_business(&self, dto: CreateBusinessDto) -> Result<Business, String>;
    async fn get_businesses(&self) -> Result<Vec<Business>, String>;
    async fn get_business(&self, id: i32) -> Result<BusinessWithCountry, String>;
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

    async fn country(&self, id: i32) -> Result<Country, String> {
        self.repository
            .get_country_by_id(id)
            .await?
            .ok_or_else(|| "Country not found".to_string())
    }
}

#[async_trait]
impl BusinessServiceTrait for BusinessService {
    async fn create_business(&self, dto: CreateBusinessDto) -> Result<Business, String> {
        self.country(dto.country_id).await?;
        self.repository.save_business(&dto).await
    }

    async fn get_businesses(&self) -> Result<Vec<Business>, String> {
        self.repository.get_businesses().await
    }

    async fn get_business(&self, id: i32) -> Result<BusinessWithCountry, String> {
        let business = self
            .repository
            .get_business_by_id(id)
            .await?
            .ok_or_else(|| "Business not found".to_string())?;
        let country = self.country(business.country_id).await?;
        Ok(BusinessWithCountry { business, country })
    }

    async fn get_business_sheet(&self, business_id: i32) -> Result<BusinessUsersSheet, String> {
        match self.repository.get_sheet_by_business(business_id).await? {
            Some(sheet) => Ok(sheet),
            None => Err("Sheet config not found for business".to_string()),
        }
    }

    async fn create_associate(&self, business_id: i32, dto: CreateAssociateDto) -> Result<BusinessAssociate, String> {
        let business = self.get_business(business_id).await?;

        // Se guarda con indicativo: así llega el número en los webhooks de Meta.
        let phone_number = business.country.international_phone(&dto.phone_number).ok_or_else(|| {
            format!("Phone number is not valid for {}", business.country.name)
        })?;

        if self.repository.username_exists(&dto.username).await? {
            return Err(USERNAME_TAKEN.to_string());
        }
        if self.repository.associate_phone_exists(&phone_number).await? {
            return Err(PHONE_TAKEN.to_string());
        }

        let password_hash = bcrypt::hash(&dto.password, bcrypt::DEFAULT_COST)
            .map_err(|e| e.to_string())?;

        let dto = CreateAssociateDto { phone_number, ..dto };
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
        countries: Vec<Country>,
        associates: Mutex<Vec<(i32, CreateAssociateDto, String)>>,
    }

    const COLOMBIA: i32 = 1;
    const GUATEMALA: i32 = 2;

    impl FakeBusinessRepository {
        fn new() -> Self {
            let country = |id, code: &str, name: &str, prefix: &str, length| Country {
                id,
                code: code.into(),
                name: name.into(),
                phone_prefix: prefix.into(),
                phone_length: length,
            };
            Self {
                businesses: Mutex::new(Vec::new()),
                countries: vec![
                    country(COLOMBIA, "CO", "Colombia", "57", 10),
                    country(GUATEMALA, "GT", "Guatemala", "502", 8),
                ],
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
                country_id: dto.country_id,
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

        async fn get_country_by_id(&self, id: i32) -> Result<Option<Country>, String> {
            Ok(self.countries.iter().find(|c| c.id == id).cloned())
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

        async fn associate_phone_exists(&self, phone_number: &str) -> Result<bool, String> {
            Ok(self.associates.lock().unwrap().iter().any(|(_, dto, _)| dto.phone_number == phone_number))
        }
    }

    fn build_service() -> (BusinessService, Arc<FakeBusinessRepository>) {
        let repository = Arc::new(FakeBusinessRepository::new());
        (BusinessService::new(repository.clone()), repository)
    }

    #[tokio::test]
    async fn create_and_get_business() {
        let (service, _) = build_service();
        let business = service.create_business(CreateBusinessDto { name: "Mi Empresa".into(), country_id: COLOMBIA }).await.unwrap();
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
        service.create_business(CreateBusinessDto { name: "Mi Empresa".into(), country_id: COLOMBIA }).await.unwrap();

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
        service.create_business(CreateBusinessDto { name: "Mi Empresa".into(), country_id: COLOMBIA }).await.unwrap();

        let dto = CreateAssociateDto {
            phone_number: "573003579384".into(),
            username: "stiven".into(),
            password: "Passw0rd".into(),
        };
        service.create_associate(1, CreateAssociateDto { phone_number: dto.phone_number.clone(), username: dto.username.clone(), password: dto.password.clone() }).await.unwrap();

        assert_eq!(service.create_associate(1, dto).await.unwrap_err(), "Username already exists");
    }

    fn associate(phone: &str, username: &str) -> CreateAssociateDto {
        CreateAssociateDto { phone_number: phone.into(), username: username.into(), password: "Passw0rd".into() }
    }

    #[tokio::test]
    async fn phone_of_another_associate_is_rejected_in_any_business() {
        let (service, repository) = build_service();
        service.create_business(CreateBusinessDto { name: "A".into(), country_id: COLOMBIA }).await.unwrap();
        service.create_business(CreateBusinessDto { name: "B".into(), country_id: COLOMBIA }).await.unwrap();
        service.create_associate(1, associate("573003579384", "ana")).await.unwrap();

        // Mismo número escrito de otra forma (sin indicativo), en otro business.
        let error = service.create_associate(2, associate("3003579384", "beto")).await.unwrap_err();

        assert_eq!(error, PHONE_TAKEN);
        assert_eq!(repository.associates.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn associate_phone_gets_country_prefix() {
        let (service, _) = build_service();
        service.create_business(CreateBusinessDto { name: "Co".into(), country_id: COLOMBIA }).await.unwrap();
        service.create_business(CreateBusinessDto { name: "Gt".into(), country_id: GUATEMALA }).await.unwrap();

        let co = service.create_associate(1, associate("+573003579384", "ana")).await.unwrap();
        let gt = service.create_associate(2, associate("51319585", "beto")).await.unwrap();

        assert_eq!(co.phone_number, "573003579384");
        assert_eq!(gt.phone_number, "50251319585");
    }

    #[tokio::test]
    async fn associate_phone_of_another_country_is_rejected() {
        let (service, _) = build_service();
        service.create_business(CreateBusinessDto { name: "Gt".into(), country_id: GUATEMALA }).await.unwrap();

        let error = service.create_associate(1, associate("3003579384", "ana")).await.unwrap_err();

        assert_eq!(error, "Phone number is not valid for Guatemala");
    }

    #[tokio::test]
    async fn business_requires_existing_country() {
        let (service, _) = build_service();

        let error = service.create_business(CreateBusinessDto { name: "X".into(), country_id: 99 }).await.unwrap_err();

        assert_eq!(error, "Country not found");
    }

    #[tokio::test]
    async fn business_includes_its_country() {
        let (service, _) = build_service();
        service.create_business(CreateBusinessDto { name: "Gt".into(), country_id: GUATEMALA }).await.unwrap();

        let business = service.get_business(1).await.unwrap();

        assert_eq!((business.country.phone_prefix.as_str(), business.country.phone_length), ("502", 8));
        let json = serde_json::to_value(&business).unwrap();
        assert_eq!(json["name"], "Gt");  // los campos del business van al nivel superior
        assert_eq!(json["country"]["code"], "GT");
    }
}
