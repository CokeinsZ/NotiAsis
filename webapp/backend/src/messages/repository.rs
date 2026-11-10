use async_trait::async_trait;
use chrono::NaiveDateTime;

use crate::messages::dtos::{
    MediaType, Message, MessageFailure, MessageStatus, RegisterMessageErrorDto, UndoneNotification,
};
use crate::tools::message_lock::lock_message;

/// Nivel de seguimiento tras borrar una notificación fallida: el más alto
/// que sí se notificó (0 si ninguno). El job lo vuelve a notificar.
const REVERT_TRACKING_LEVEL: &str = r#"
    UPDATE guides SET
        tracking_level = COALESCE((
            SELECT MAX(notification_step) FROM guide_notifications
            WHERE guide_number = $1 AND kind = 'tracking'
        ), 0),
        tracking_notified_at = (
            SELECT MAX(created_at) FROM guide_notifications
            WHERE guide_number = $1 AND kind = 'tracking'
        )
    WHERE number = $1
"#;

/// Paso de oficina tras borrar una notificación fallida: una menos, y la
/// fecha de la última que sí se notificó (NULL si no queda ninguna), para
/// que la regla de "una por día" no frene el reintento.
const REVERT_OFFICE_STEP: &str = r#"
    UPDATE guides SET
        notification_count = GREATEST(notification_count - 1, 0),
        last_notification_timestamp = (
            SELECT MAX(created_at) FROM guide_notifications
            WHERE guide_number = $1 AND kind = 'office'
        )
    WHERE number = $1
"#;

/// Datos necesarios para persistir un mensaje nuevo.
pub struct NewMessage {
    pub meta_message_id: String,
    pub business_id: i32,
    pub user_id: String,
    pub media_id: Option<String>,
    pub media_type: MediaType,
    pub message: Option<String>,
    pub status: Option<MessageStatus>,
    pub from_user: bool,
    /// Si es None, la base de datos usa CURRENT_TIMESTAMP.
    pub created_at: Option<NaiveDateTime>,
    pub guide_number: Option<String>,
}


#[async_trait]
pub trait MessageRepositoryTrait: Send + Sync {
    /// Guarda el mensaje. Si el meta_message_id ya existe (Meta puede
    /// reenviar webhooks), retorna el mensaje ya guardado. Un saliente
    /// cuyo error Meta ya reportó nace 'failed'.
    async fn save_message(&self, message: &NewMessage) -> Result<Message, String>;
    async fn get_messages_by_chat(&self, business_id: i32, user_id: &str) -> Result<Vec<Message>, String>;
    /// Actualiza el estado de un mensaje saliente. Nunca retrocede un
    /// mensaje ya marcado como 'read' ni cambia uno 'failed'. Retorna si
    /// se actualizó algo.
    async fn update_status_by_meta_id(&self, meta_message_id: &str, status: MessageStatus) -> Result<bool, String>;
    /// Registra el error que Meta reportó de un mensaje: lo guarda en
    /// messages_errors, deja el mensaje en 'failed' y deshace la
    /// notificación de guía ligada a él (la guía vuelve al paso o nivel
    /// anterior y se reintenta en la próxima corrida del job). Idempotente.
    async fn register_failure(
        &self,
        meta_message_id: &str,
        error: &RegisterMessageErrorDto,
    ) -> Result<MessageFailure, String>;
    async fn find_by_meta_id(&self, meta_message_id: &str) -> Result<Option<Message>, String>;
    /// Si algún mensaje del business tiene ese media_id (para autorizar
    /// su descarga).
    async fn media_belongs_to_business(&self, media_id: &str, business_id: i32) -> Result<bool, String>;
}

pub struct PostgresMessageRepository {
    db_pool: sqlx::PgPool,
}

impl PostgresMessageRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { db_pool: pool }
    }
}

