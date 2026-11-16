//! El bucle del agente contra un servidor que imita la regla de DeepSeek:
//! con herramientas y thinking activo, cada mensaje previo del asistente
//! debe traer su `reasoning_content`; si falta, 400.

use std::sync::{Arc, Mutex};

use ai_agents::agent::{AgentRunner, Conversation, ToolBox, ToolError, TypedTool};
use ai_agents::llm::deepseek::{DeepSeekChatModel, DeepSeekConfig};
use ai_agents::shared::retry::RetryPolicy;
use async_trait::async_trait;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

type Requests = Arc<Mutex<Vec<Value>>>;

const MISSING_REASONING: &str = "The `reasoning_content` in the thinking mode must be passed back to the API.";

/// Imita a DeepSeek: 1.ª petición → llamada a herramienta; 2.ª → texto final.
async fn fake_deepseek(State(requests): State<Requests>, Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
    let missing_reasoning = body.get("tools").is_some()
        && body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["role"] == "assistant" && message.get("reasoning_content").is_none());
    if missing_reasoning {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": { "message": MISSING_REASONING } })));
    }

    let call_number = {
        let mut requests = requests.lock().unwrap();
        requests.push(body);
        requests.len()
    };
    let message = if call_number == 1 {
        json!({
            "role": "assistant",
            "content": "",
            "reasoning_content": "El cliente pregunta por su pedido: consulto la guía.",
            "tool_calls": [{"id": "call_00_abc", "type": "function",
                            "function": {"name": "detalle_guia", "arguments": "{\"numero_guia\": \"240064842467\"}"}}]
        })
    } else {
        json!({"role": "assistant", "content": "Tu pedido está en la oficina de Silvania.",
               "reasoning_content": "Ya tengo el dato."})
    };
    let finish_reason = if call_number == 1 { "tool_calls" } else { "stop" };
    (
        StatusCode::OK,
        Json(json!({
            "choices": [{"index": 0, "message": message, "finish_reason": finish_reason}],
            "usage": {"prompt_tokens": 50, "completion_tokens": 10, "prompt_cache_hit_tokens": 0,
                      "completion_tokens_details": {"reasoning_tokens": 8}}
        })),
    )
}

async fn start_fake_deepseek() -> (String, Requests) {
    let requests = Requests::default();
    let app = Router::new().route("/chat/completions", post(fake_deepseek)).with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), requests)
}

#[derive(Deserialize, JsonSchema)]
struct GuideArgs {
    /// Número de guía.
    numero_guia: String,
}

struct FakeGuideTool;

#[async_trait]
impl TypedTool<()> for FakeGuideTool {
    type Args = GuideArgs;
    const NAME: &'static str = "detalle_guia";
    const DESCRIPTION: &'static str = "Detalle de una guía.";

    async fn call(&self, _: &(), args: GuideArgs) -> Result<Value, ToolError> {
        Ok(json!({ "guia": args.numero_guia, "ciudad": "SILVANIA" }))
    }
}

#[tokio::test]
async fn reasoning_is_passed_back_so_the_tool_loop_succeeds() {
    let (base_url, requests) = start_fake_deepseek().await;
    let mut config = DeepSeekConfig::new("test-key");
    config.base_url = base_url;
    config.retry = RetryPolicy::no_retries();
    let model = Arc::new(DeepSeekChatModel::new(config).unwrap());
    let runner = AgentRunner::new(model, ToolBox::new().with(FakeGuideTool), 4);

    let run = runner.run(&(), Conversation::new("sistema", "¿Dónde está mi pedido?")).await.unwrap();
    assert_eq!(run.reply.as_deref(), Some("Tu pedido está en la oficina de Silvania."));
    assert_eq!(run.usage.reasoning_tokens, 16);

    let requests = requests.lock().unwrap();
    let first = &requests[0];
    assert_eq!(first["model"], "deepseek-flash");
    assert_eq!(first["thinking"], json!({"type": "enabled"}));
    assert_eq!(first["reasoning_effort"], "max");
    assert_eq!(first["tool_choice"], "auto");

    let second = &requests[1]["messages"];
    assert_eq!(second[2]["role"], "assistant");
    assert_eq!(second[2]["reasoning_content"], "El cliente pregunta por su pedido: consulto la guía.");
    assert_eq!(second[3]["role"], "tool");
    assert_eq!(second[3]["tool_call_id"], "call_00_abc");
    assert_eq!(second[3]["content"], r#"{"ciudad":"SILVANIA","guia":"240064842467"}"#);
}

#[tokio::test]
async fn the_fake_server_rejects_history_without_reasoning() {
    let (base_url, _) = start_fake_deepseek().await;
    let response = reqwest::Client::new()
        .post(format!("{base_url}/chat/completions"))
        .json(&json!({
            "model": "deepseek-flash",
            "tools": [{"type": "function", "function": {"name": "x", "parameters": {"type": "object"}}}],
            "messages": [
                {"role": "user", "content": "hola"},
                {"role": "assistant", "content": null,
                 "tool_calls": [{"id": "1", "type": "function", "function": {"name": "x", "arguments": "{}"}}]},
                {"role": "tool", "tool_call_id": "1", "content": "{}"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(response.text().await.unwrap().contains("must be passed back"));
}
