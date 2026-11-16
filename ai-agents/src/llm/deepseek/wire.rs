//! Formato JSON de `POST /chat/completions` de DeepSeek y su traducción.
//!
//! El punto delicado es `reasoning_content`: en modo thinking, si la
//! petición lleva herramientas, el razonamiento de TODOS los mensajes
//! previos del asistente debe volver a la API (aunque ese turno no haya
//! llamado herramientas). Si falta, la API responde 400. Por eso cada
//! mensaje del asistente se serializa siempre con su `reasoning_content`
//! cuando thinking está activo.

use serde::{Deserialize, Serialize};

use super::config::{DeepSeekConfig, ReasoningEffort, ThinkingMode};
use crate::llm::{
    AssistantMessage, ChatCompletion, ChatMessage, ChatRequest, FinishReason, LlmError, TokenUsage, ToolCall, ToolSpec,
};

// ----------------------------- Petición -----------------------------

#[derive(Debug, Serialize)]
pub struct WireRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireTool<'a>>,
    /// En modo thinking solo se admite `auto` (`required` o una función
    /// específica responden 400).
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<&'static str>,
    thinking: WireThinking,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<ReasoningEffort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct WireThinking {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(tag = "role", rename_all = "lowercase")]
enum WireMessage<'a> {
    System {
        content: &'a str,
    },
    User {
        content: &'a str,
    },
    Assistant {
        content: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning_content: Option<&'a str>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<WireToolCall<'a>>,
    },
    Tool {
        tool_call_id: &'a str,
        content: &'a str,
    },
}

#[derive(Debug, Serialize)]
struct WireToolCall<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireFunctionCall<'a>,
}

#[derive(Debug, Serialize)]
struct WireFunctionCall<'a> {
    name: &'a str,
    arguments: &'a str,
}

#[derive(Debug, Serialize)]
struct WireTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireFunction<'a>,
}

#[derive(Debug, Serialize)]
struct WireFunction<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

/// Traduce una petición del dominio al formato de DeepSeek.
pub fn build_request<'a>(config: &'a DeepSeekConfig, request: &ChatRequest<'a>) -> WireRequest<'a> {
    let thinking = config.thinking == ThinkingMode::Enabled;
    WireRequest {
        model: &config.model,
        messages: request.messages.iter().map(|message| to_wire_message(message, thinking)).collect(),
        tools: request.tools.iter().map(to_wire_tool).collect(),
        tool_choice: (!request.tools.is_empty()).then_some("auto"),
        thinking: WireThinking { kind: if thinking { "enabled" } else { "disabled" } },
        reasoning_effort: thinking.then_some(config.reasoning_effort),
        max_tokens: config.max_tokens,
        stream: false,
    }
}

fn to_wire_message(message: &ChatMessage, thinking: bool) -> WireMessage<'_> {
    match message {
        ChatMessage::System(content) => WireMessage::System { content },
        ChatMessage::User(content) => WireMessage::User { content },
        ChatMessage::Assistant(assistant) => WireMessage::Assistant {
            content: assistant.content.as_deref(),
            // Siempre de vuelta en modo thinking (vacío si el modelo no
            // razonó), nunca en modo normal.
            reasoning_content: thinking.then(|| assistant.reasoning.as_deref().unwrap_or("")),
            tool_calls: assistant
                .tool_calls
                .iter()
                .map(|call| WireToolCall {
                    id: &call.id,
                    kind: "function",
                    function: WireFunctionCall { name: &call.name, arguments: &call.arguments },
                })
                .collect(),
        },
        ChatMessage::ToolResult { call_id, content } => WireMessage::Tool { tool_call_id: call_id, content },
    }
}

fn to_wire_tool(spec: &ToolSpec) -> WireTool<'_> {
    WireTool {
        kind: "function",
        function: WireFunction { name: &spec.name, description: &spec.description, parameters: &spec.parameters },
    }
}

// ----------------------------- Respuesta -----------------------------

#[derive(Debug, Deserialize)]
pub struct WireResponse {
    #[serde(default)]
    choices: Vec<WireChoice>,
    #[serde(default)]
    usage: Option<WireUsage>,
}

#[derive(Debug, Deserialize)]
struct WireChoice {
    message: WireResponseMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<WireResponseToolCall>>,
}

#[derive(Debug, Deserialize)]
struct WireResponseToolCall {
    id: String,
    function: WireResponseFunction,
}

#[derive(Debug, Deserialize)]
struct WireResponseFunction {
    name: String,
    #[serde(default)]
    arguments: String,
}

#[derive(Debug, Default, Deserialize)]
struct WireUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    prompt_cache_hit_tokens: u64,
    #[serde(default)]
    completion_tokens_details: Option<WireCompletionDetails>,
}

#[derive(Debug, Default, Deserialize)]
struct WireCompletionDetails {
    #[serde(default)]
    reasoning_tokens: u64,
}