#[async_trait]
impl MessageRepositoryTrait for PostgresMessageRepository {
    async fn save_message(&self, message: &NewMessage) -> Result<Message, String> {
        // El webhook del error puede llegar antes que el registro del
        // mensaje: con el bloqueo, o el error ya está y el mensaje nace
        // 'failed', o el error llega después y lo encuentra.
        let query = r#"
            INSERT INTO messages (
                meta_message_id, business_id, user_id,
                media_id, media_type, message, status, from_user, created_at, guide_number
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                CASE
                    WHEN $7::message_status_enum IS NOT NULL
                         AND EXISTS (SELECT 1 FROM messages_errors WHERE meta_message_id = $1)
                    THEN 'failed'::message_status_enum
                    ELSE $7::message_status_enum
                END,
                $8, COALESCE($9::timestamp, CURRENT_TIMESTAMP), $10
            )
            ON CONFLICT (meta_message_id) DO NOTHING
            RETURNING id, meta_message_id, business_id, user_id,
                      media_id, media_type, message, status, from_user, created_at, guide_number
        "#;

        let mut tx = self.db_pool.begin().await.map_err(|e| e.to_string())?;
        lock_message(&mut tx, &message.meta_message_id).await?;

        let inserted: Option<Message> = sqlx::query_as(query)
            .bind(&message.meta_message_id)
            .bind(message.business_id)
            .bind(&message.user_id)
            .bind(&message.media_id)
            .bind(message.media_type)
            .bind(&message.message)
            .bind(message.status)
            .bind(message.from_user)
            .bind(message.created_at)
            .bind(&message.guide_number)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        tx.commit().await.map_err(|e| e.to_string())?;

        if let Some(row) = inserted {
            return Ok(row);
        }

        // Webhook duplicado: el mensaje ya existía.
        self.find_by_meta_id(&message.meta_message_id)
            .await?
            .ok_or_else(|| format!("Message {} not found after conflict", message.meta_message_id))
    }

    async fn get_messages_by_chat(&self, business_id: i32, user_id: &str) -> Result<Vec<Message>, String> {
        let query = r#"
            SELECT id, meta_message_id, business_id, user_id,
                   media_id, media_type, message, status, from_user, created_at, guide_number
            FROM messages
            WHERE business_id = $1 AND user_id = $2
            ORDER BY created_at ASC, id ASC
        "#;

        sqlx::query_as::<_, Message>(query)
            .bind(business_id)
            .bind(user_id)
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn update_status_by_meta_id(&self, meta_message_id: &str, status: MessageStatus) -> Result<bool, String> {
        let query = r#"
            UPDATE messages SET status = $1
            WHERE meta_message_id = $2 AND status NOT IN ('read', 'failed')
        "#;

        let result = sqlx::query(query)
            .bind(status)
            .bind(meta_message_id)
            .execute(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(result.rows_affected() > 0)
    }

    async fn register_failure(
        &self,
        meta_message_id: &str,
        error: &RegisterMessageErrorDto,
    ) -> Result<MessageFailure, String> {
        let payload = serde_json::to_string(&error.payload).map_err(|e| e.to_string())?;

        let mut tx = self.db_pool.begin().await.map_err(|e| e.to_string())?;
        lock_message(&mut tx, meta_message_id).await?;

        // Meta puede reenviar el mismo webhook: un error por mensaje.
        sqlx::query(
            r#"
            INSERT INTO messages_errors (meta_message_id, error_code, error_title, payload)
            VALUES ($1, $2, $3, $4::jsonb)
            ON CONFLICT (meta_message_id) DO NOTHING
            "#,
        )
        .bind(meta_message_id)
        .bind(error.error_code)
        .bind(&error.error_title)
        .bind(payload)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        let (message_found,): (bool,) = sqlx::query_as(
            r#"
            WITH updated AS (
                UPDATE messages SET status = 'failed'
                WHERE meta_message_id = $1
                RETURNING id
            )
            SELECT EXISTS(SELECT 1 FROM updated)
            "#,
        )
        .bind(meta_message_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        // Si el bot ya marcó la notificación de la guía con este mensaje,
        // se borra (no se entregó: no cuenta en el dashboard) y la guía
        // vuelve al paso o nivel anterior.
        let undone: Vec<UndoneNotification> = sqlx::query_as(
            r#"
            DELETE FROM guide_notifications
            WHERE meta_message_id = $1
            RETURNING guide_number, kind, notification_step
            "#,
        )
        .bind(meta_message_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        for notification in &undone {
            let Some(guide_number) = &notification.guide_number else {
                continue;
            };
            let revert = if notification.kind == "tracking" { REVERT_TRACKING_LEVEL } else { REVERT_OFFICE_STEP };
            sqlx::query(revert)
                .bind(guide_number)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        }

        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(MessageFailure { message_found, undone })
    }

    async fn find_by_meta_id(&self, meta_message_id: &str) -> Result<Option<Message>, String> {
        let query = r#"
            SELECT id, meta_message_id, business_id, user_id,
                   media_id, media_type, message, status, from_user, created_at, guide_number
            FROM messages WHERE meta_message_id = $1
        "#;

        sqlx::query_as::<_, Message>(query)
            .bind(meta_message_id)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn media_belongs_to_business(&self, media_id: &str, business_id: i32) -> Result<bool, String> {
        let query = r#"
            SELECT EXISTS(SELECT 1 FROM messages WHERE media_id = $1 AND business_id = $2)
        "#;

        let result: (bool,) = sqlx::query_as(query)
            .bind(media_id)
            .bind(business_id)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(result.0)
    }
}
