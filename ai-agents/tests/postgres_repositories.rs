//! Repositorios contra un Postgres real con el esquema de `db.sql`.
//!
//! Corren solo si `TEST_DATABASE_URL` está definida (por ejemplo, con
//! `docker run -e POSTGRES_PASSWORD=test -p 55432:5432 postgres:17` y
//! `TEST_DATABASE_URL=postgres://postgres:test@localhost:55432/postgres`).
//! Cada test siembra su propio business y cliente, así no se pisan.

use ai_agents::agents::customer_service::CustomerServiceConfig;
use ai_agents::agents::customer_service::domain::{
    Author, Escalation, EscalationCategory, GuideStage, GuideUpdate, GuideUpdateKind, IncomingMessageEvent, MediaKind,
};
use ai_agents::agents::customer_service::eligibility::EligibilityGate;
use ai_agents::agents::customer_service::policy::{ReplyPolicy, Verdict};
use ai_agents::agents::customer_service::ports::{
    ChatStateReader, CustomerContextReader, EscalationWriter, GuideReader, GuideUpdateWriter, ProductCatalog,
    TranscriptReader, VoiceNoteStore,
};
use ai_agents::agents::customer_service::repositories::{
    PgChatStateReader, PgCustomerContextReader, PgEscalationWriter, PgGuideReader, PgGuideUpdateWriter,
    PgProductCatalog, PgTranscriptReader, PgVoiceNoteStore,
};
use ai_agents::domain::ChatKey;
use chrono::{NaiveDate, Timelike};
use common::{pool, seed};
use std::sync::Arc;

mod common;

#[tokio::test]
async fn snapshot_and_policy_allow_a_fresh_message_and_stop_after_a_human_writes() {
    let Some(pool) = pool().await else { return };
    let (chat, latest) = seed(&pool).await;
    let event = IncomingMessageEvent { message_id: latest, chat: chat.clone() };
    let state = Arc::new(PgChatStateReader::new(pool.clone()));

    let snapshot = state.snapshot(&chat).await.unwrap().expect("chat exists");
    assert_eq!(snapshot.latest_incoming.unwrap().id, latest);
    assert!(snapshot.agent.unwrap().enabled);
    assert_eq!(snapshot.agent.unwrap().human_takeover_hours, 8);
    assert_eq!(snapshot.ai_replies_last_hour, 0);

    let gate = EligibilityGate::new(state.clone(), ReplyPolicy::standard(&CustomerServiceConfig::default()));
    assert_eq!(gate.verdict(&event).await.unwrap(), Verdict::Reply);

    // Un asesor escribe desde la webapp: el agente se calla.
    sqlx::query(
        "INSERT INTO messages (meta_message_id, business_id, user_id, media_type, message, from_user, sender)
         VALUES ($1, $2, $3, 'text', 'Hola, te ayudo yo', false, 'associate')",
    )
    .bind(format!("wamid.h{latest}"))
    .bind(chat.business_id)
    .bind(&chat.user_phone)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(gate.verdict(&event).await.unwrap(), Verdict::Skip { rule: "human_takeover", .. }));

    // Un chat que no existe.
    let missing = IncomingMessageEvent { message_id: 1, chat: ChatKey::new(chat.business_id, "570000000000") };
    assert!(matches!(gate.verdict(&missing).await.unwrap(), Verdict::Skip { rule: "chat_exists", .. }));
}

#[tokio::test]
async fn customer_context_has_business_persona_and_guides_in_local_time() {
    let Some(pool) = pool().await else { return };
    let (chat, _) = seed(&pool).await;

    let context = PgCustomerContextReader::new(pool).load(&chat, 10).await.unwrap().expect("context");
    assert_eq!(context.business.name, "Victor Store");
    assert_eq!(context.business.timezone, "America/Bogota");
    assert_eq!(context.persona.name, "Verónica");
    assert_eq!(context.persona.policies, "Guardan el pedido 8 días.");
    assert_eq!(context.customer_name, "Carmen B");

    let guide = &context.guides[0];
    assert_eq!(guide.stage, GuideStage::AtOffice);
    assert_eq!(guide.office_notifications, 3);
    assert_eq!(guide.products[0].name, "EXTRACTO PIMIENTA DE CAYENA 3000 Mg");
    assert_eq!(guide.products[0].quantity, 2);
    assert_eq!(guide.order_total, Some(79_900.0));
    assert_eq!(guide.at_office_since, NaiveDate::from_ymd_opt(2026, 11, 6));
    assert!(context.vocabulary().contains(&"INTERRAPIDISIMO".to_string()));

    // Solo la ficha enlazada al producto de su guía, con los datos que hay.
    let names: Vec<&str> = context.products.iter().map(|product| product.name.as_str()).collect();
    assert_eq!(names, ["Pimienta de Cayena (Cayenne Pepper)"]);
    assert_eq!(context.products[0].dosage.as_deref(), Some("2 cápsulas al día"));
    assert_eq!(context.products[0].ingredients, None);
    assert!(context.vocabulary().contains(&"Pimienta de Cayena (Cayenne Pepper)".to_string()));
}