/// Traduce la respuesta de DeepSeek al dominio, conservando el
/// razonamiento para reenviarlo en la siguiente petición.
pub fn parse_response(response: WireResponse) -> Result<ChatCompletion, LlmError> {
    let choice = response.choices.into_iter().next().ok_or(LlmError::EmptyResponse)?;
    let message = AssistantMessage {
        content: choice.message.content.filter(|text| !text.trim().is_empty()),
        reasoning: choice.message.reasoning_content,
        tool_calls: choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|call| ToolCall { id: call.id, name: call.function.name, arguments: call.function.arguments })
            .collect(),
    };
    let usage = response.usage.unwrap_or_default();
    Ok(ChatCompletion {
        message,
        finish_reason: parse_finish_reason(choice.finish_reason.as_deref()),
        usage: TokenUsage {
            prompt_tokens: usage.prompt_tokens,
            cached_prompt_tokens: usage.prompt_cache_hit_tokens,
            completion_tokens: usage.completion_tokens,
            reasoning_tokens: usage.completion_tokens_details.map(|details| details.reasoning_tokens).unwrap_or(0),
        },
    })
}

fn parse_finish_reason(reason: Option<&str>) -> FinishReason {
    match reason {
        Some("stop") | None => FinishReason::Stop,
        Some("tool_calls") => FinishReason::ToolCalls,
        Some("length") => FinishReason::Length,
        Some("content_filter") => FinishReason::ContentFilter,
        Some("insufficient_system_resource") => FinishReason::InsufficientResources,
        Some(other) => FinishReason::Other(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool_round_trip() -> Vec<ChatMessage> {
        vec![
            ChatMessage::System("sistema".into()),
            ChatMessage::User("¿dónde está mi pedido?".into()),
            ChatMessage::Assistant(AssistantMessage {
                content: None,
                reasoning: Some("Debo consultar la guía".into()),
                tool_calls: vec![ToolCall { id: "call_1".into(), name: "detalle_guia".into(), arguments: "{}".into() }],
            }),
            ChatMessage::ToolResult { call_id: "call_1".into(), content: "{\"ok\":true}".into() },
        ]
    }

    fn spec() -> ToolSpec {
        ToolSpec { name: "detalle_guia".into(), description: "d".into(), parameters: json!({"type": "object"}) }
    }

    #[test]
    fn thinking_requests_pass_reasoning_back_and_use_max_effort() {
        let config = DeepSeekConfig::new("key");
        let messages = tool_round_trip();
        let tools = [spec()];
        let body =
            serde_json::to_value(build_request(&config, &ChatRequest { messages: &messages, tools: &tools })).unwrap();

        assert_eq!(body["model"], "deepseek-flash");
        assert_eq!(body["thinking"], json!({"type": "enabled"}));
        assert_eq!(body["reasoning_effort"], "max");
        assert_eq!(body["tool_choice"], "auto");
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("temperature").is_none(), "temperature is ignored in thinking mode");

        let assistant = &body["messages"][2];
        assert_eq!(assistant["role"], "assistant");
        assert_eq!(assistant["reasoning_content"], "Debo consultar la guía");
        assert_eq!(assistant["content"], serde_json::Value::Null);
        assert_eq!(assistant["tool_calls"][0]["function"]["name"], "detalle_guia");
        assert_eq!(body["messages"][3], json!({"role": "tool", "tool_call_id": "call_1", "content": "{\"ok\":true}"}));
    }

    #[test]
    fn assistant_without_reasoning_still_sends_the_field_in_thinking_mode() {
        let config = DeepSeekConfig::new("key");
        let messages =
            vec![ChatMessage::Assistant(AssistantMessage { content: Some("hola".into()), ..Default::default() })];
        let body =
            serde_json::to_value(build_request(&config, &ChatRequest { messages: &messages, tools: &[] })).unwrap();
        assert_eq!(body["messages"][0]["reasoning_content"], "");
        assert!(body.get("tools").is_none());
        assert!(body.get("tool_choice").is_none());
    }

    #[test]
    fn non_thinking_mode_omits_reasoning() {
        let mut config = DeepSeekConfig::new("key");
        config.thinking = ThinkingMode::Disabled;
        let messages = tool_round_trip();
        let body =
            serde_json::to_value(build_request(&config, &ChatRequest { messages: &messages, tools: &[] })).unwrap();
        assert_eq!(body["thinking"], json!({"type": "disabled"}));
        assert!(body.get("reasoning_effort").is_none());
        assert!(body["messages"][2].get("reasoning_content").is_none());
    }

    #[test]
    fn parses_reasoning_tool_calls_and_usage() {
        let response: WireResponse = serde_json::from_value(json!({
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "",
                    "reasoning_content": "Pienso...",
                    "tool_calls": [{"id": "call_9", "type": "function",
                                    "function": {"name": "escalar_a_humano", "arguments": "{\"motivo\":\"x\"}"}}]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 100, "completion_tokens": 40, "prompt_cache_hit_tokens": 64,
                      "completion_tokens_details": {"reasoning_tokens": 30}}
        }))
        .unwrap();

        let completion = parse_response(response).unwrap();
        assert_eq!(completion.finish_reason, FinishReason::ToolCalls);
        assert_eq!(completion.message.content, None);
        assert_eq!(completion.message.reasoning.as_deref(), Some("Pienso..."));
        assert_eq!(completion.message.tool_calls[0].name, "escalar_a_humano");
        assert_eq!(completion.usage.cached_prompt_tokens, 64);
        assert_eq!(completion.usage.reasoning_tokens, 30);
    }

    #[test]
    fn empty_choices_is_an_error() {
        let response: WireResponse = serde_json::from_value(json!({"choices": []})).unwrap();
        assert!(matches!(parse_response(response), Err(LlmError::EmptyResponse)));
    }
}
