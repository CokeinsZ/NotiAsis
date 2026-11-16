//! Un turno completo con el ensamblado real (`build_pipeline`) y Postgres:
//! solo el modelo, el transcriptor, la descarga de audio, el envío y Dropi
//! son falsos. Requiere `TEST_DATABASE_URL` (ver `tests/common`).

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ai_agents::agents::customer_service::domain::IncomingMessageEvent;
use ai_agents::agents::customer_service::turn::Turn;
use ai_agents::agents::customer_service::{CustomerServiceConfig, CustomerServiceDeps, build_pipeline};
use ai_agents::domain::ChatKey;
use ai_agents::integrations::backend::{AiMessageSender, BackendError, DownloadedMedia, MediaDownloader, SentMessage};
use ai_agents::integrations::dropi::{DropiError, OrderLookup, OrderStatus};
use ai_agents::llm::{
    AssistantMessage, ChatCompletion, ChatMessage, ChatModel, ChatRequest, FinishReason, LlmError, TokenUsage, ToolCall,
};
use ai_agents::pipeline::Outcome;
use ai_agents::transcription::{AudioClip, Transcriber, TranscriptionError, TranscriptionHints};
use async_trait::async_trait;
use chrono::NaiveDate;
use common::{pool, seed};

struct ScriptedModel {
    replies: Mutex<Vec<ChatCompletion>>,
    requests: Mutex<Vec<Vec<ChatMessage>>>,
}

impl ScriptedModel {
    fn new(mut replies: Vec<ChatCompletion>) -> Arc<Self> {
        replies.reverse();
        Arc::new(Self { replies: Mutex::new(replies), requests: Mutex::new(Vec::new()) })
    }
}

#[async_trait]
impl ChatModel for ScriptedModel {
    fn name(&self) -> &str {
        "scripted"
    }

    async fn complete(&self, request: ChatRequest<'_>) -> Result<ChatCompletion, LlmError> {
        self.requests.lock().unwrap().push(request.messages.to_vec());
        Ok(self.replies.lock().unwrap().pop().expect("unexpected model call"))
    }
}

fn final_text(text: &str) -> ChatCompletion {
    ChatCompletion {
        message: AssistantMessage { content: Some(text.into()), reasoning: Some("listo".into()), tool_calls: vec![] },
        finish_reason: FinishReason::Stop,
        usage: TokenUsage::default(),
    }
}

fn tool_call(name: &str, arguments: String) -> ChatCompletion {
    ChatCompletion {
        message: AssistantMessage {
            content: None,
            reasoning: Some("registro la fecha".into()),
            tool_calls: vec![ToolCall { id: "call_1".into(), name: name.into(), arguments }],
        },
        finish_reason: FinishReason::ToolCalls,
        usage: TokenUsage::default(),
    }
}

#[derive(Default)]
struct FakeTranscriber {
    hints: Mutex<Vec<TranscriptionHints>>,
}

#[async_trait]
impl Transcriber for FakeTranscriber {
    async fn transcribe(&self, clip: &AudioClip, hints: &TranscriptionHints) -> Result<String, TranscriptionError> {
        assert_eq!(clip.base_mime_type(), "audio/ogg");
        self.hints.lock().unwrap().push(hints.clone());
        Ok("Voy el lunes por el pedido".into())
    }
}

struct FakeMedia;

#[async_trait]
impl MediaDownloader for FakeMedia {
    async fn download(&self, _: &str) -> Result<DownloadedMedia, BackendError> {
        Ok(DownloadedMedia { bytes: b"OggS".to_vec(), mime_type: "audio/ogg; codecs=opus".into() })
    }
}

#[derive(Default)]
struct RecordingSender {
    sent: Mutex<Vec<(ChatKey, String)>>,
}

#[async_trait]
impl AiMessageSender for RecordingSender {
    async fn send_ai_message(&self, chat: &ChatKey, text: &str) -> Result<SentMessage, BackendError> {
        self.sent.lock().unwrap().push((chat.clone(), text.to_string()));
        Ok(SentMessage { id: 1, meta_message_id: "wamid.ai".into() })
    }
}

struct NoOrders;

#[async_trait]
impl OrderLookup for NoOrders {
    async fn order_status(&self, _: i32, _: &str) -> Result<Option<OrderStatus>, DropiError> {
        Ok(None)
    }
}

struct Harness {
    model: Arc<ScriptedModel>,
    transcriber: Arc<FakeTranscriber>,
    sender: Arc<RecordingSender>,
}

async fn run_turn(pool: sqlx::PgPool, event: IncomingMessageEvent, replies: Vec<ChatCompletion>) -> (Outcome, Harness) {
    let harness = Harness {
        model: ScriptedModel::new(replies),
        transcriber: Arc::new(FakeTranscriber::default()),
        sender: Arc::new(RecordingSender::default()),
    };
    let config = CustomerServiceConfig { quiet_period: Duration::ZERO, ..Default::default() };
    let pipeline = build_pipeline(
        &config,
        CustomerServiceDeps {
            pool,
            chat_model: harness.model.clone(),
            transcriber: harness.transcriber.clone(),
            media: Arc::new(FakeMedia),
            messages: harness.sender.clone(),
            orders: Arc::new(NoOrders),
        },
    );
    let outcome = pipeline.run(&mut Turn::new(event)).await.expect("turn failed");
    (outcome, harness)
}

