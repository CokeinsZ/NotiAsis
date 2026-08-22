use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};

use crate::auth::dtos::{
    API_KEY_TOKEN_HOURS, ASSOCIATE_TOKEN_MINUTES, ApiKeyLoginDto, Claims, LoginDto, TokenResponse,
};
use crate::auth::repository::AuthRepositoryTrait;

#[async_trait]
pub trait AuthServiceTrait: Send + Sync {
    /// Login de un business associate (webapp): token de 15 minutos.
    async fn login(&self, dto: LoginDto) -> Result<TokenResponse, String>;
    /// Login con api_key (bot): token de 24 horas.
    async fn login_with_api_key(&self, dto: ApiKeyLoginDto) -> Result<TokenResponse, String>;
    /// Valida un JWT y retorna sus claims.
    fn validate_token(&self, token: &str) -> Result<Claims, String>;
    /// Emite un token nuevo con los mismos claims (renovación).
    fn renew_token(&self, claims: &Claims) -> Result<TokenResponse, String>;
}

pub struct AuthService {
    repository: Arc<dyn AuthRepositoryTrait>,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl AuthService {
    pub fn new(repository: Arc<dyn AuthRepositoryTrait>, jwt_secret: String) -> Self {
        Self {
            repository,
            encoding_key: EncodingKey::from_secret(jwt_secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(jwt_secret.as_bytes()),
        }
    }

    fn issue_token(
        &self,
        sub: String,
        kind: &str,
        business_id: Option<i32>,
        phone_number: Option<String>,
        duration: Duration,
    ) -> Result<TokenResponse, String> {
        let now = Utc::now();
        let claims = Claims {
            sub,
            kind: kind.to_string(),
            business_id,
            phone_number,
            iat: now.timestamp() as usize,
            exp: (now + duration).timestamp() as usize,
        };

        let token = encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| e.to_string())?;

        Ok(TokenResponse {
            token,
            expires_in: duration.num_seconds(),
            business_id: claims.business_id,
            phone_number: claims.phone_number,
        })
    }
}

#[async_trait]
impl AuthServiceTrait for AuthService {
    async fn login(&self, dto: LoginDto) -> Result<TokenResponse, String> {
        let credentials = self.repository
            .find_associate_by_username(&dto.username)
            .await?
            .ok_or_else(|| "Invalid credentials".to_string())?;

        let valid = bcrypt::verify(&dto.password, &credentials.password_hash)
            .map_err(|e| e.to_string())?;
        if !valid {
            return Err("Invalid credentials".to_string());
        }

        self.issue_token(
            credentials.username,
            "associate",
            Some(credentials.business_id),
            Some(credentials.phone_number),
            Duration::minutes(ASSOCIATE_TOKEN_MINUTES),
        )
    }

    async fn login_with_api_key(&self, dto: ApiKeyLoginDto) -> Result<TokenResponse, String> {
        let key_id = self.repository
            .find_api_key_id(&dto.api_key)
            .await?
            .ok_or_else(|| "Invalid api key".to_string())?;

        self.issue_token(
            format!("api_key:{key_id}"),
            "api_key",
            None,
            None,
            Duration::hours(API_KEY_TOKEN_HOURS),
        )
    }

    fn validate_token(&self, token: &str) -> Result<Claims, String> {
        decode::<Claims>(token, &self.decoding_key, &Validation::default())
            .map(|data| data.claims)
            .map_err(|e| e.to_string())
    }

