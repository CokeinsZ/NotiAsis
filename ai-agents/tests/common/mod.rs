//! Base de datos de prueba compartida por los tests de integración.
//!
//! Solo corren si `TEST_DATABASE_URL` está definida; el esquema sale de
//! `webapp/backend/db.sql` y se aplica una vez por binario de tests.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU32, Ordering};

use ai_agents::domain::ChatKey;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::OnceCell;

const SCHEMA: &str = include_str!("../../../webapp/backend/db.sql");

static SCHEMA_READY: OnceCell<()> = OnceCell::const_new();
static NEXT_PHONE: AtomicU32 = AtomicU32::new(0);

pub async fn pool() -> Option<PgPool> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    SCHEMA_READY
        .get_or_init(|| async {
            let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("connect");
            sqlx::raw_sql(SCHEMA).execute(&pool).await.expect("apply db.sql");
        })
        .await;
    Some(PgPoolOptions::new().max_connections(4).connect(&url).await.expect("connect"))
}

/// Business colombiano con un chat, una guía en oficina y su historial:
/// 3 avisos de oficina (UTC 18:06 = 13:06 en Bogotá), un video, una nota de
/// voz y un texto del cliente. El agente está activado. Catálogo: la
/// Pimienta de Cayena (enlazada al id de Dropi de la guía), un producto
/// sin enlace y uno inactivo.
pub async fn seed(pool: &PgPool) -> (ChatKey, i32) {
    // Único entre corridas (microsegundos) y entre tests de una corrida (contador).
    let micros = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_micros();
    let suffix = ((micros + u128::from(NEXT_PHONE.fetch_add(1, Ordering::SeqCst)) * 7919) % 100_000_000) as u32;
    let phone = format!("5730{suffix:08}");
    let guide = format!("G{suffix:08}");

    let business_id: i32 = sqlx::query_scalar(
        "INSERT INTO businesses (name, country_id) SELECT 'Victor Store', id FROM countries WHERE code = 'CO' RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let script = format!(
        r#"
        INSERT INTO business_ai_agents (business_id, enabled, policies) VALUES ({business_id}, true, 'Guardan el pedido 8 días.');
        INSERT INTO users (phone_number, full_name) VALUES ('{phone}', 'Carmen B');
        INSERT INTO chats (business_id, user_id, last_user_message_timestamp)
            VALUES ({business_id}, '{phone}', (now() AT TIME ZONE 'UTC') - interval '5 minutes');
        INSERT INTO guides (number, user_id, business_id, order_id, customer_name, delivery_address, city,
                            shipping_company, order_total, currency, notification_count, last_notification_timestamp)
            VALUES ('{guide}', '{phone}', {business_id}, '95204087', 'Carmen Buitrago', 'Oficina Interrapidisimo',
                    'SILVANIA', 'INTERRAPIDISIMO', 79900, 'COP', 3, '2026-11-08 18:06:00');
        INSERT INTO guide_products (guide_number, product_id, product_name, quantity)
            VALUES ('{guide}', 2240718, 'EXTRACTO PIMIENTA DE CAYENA 3000 Mg', 2);
        WITH cayena AS (
            INSERT INTO products (business_id, name, dosage, treatment_duration, agent_notes)
            VALUES ({business_id}, 'Pimienta de Cayena (Cayenne Pepper)', '2 cápsulas al día',
                    '1 frasco: 2 meses', 'Habla de duración, no de cápsulas.')
            RETURNING id
        )
        INSERT INTO product_dropi_ids (business_id, dropi_product_id, product_id)
            SELECT {business_id}, 2240718, id FROM cayena;
        INSERT INTO products (business_id, name, description) VALUES
            ({business_id}, 'Testosterone+ Advanced 15 en 1', 'Suplemento para hombre.');
        INSERT INTO products (business_id, name, active) VALUES ({business_id}, 'Cayena descontinuada', false);
        INSERT INTO guide_notifications (guide_number, business_id, user_id, notification_step, kind, created_at) VALUES
            ('{guide}', {business_id}, '{phone}', 1, 'office', '2026-11-06 18:06:00'),
            ('{guide}', {business_id}, '{phone}', 2, 'office', '2026-11-07 18:06:00'),
            ('{guide}', {business_id}, '{phone}', 3, 'office', '2026-11-08 18:06:00');
        INSERT INTO messages (meta_message_id, business_id, user_id, media_type, message, status, from_user, created_at, guide_number, sender) VALUES
            ('wamid.v{suffix}', {business_id}, '{phone}', 'video', NULL, 'read', false, '2026-11-06 18:06:00', NULL, 'bot'),
            ('wamid.n{suffix}', {business_id}, '{phone}', 'text', 'Es hora de recoger tu pedido', 'read', false, '2026-11-06 18:06:01', '{guide}', 'bot');
        INSERT INTO messages (meta_message_id, business_id, user_id, media_type, media_id, from_user, created_at) VALUES
            ('wamid.a{suffix}', {business_id}, '{phone}', 'audio', 'media-{suffix}', true, (now() AT TIME ZONE 'UTC') - interval '6 minutes');
        INSERT INTO messages (meta_message_id, business_id, user_id, media_type, message, from_user, created_at) VALUES
            ('wamid.t{suffix}', {business_id}, '{phone}', 'text', 'Mañana paso', true, (now() AT TIME ZONE 'UTC') - interval '5 minutes');
        "#
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(script)).execute(pool).await.unwrap();
    let latest: i32 = sqlx::query_scalar("SELECT max(id) FROM messages WHERE user_id = $1 AND from_user")
        .bind(&phone)
        .fetch_one(pool)
        .await
        .unwrap();
    (ChatKey::new(business_id, phone), latest)
}