#[tokio::test]
async fn full_turn_transcribes_uses_tools_and_replies() {
    let Some(pool) = pool().await else { return };
    let (chat, latest) = seed(&pool).await;
    let guide: String = sqlx::query_scalar("SELECT number FROM guides WHERE user_id = $1")
        .bind(&chat.user_phone)
        .fetch_one(&pool)
        .await
        .unwrap();
    let replies = vec![
        tool_call(
            "registrar_novedad",
            format!(r#"{{"numero_guia": "{guide}", "tipo": "will_pickup", "fecha_compromiso": "2026-11-16"}}"#),
        ),
        final_text("¡Perfecto, Carmen! Te esperamos el lunes."),
    ];

    let event = IncomingMessageEvent { message_id: latest, chat: chat.clone() };
    let (outcome, harness) = run_turn(pool.clone(), event, replies).await;

    assert_eq!(outcome, Outcome::Completed);
    assert_eq!(
        harness.sender.sent.lock().unwrap().as_slice(),
        [(chat.clone(), "¡Perfecto, Carmen! Te esperamos el lunes.".to_string())]
    );

    // El prompt lleva la persona, las políticas, la guía y el historial con
    // la nota de voz ya transcrita.
    let first_request = harness.model.requests.lock().unwrap()[0].clone();
    let [ChatMessage::System(system), ChatMessage::User(user)] = &first_request[..] else { panic!("system + user") };
    assert!(system.starts_with("Eres Verónica"));
    assert!(system.contains("Guardan el pedido 8 días."));
    assert!(system.contains(&guide));
    assert!(system.contains("$79.900"));
    assert!(user.contains("CLIENTE: [nota de voz] Voy el lunes por el pedido"), "{user}");
    assert!(user.contains("CLIENTE: Mañana paso"));
    // La ficha del producto de su guía va en el prompt.
    assert!(system.contains(r#""dosis": "2 cápsulas al día""#), "{system}");
    assert!(system.contains(r#""notas_para_ti": "Habla de duración, no de cápsulas.""#));
    assert!(!system.contains("Testosterone"), "only the customer's products go in the prompt");
    assert!(system.contains("(nada: no has escalado este chat ni registrado novedades)"));

    // La herramienta escribió en la base, y la transcripción quedó guardada.
    let promised: NaiveDate =
        sqlx::query_scalar("SELECT promised_date FROM guide_customer_updates WHERE guide_number = $1")
            .bind(&guide)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(promised, NaiveDate::from_ymd_opt(2026, 11, 16).unwrap());
    let transcribed: i64 =
        sqlx::query_scalar("SELECT count(*) FROM messages WHERE user_id = $1 AND transcription IS NOT NULL")
            .bind(&chat.user_phone)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(transcribed, 1);

    // El vocabulario del cliente llega a la transcripción.
    let hints = harness.transcriber.hints.lock().unwrap();
    assert!(hints[0].vocabulary.contains(&"EXTRACTO PIMIENTA DE CAYENA 3000 Mg".to_string()));
    assert!(hints[0].vocabulary.contains(&"Interrapidísimo".to_string()));
}

#[tokio::test]
async fn disabled_agent_never_calls_the_model() {
    let Some(pool) = pool().await else { return };
    let (chat, latest) = seed(&pool).await;
    sqlx::query("UPDATE business_ai_agents SET enabled = false WHERE business_id = $1")
        .bind(chat.business_id)
        .execute(&pool)
        .await
        .unwrap();

    let (outcome, harness) = run_turn(pool, IncomingMessageEvent { message_id: latest, chat }, vec![]).await;

    assert!(matches!(outcome, Outcome::Halted { step: "check_eligibility", .. }));
    assert!(harness.model.requests.lock().unwrap().is_empty());
    assert!(harness.sender.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn silent_reply_sends_nothing() {
    let Some(pool) = pool().await else { return };
    let (chat, latest) = seed(&pool).await;

    let (outcome, harness) =
        run_turn(pool, IncomingMessageEvent { message_id: latest, chat }, vec![final_text("[NO_RESPONDER]")]).await;

    assert!(matches!(outcome, Outcome::Halted { step: "generate_reply", .. }));
    assert!(harness.sender.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_older_message_lets_the_newest_turn_answer() {
    let Some(pool) = pool().await else { return };
    let (chat, latest) = seed(&pool).await;

    // Aviso del mensaje anterior (la nota de voz): el texto llegó después.
    let (outcome, harness) = run_turn(pool, IncomingMessageEvent { message_id: latest - 1, chat }, vec![]).await;

    assert!(
        matches!(outcome, Outcome::Halted { step: "check_eligibility", ref reason } if reason.starts_with("latest_message"))
    );
    assert!(harness.model.requests.lock().unwrap().is_empty());
}