#[tokio::test]
async fn catalog_has_the_active_products_of_its_business_only() {
    let Some(pool) = pool().await else { return };
    let (chat, _) = seed(&pool).await;
    let (other_chat, _) = seed(&pool).await;
    sqlx::query("UPDATE products SET name = 'Otro negocio' WHERE business_id = $1 AND name LIKE 'Testosterone%'")
        .bind(other_chat.business_id)
        .execute(&pool)
        .await
        .unwrap();

    let catalog = PgProductCatalog::new(pool.clone());
    let names: Vec<String> =
        catalog.products(chat.business_id).await.unwrap().into_iter().map(|product| product.name).collect();
    assert_eq!(names, ["Pimienta de Cayena (Cayenne Pepper)", "Testosterone+ Advanced 15 en 1"]);

    // Un id de Dropi solo se enlaza a un producto del mismo business.
    let foreign_product: i32 =
        sqlx::query_scalar("SELECT id FROM products WHERE business_id = $1 AND name = 'Otro negocio'")
            .bind(other_chat.business_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let cross_link = sqlx::query(
        "INSERT INTO product_dropi_ids (business_id, dropi_product_id, product_id) VALUES ($1, 2311534, $2)",
    )
    .bind(chat.business_id)
    .bind(foreign_product)
    .execute(&pool)
    .await;
    assert!(cross_link.is_err(), "a Dropi id must link to a product of the same business");
}

#[tokio::test]
async fn transcript_is_chronological_with_authors_and_local_times() {
    let Some(pool) = pool().await else { return };
    let (chat, _) = seed(&pool).await;

    let reader = PgTranscriptReader::new(pool);
    let transcript = reader.recent(&chat, 40).await.unwrap();
    let authors: Vec<Author> = transcript.iter().map(|entry| entry.author).collect();
    assert_eq!(authors, vec![Author::Notification, Author::Notification, Author::Customer, Author::Customer]);
    assert_eq!(transcript[0].media, MediaKind::Video);
    assert_eq!(transcript[0].at.hour(), 13, "18:06 UTC is 13:06 in Bogotá");
    assert_eq!(transcript[1].guide_number.as_deref().map(|g| g.starts_with('G')), Some(true));
    assert_eq!(transcript[2].media, MediaKind::Audio);
    assert_eq!(transcript[3].text.as_deref(), Some("Mañana paso"));

    // Con límite trae los más recientes, en orden cronológico.
    let last_two = reader.recent(&chat, 2).await.unwrap();
    assert_eq!(last_two.len(), 2);
    assert_eq!(last_two[1].text.as_deref(), Some("Mañana paso"));
}

#[tokio::test]
async fn a_reply_carries_the_message_it_quotes() {
    let Some(pool) = pool().await else { return };
    let (chat, _) = seed(&pool).await;
    // El cliente responde citando el aviso de oficina (de días antes).
    sqlx::query(
        "INSERT INTO messages (meta_message_id, business_id, user_id, media_type, message, from_user, created_at, \
                               reply_to_message_id)
         SELECT 'wamid.q' || id, business_id, user_id, 'text', 'Hoy voy', true, now() AT TIME ZONE 'UTC', id
         FROM messages WHERE business_id = $1 AND user_id = $2 AND sender = 'bot' AND media_type = 'text'",
    )
    .bind(chat.business_id)
    .bind(&chat.user_phone)
    .execute(&pool)
    .await
    .unwrap();

    let transcript = PgTranscriptReader::new(pool).recent(&chat, 40).await.unwrap();
    let reply = transcript.last().unwrap();
    assert_eq!(reply.text.as_deref(), Some("Hoy voy"));
    let quoted = reply.quoted.as_ref().expect("quoted message");
    assert_eq!(quoted.author, Author::Notification);
    assert_eq!(quoted.text.as_deref(), Some("Es hora de recoger tu pedido"));
    assert_eq!(quoted.at.hour(), 13, "local time of the quoted message");
    assert!(transcript[0].quoted.is_none());
}

#[tokio::test]
async fn context_has_past_escalations_and_updates_in_order() {
    let Some(pool) = pool().await else { return };
    let (chat, _) = seed(&pool).await;
    let guide: String = sqlx::query_scalar("SELECT number FROM guides WHERE user_id = $1")
        .bind(&chat.user_phone)
        .fetch_one(&pool)
        .await
        .unwrap();
    let writer = PgGuideUpdateWriter::new(pool.clone());
    let update = GuideUpdate {
        guide_number: guide.clone(),
        kind: GuideUpdateKind::WillPickup,
        promised_date: NaiveDate::from_ymd_opt(2026, 11, 16),
        note: Some("cobra el lunes".into()),
    };
    assert!(writer.record(&chat, &update).await.unwrap());
    let escalation = Escalation {
        category: EscalationCategory::ShippingIssue,
        reason: "Quiere recibirlo en casa".into(),
        guide_number: Some(guide.clone()),
    };
    PgEscalationWriter::new(pool.clone()).escalate(&chat, &escalation).await.unwrap();

    let reader = PgCustomerContextReader::new(pool.clone());
    let history = reader.load(&chat, 10).await.unwrap().unwrap().history;
    assert_eq!(history.updates.len(), 1);
    assert_eq!(history.updates[0].update, update);
    assert_eq!(history.escalations.len(), 1);
    assert_eq!(history.escalations[0].escalation, escalation);
    assert!(!history.escalations[0].answered);

    // Un asesor responde: el escalamiento queda atendido.
    sqlx::query(
        "INSERT INTO messages (meta_message_id, business_id, user_id, media_type, message, from_user, sender) \
         VALUES ('wamid.asesor' || $2, $1, $2, 'text', 'Ya lo reviso', false, 'associate')",
    )
    .bind(chat.business_id)
    .bind(&chat.user_phone)
    .execute(&pool)
    .await
    .unwrap();
    let history = reader.load(&chat, 10).await.unwrap().unwrap().history;
    assert!(history.escalations[0].answered);
}

#[tokio::test]
async fn voice_notes_are_pending_until_transcribed() {
    let Some(pool) = pool().await else { return };
    let (chat, _) = seed(&pool).await;
    let store = PgVoiceNoteStore::new(pool.clone());

    let pending = store.pending(&chat, 5).await.unwrap();
    assert_eq!(pending.len(), 1);
    store.save_transcription(pending[0].message_id, "Voy el lunes por el pedido").await.unwrap();
    assert!(store.pending(&chat, 5).await.unwrap().is_empty());

    let transcript = PgTranscriptReader::new(pool).recent(&chat, 40).await.unwrap();
    assert_eq!(transcript[2].transcription.as_deref(), Some("Voy el lunes por el pedido"));
}

#[tokio::test]
async fn guides_are_only_visible_and_writable_by_their_owner() {
    let Some(pool) = pool().await else { return };
    let (chat, _) = seed(&pool).await;
    let (other_chat, _) = seed(&pool).await;
    let guide_number: String = sqlx::query_scalar("SELECT number FROM guides WHERE user_id = $1")
        .bind(&chat.user_phone)
        .fetch_one(&pool)
        .await
        .unwrap();

    let reader = PgGuideReader::new(pool.clone());
    let details = reader.details(&chat, &guide_number).await.unwrap().expect("own guide");
    assert_eq!(details.order_id.as_deref(), Some("95204087"));
    assert_eq!(details.timeline.len(), 3);
    assert_eq!(details.timeline[2].label(), "Recordatorio final: riesgo de devolución");
    assert!(reader.details(&other_chat, &guide_number).await.unwrap().is_none(), "another customer's guide");

    let writer = PgGuideUpdateWriter::new(pool.clone());
    let update = GuideUpdate {
        guide_number: guide_number.clone(),
        kind: GuideUpdateKind::WillPickup,
        promised_date: NaiveDate::from_ymd_opt(2026, 11, 16),
        note: Some("espera la quincena".into()),
    };
    assert!(writer.record(&chat, &update).await.unwrap());
    assert!(!writer.record(&other_chat, &update).await.unwrap());
    let kind: String = sqlx::query_scalar("SELECT kind FROM guide_customer_updates WHERE guide_number = $1")
        .bind(&guide_number)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(kind, "will_pickup");
}

#[tokio::test]
async fn escalation_marks_the_chat_and_silences_the_agent() {
    let Some(pool) = pool().await else { return };
    let (chat, latest) = seed(&pool).await;
    let (other_chat, _) = seed(&pool).await;
    let other_guide: String = sqlx::query_scalar("SELECT number FROM guides WHERE user_id = $1")
        .bind(&other_chat.user_phone)
        .fetch_one(&pool)
        .await
        .unwrap();

    PgEscalationWriter::new(pool.clone())
        .escalate(
            &chat,
            &Escalation {
                category: EscalationCategory::Cancellation,
                reason: "Quiere cancelar".into(),
                // Guía de otro cliente: se guarda NULL.
                guide_number: Some(other_guide),
            },
        )
        .await
        .unwrap();

    let (important, guide): (bool, Option<String>) = sqlx::query_as(
        "SELECT c.is_important, e.guide_number FROM chats c
         JOIN ai_escalations e ON e.business_id = c.business_id AND e.user_id = c.user_id
         WHERE c.business_id = $1 AND c.user_id = $2",
    )
    .bind(chat.business_id)
    .bind(&chat.user_phone)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(important);
    assert_eq!(guide, None);

    let gate = EligibilityGate::new(
        Arc::new(PgChatStateReader::new(pool)),
        ReplyPolicy::standard(&CustomerServiceConfig::default()),
    );
    let event = IncomingMessageEvent { message_id: latest, chat };
    assert!(matches!(gate.verdict(&event).await.unwrap(), Verdict::Skip { rule: "open_escalation", .. }));
}