    fn renew_token(&self, claims: &Claims) -> Result<TokenResponse, String> {
        let duration = match claims.kind.as_str() {
            "associate" => Duration::minutes(ASSOCIATE_TOKEN_MINUTES),
            _ => Duration::hours(API_KEY_TOKEN_HOURS),
        };
        self.issue_token(
            claims.sub.clone(),
            &claims.kind,
            claims.business_id,
            claims.phone_number.clone(),
            duration,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::repository::AssociateCredentials;

    struct FakeAuthRepository {
        associate: Option<AssociateCredentials>,
        api_key_id: Option<i32>,
    }

    #[async_trait]
    impl AuthRepositoryTrait for FakeAuthRepository {
        async fn find_associate_by_username(&self, _: &str) -> Result<Option<AssociateCredentials>, String> {
            Ok(self.associate.clone())
        }

        async fn find_api_key_id(&self, _: &str) -> Result<Option<i32>, String> {
            Ok(self.api_key_id)
        }
    }

    fn build_service(with_associate: bool, api_key_id: Option<i32>) -> AuthService {
        let associate = if with_associate {
            Some(AssociateCredentials {
                username: "stiven".to_string(),
                phone_number: "573003579384".to_string(),
                business_id: 7,
                password_hash: bcrypt::hash("Passw0rd", 4).unwrap(),
            })
        } else {
            None
        };
        AuthService::new(
            Arc::new(FakeAuthRepository { associate, api_key_id }),
            "test-secret".to_string(),
        )
    }

    #[tokio::test]
    async fn login_ok_returns_associate_token_with_business_and_phone() {
        let service = build_service(true, None);
        let response = service
            .login(LoginDto { username: "stiven".into(), password: "Passw0rd".into() })
            .await
            .unwrap();

        assert_eq!(response.expires_in, 15 * 60);
        assert_eq!(response.business_id, Some(7));
        assert_eq!(response.phone_number.as_deref(), Some("573003579384"));

        let claims = service.validate_token(&response.token).unwrap();
        assert_eq!(claims.kind, "associate");
        assert_eq!(claims.business_id, Some(7));
        assert!(claims.exp > claims.iat);
    }

    #[tokio::test]
    async fn login_with_wrong_password_fails() {
        let service = build_service(true, None);
        let result = service
            .login(LoginDto { username: "stiven".into(), password: "Mala123".into() })
            .await;
        assert_eq!(result.unwrap_err(), "Invalid credentials");
    }

    #[tokio::test]
    async fn login_with_unknown_user_fails() {
        let service = build_service(false, None);
        let result = service
            .login(LoginDto { username: "nadie".into(), password: "Passw0rd".into() })
            .await;
        assert_eq!(result.unwrap_err(), "Invalid credentials");
    }

    #[tokio::test]
    async fn api_key_login_returns_24h_global_token() {
        let service = build_service(false, Some(3));
        let response = service
            .login_with_api_key(ApiKeyLoginDto { api_key: "secret-key".into() })
            .await
            .unwrap();

        assert_eq!(response.expires_in, 24 * 3600);
        let claims = service.validate_token(&response.token).unwrap();
        assert_eq!(claims.kind, "api_key");
        assert_eq!(claims.business_id, None);
        assert!(claims.can_access_business(123));
    }

    #[tokio::test]
    async fn invalid_api_key_fails() {
        let service = build_service(false, None);
        let result = service
            .login_with_api_key(ApiKeyLoginDto { api_key: "mala".into() })
            .await;
        assert_eq!(result.unwrap_err(), "Invalid api key");
    }

    #[tokio::test]
    async fn validate_rejects_token_signed_with_other_secret() {
        let service = build_service(true, None);
        let response = service
            .login(LoginDto { username: "stiven".into(), password: "Passw0rd".into() })
            .await
            .unwrap();

        let other = AuthService::new(
            Arc::new(FakeAuthRepository { associate: None, api_key_id: None }),
            "otro-secret".to_string(),
        );
        assert!(other.validate_token(&response.token).is_err());
    }

    #[tokio::test]
    async fn renew_keeps_claims_and_resets_expiration() {
        let service = build_service(true, None);
        let original = service
            .login(LoginDto { username: "stiven".into(), password: "Passw0rd".into() })
            .await
            .unwrap();

        let claims = service.validate_token(&original.token).unwrap();
        let renewed = service.renew_token(&claims).unwrap();
        let renewed_claims = service.validate_token(&renewed.token).unwrap();

        assert_eq!(renewed_claims.sub, claims.sub);
        assert_eq!(renewed_claims.business_id, claims.business_id);
        assert_eq!(renewed_claims.phone_number, claims.phone_number);
        assert!(renewed_claims.exp >= claims.exp);
        assert_eq!(renewed.expires_in, 15 * 60);
    }
}
