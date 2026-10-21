use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, NaiveDateTime};

use crate::chats::repository::ChatRepositoryTrait;
use crate::messages::dtos::{
    IncomingMessageDto, MediaType, Message, MessageStatus, OutgoingMessageDto, SendMessageDto,
};
use crate::messages::meta_client::MetaClientTrait;
use crate::messages::repository::{MessageRepositoryTrait, NewMessage};
use crate::tools::phones::normalize_phone;
use crate::users::repository::UserRepositoryTrait;

/// Ventana de atención al cliente de Meta: tras el último mensaje del
/// usuario solo se pueden enviar mensajes libres durante 24 horas.
pub const CUSTOMER_SERVICE_WINDOW: Duration = Duration::hours(24);

/// Regla de la ventana de 24h, como función pura para probarla fácilmente.
pub fn is_window_open_at(last_user_message_timestamp: Option<NaiveDateTime>, now: NaiveDateTime) -> bool {
    match last_user_message_timestamp {
        Some(timestamp) => {
            now >= timestamp && now.signed_duration_since(timestamp) < CUSTOMER_SERVICE_WINDOW
        }
        None => false,
    }
}

/// Texto de vista previa en la lista de chats según el tipo de mensaje.
fn message_preview(media_type: MediaType, message: &Option<String>) -> String {
    match (media_type, message) {
        (MediaType::Text, Some(text)) => text.clone(),
        (MediaType::Text, None) => String::new(),
        (MediaType::Document, Some(caption)) => format!("[documento] {caption}"),
        (MediaType::Document, None) => "[documento]".to_string(),
        (MediaType::Audio, _) => "[audio]".to_string(),
        (MediaType::Image, Some(caption)) => format!("[imagen] {caption}"),
        (MediaType::Image, None) => "[imagen]".to_string(),
        (MediaType::Video, Some(caption)) => format!("[video] {caption}"),
        (MediaType::Video, None) => "[video]".to_string(),
    }
}

/// Límites de tamaño aceptados en memoria para envíos multimedia
/// (alineados con los límites de la WhatsApp Cloud API).
pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_VIDEO_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_AUDIO_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_DOCUMENT_BYTES: usize = 25 * 1024 * 1024;

/// Valida mime y tamaño según el tipo de medio. Función pura para probarla.
pub fn validate_media_upload(media_type: MediaType, mime: &str, size: usize) -> Result<(), String> {
    let mime = mime.to_lowercase();
    match media_type {
        MediaType::Text => Err("Text messages do not use media upload".to_string()),
        MediaType::Image => {
            if !(mime == "image/jpeg" || mime == "image/png" || mime == "image/webp") {
                return Err("Formato de imagen no soportado. Usa JPG, PNG o WEBP.".to_string());
            }
            if size == 0 {
                return Err("El archivo está vacío.".to_string());
            }
            if size > MAX_IMAGE_BYTES {
                return Err("La imagen supera los 5MB permitidos.".to_string());
            }
            Ok(())
        }
        MediaType::Video => {
            if !(mime == "video/mp4" || mime == "video/3gpp" || mime == "video/3gpp2" || mime == "video/quicktime") {
                return Err("Formato de video no soportado. Usa MP4.".to_string());
            }
            if size == 0 {
                return Err("El archivo está vacío.".to_string());
            }
            if size > MAX_VIDEO_BYTES {
                return Err("El video supera los 16MB permitidos.".to_string());
            }
            Ok(())
        }
        MediaType::Audio => {
            let ok = mime.starts_with("audio/")
                || mime == "video/webm"
                || mime == "application/ogg"
                || mime == "application/octet-stream";
            if !ok {
                return Err("Formato de audio no soportado.".to_string());
            }
            if size == 0 {
                return Err("El archivo está vacío.".to_string());
            }
            if size > MAX_AUDIO_BYTES {
                return Err("El audio supera los 16MB permitidos.".to_string());
            }
            Ok(())
        }
        MediaType::Document => {
            let ok = mime == "application/pdf"
                || mime == "text/plain"
                || mime == "text/csv"
                || mime == "application/vnd.ms-excel"
                || mime == "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                || mime == "application/msword"
                || mime == "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                || mime == "application/vnd.ms-powerpoint"
                || mime == "application/vnd.openxmlformats-officedocument.presentationml.presentation"
                || mime.starts_with("application/")
                || mime.starts_with("text/");
            if !ok {
                return Err("Formato de documento no soportado.".to_string());
            }
            if size == 0 {
                return Err("El archivo está vacío.".to_string());
            }
            if size > MAX_DOCUMENT_BYTES {
                return Err("El documento supera los 25MB permitidos.".to_string());
            }
            Ok(())
        }
    }
}

/// Caption opcional para mensajes multimedia (límite de Meta: 1024).
pub fn validate_media_caption(caption: Option<&str>) -> Result<Option<String>, String> {
    match caption {
        None => Ok(None),
        Some(c) => {
            let trimmed = c.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.chars().count() > 1024 {
                return Err("El texto adjunto no puede superar 1024 caracteres.".to_string());
            }
            Ok(Some(trimmed.to_string()))
        }
    }
}

/// Salida normalizada para notas de voz: lo único que las apps
/// nativas de WhatsApp reproducen de forma fiable como PTT.
/// (Meta exige .ogg con códec OPUS, mono; ver docs de audio-messages.)
pub const VOICE_MIME: &str = "audio/ogg";
pub const VOICE_FILENAME: &str = "audio.ogg";
/// Tiempo máximo para la transcodificación de una nota de voz.
pub const VOICE_TRANSCODE_TIMEOUT_SECS: u64 = 120;  /// 2 minutos, suficiente para audios largos de 16MB.

/// Sufijo para el archivo temporal de entrada según el MIME declarado.
/// ffmpeg detecta por contenido, pero el sufijo ayuda al demuxer.
pub fn voice_input_suffix(mime: &str) -> &'static str {
    match mime.to_lowercase().as_str() {
        m if m.contains("webm") => ".webm",
        m if m.contains("mp4") || m.contains("m4a") => ".m4a",
        m if m.contains("mpeg") || m.contains("mp3") => ".mp3",
        m if m.contains("ogg") || m.contains("opus") => ".ogg",
        m if m.contains("aac") => ".aac",
        m if m.contains("amr") => ".amr",
        m if m.contains("wav") => ".wav",
        _ => ".bin",
    }
}

/// Argumentos de ffmpeg para normalizar cualquier audio de entrada a
/// nota de voz: mono, 16kHz, OPUS a 32k, contenedor OGG.
pub fn ffmpeg_voice_args(input_path: &str, output_path: &str) -> Vec<String> {
    vec![
        "-y".to_string(),
        "-i".to_string(),
        input_path.to_string(),
        "-ac".to_string(),
        "1".to_string(),
        "-ar".to_string(),
        "16000".to_string(),
        "-c:a".to_string(),
        "libopus".to_string(),
        "-b:a".to_string(),
        "32k".to_string(),
        "-f".to_string(),
        "ogg".to_string(),
        output_path.to_string(),
    ]
}

/// Transcodificador de notas de voz a OGG/Opus. Se abstrae en trait
/// para poder usar un fake en tests sin requerir el binario ffmpeg.
#[async_trait]
pub trait VoiceTranscoderTrait: Send + Sync {
    /// Convierte cualquier audio de entrada (webm/mp4/mpeg/ogg/...) a
    /// OGG/Opus mono. Retorna (bytes_ogg, filename, mime).
    async fn transcode_to_voice_ogg(&self, bytes: Vec<u8>, mime_hint: &str) -> Result<(Vec<u8>, String, String), String>;
}

pub struct FfmpegVoiceTranscoder {
    ffmpeg_path: String,
}

impl FfmpegVoiceTranscoder {
    pub fn new() -> Self {
        Self {
            ffmpeg_path: std::env::var("FFMPEG_PATH").unwrap_or_else(|_| "ffmpeg".to_string()),
        }
    }

    #[cfg(test)]
    pub fn with_path(path: &str) -> Self {
        Self { ffmpeg_path: path.to_string() }
    }
}

impl Default for FfmpegVoiceTranscoder {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VoiceTranscoderTrait for FfmpegVoiceTranscoder {
    async fn transcode_to_voice_ogg(&self, bytes: Vec<u8>, mime_hint: &str) -> Result<(Vec<u8>, String, String), String> {
        use std::io::Write;

        if bytes.is_empty() {
            return Err("El archivo está vacío.".to_string());
        }

        let suffix = voice_input_suffix(mime_hint);
        let mut input_file = tempfile::Builder::new()
            .prefix("voice-in-")
            .suffix(suffix)
            .tempfile()
            .map_err(|e| format!("No se pudo crear archivo temporal: {e}"))?;
        input_file
            .write_all(&bytes)
            .map_err(|e| format!("No se pudo escribir audio temporal: {e}"))?;
        input_file
            .flush()
            .map_err(|e| format!("No se pudo escribir audio temporal: {e}"))?;
        let input_path = input_file.path().to_string_lossy().to_string();

        let output_file = tempfile::Builder::new()
            .prefix("voice-out-")
            .suffix(".ogg")
            .tempfile()
            .map_err(|e| format!("No se pudo crear archivo temporal: {e}"))?;
        let output_path = output_file.path().to_string_lossy().to_string();
        
        drop(output_file); // Cerrar handles

        let args = ffmpeg_voice_args(&input_path, &output_path);
        println!(
            "Transcoding voice note: input_mime={mime_hint} input_bytes={} -> ogg opus mono",
            bytes.len()
        );

        let mut child = tokio::process::Command::new(&self.ffmpeg_path)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    "ffmpeg no está instalado en el servidor. Contacta al administrador.".to_string()
                } else {
                    format!("No se pudo ejecutar ffmpeg: {e}")
                }
            })?;

        // Esperar con timeout: con kill_on_drop(true) el proceso se mata
        // solo al expirar el timeout y dropearse el futuro.
        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(VOICE_TRANSCODE_TIMEOUT_SECS),
            child.wait_with_output(),
        )
        .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => return Err(format!("Error ejecutando ffmpeg: {e}")),
            Err(_) => {
                return Err("La conversión del audio tardó demasiado. Intenta con un audio más corto.".to_string());
            }
        };
        // Los temporales se borran al salir del scope (input_file drop aquí).
        drop(input_file);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let snippet: String = stderr.chars().take(500).collect();
            eprintln!("ffmpeg voice transcode failed: {snippet}");
            return Err("No se pudo procesar el audio para nota de voz. Prueba de nuevo o adjúntalo como documento.".to_string());
        }

        let ogg_bytes = tokio::fs::read(&output_path)
            .await
            .map_err(|e| format!("No se pudo leer el audio convertido: {e}"))?;
        // Borrar el temporal de salida explícitamente (el NamedTempFile ya
        // hizo drop, pero el path puede persistir según plataforma).
        let _ = tokio::fs::remove_file(&output_path).await;

        if ogg_bytes.is_empty() {
            return Err("La conversión del audio produjo un archivo vacío.".to_string());
        }
        if ogg_bytes.len() > MAX_AUDIO_BYTES {
            return Err("El audio supera los 16MB permitidos.".to_string());
        }

        println!("Voice note transcoded: {} bytes -> {} bytes ogg", bytes.len(), ogg_bytes.len());
        Ok((ogg_bytes, VOICE_FILENAME.to_string(), VOICE_MIME.to_string()))
    }
}

#[async_trait]
pub trait MessageServiceTrait: Send + Sync {
    async fn get_chat_messages(&self, business_id: i32, user_phone: &str) -> Result<Vec<Message>, String>;
    async fn send_free_message(&self, business_id: i32, user_phone: &str, dto: SendMessageDto) -> Result<Message, String>;
    /// Envía un mensaje multimedia libre (imagen/video/audio/documento)
    /// dentro de la ventana de 24h: sube el archivo a Meta y luego envía
    /// el mensaje referenciando el media_id.
    /// `is_voice` solo aplica a Audio: true convierte a OGG/Opus mono con
    /// ffmpeg y lo envía como nota de voz nativa (voice:true).
    async fn send_free_media_message(
        &self,
        business_id: i32,
        user_phone: &str,
        media_type: MediaType,
        caption: Option<String>,
        filename: String,
        mime: String,
        bytes: Vec<u8>,
        is_voice: bool,
    ) -> Result<Message, String>;
    async fn register_incoming(&self, dto: IncomingMessageDto) -> Result<Message, String>;
    async fn register_outgoing(&self, dto: OutgoingMessageDto) -> Result<Message, String>;
    async fn update_status(&self, meta_message_id: &str, status: MessageStatus) -> Result<(), String>;
    /// Descarga multimedia de Meta en memoria (sin tocar disco) para
    /// visualizarla/reproducirla en el navegador.
    async fn fetch_media(&self, media_id: &str) -> Result<(String, Vec<u8>), String>;
}

pub struct MessageService {
    message_repository: Arc<dyn MessageRepositoryTrait>,
    chat_repository: Arc<dyn ChatRepositoryTrait>,
    user_repository: Arc<dyn UserRepositoryTrait>,
    meta_client: Arc<dyn MetaClientTrait>,
    voice_transcoder: Arc<dyn VoiceTranscoderTrait>,
}

impl MessageService {
    pub fn new(
        message_repository: Arc<dyn MessageRepositoryTrait>,
        chat_repository: Arc<dyn ChatRepositoryTrait>,
        user_repository: Arc<dyn UserRepositoryTrait>,
        meta_client: Arc<dyn MetaClientTrait>,
        voice_transcoder: Arc<dyn VoiceTranscoderTrait>,
    ) -> Self {
        Self {
            message_repository,
            chat_repository,
            user_repository,
            meta_client,
            voice_transcoder,
        }
    }
}

#[async_trait]
impl MessageServiceTrait for MessageService {
    async fn get_chat_messages(&self, business_id: i32, user_phone: &str) -> Result<Vec<Message>, String> {
        self.message_repository.get_messages_by_chat(business_id, &normalize_phone(user_phone)).await
    }

    async fn send_free_message(&self, business_id: i32, user_phone: &str, dto: SendMessageDto) -> Result<Message, String> {
        let user_phone = &normalize_phone(user_phone);
        let chat = self.chat_repository
            .get_chat(business_id, user_phone)
            .await?
            .ok_or_else(|| "Chat not found".to_string())?;

        let now = chrono::Utc::now().naive_utc();
        if !is_window_open_at(chat.last_user_message_timestamp, now) {
            return Err("Customer service window is closed (24h). Use a template message instead.".to_string());
        }

        let meta_message_id = self.meta_client
            .send_text_message(user_phone, &dto.message)
            .await?;

        let new_message = NewMessage {
            meta_message_id,
            business_id,
            user_id: user_phone.to_string(),
            media_id: None,
            media_type: MediaType::Text,
            message: Some(dto.message),
            status: Some(MessageStatus::Sent),
            from_user: false,
            created_at: None,
        };

        self.message_repository.save_message(&new_message).await
    }

    async fn send_free_media_message(
        &self,
        business_id: i32,
        user_phone: &str,
        media_type: MediaType,
        caption: Option<String>,
        filename: String,
        mime: String,
        bytes: Vec<u8>,
        is_voice: bool,
    ) -> Result<Message, String> {
        if media_type == MediaType::Text {
            return Err("Use text endpoint for text messages".to_string());
        }
        if is_voice && media_type != MediaType::Audio {
            return Err("Voice messages must be audio".to_string());
        }
        validate_media_upload(media_type, &mime, bytes.len())?;
        let caption = validate_media_caption(caption.as_deref())?;

        let user_phone = &normalize_phone(user_phone);
        let chat = self.chat_repository
            .get_chat(business_id, user_phone)
            .await?
            .ok_or_else(|| "Chat not found".to_string())?;

        let now = chrono::Utc::now().naive_utc();
        if !is_window_open_at(chat.last_user_message_timestamp, now) {
            return Err("Customer service window is closed (24h). Use a template message instead.".to_string());
        }

        // Notas de voz: normalizar a OGG/Opus mono con ffmpeg antes de
        // subir. Los webm/mp4 que graba el navegador no los decodifican
        // las apps nativas (solo Web los reproduce).
        // Las notas de voz no llevan caption en WhatsApp.
        let (upload_bytes, upload_filename, upload_mime, stored_caption, send_voice) =
            if media_type == MediaType::Audio && is_voice {
                let (ogg, name, mime) = self.voice_transcoder
                    .transcode_to_voice_ogg(bytes, &mime)
                    .await?;
                (ogg, name, mime, None, true)
            } else {
                (bytes, filename, mime, caption, false)
            };

        // 1. Subir el archivo a Meta (en memoria, sin tocar disco).
        let uploaded_media_id = self.meta_client
            .upload_media(upload_bytes, &upload_filename, &upload_mime)
            .await?;

        // 2. Enviar el mensaje multimedia referenciando el media_id.
        let meta_message_id = self.meta_client
            .send_media_message(user_phone, &uploaded_media_id, media_type, stored_caption.as_deref(), Some(upload_filename.as_str()), send_voice)
            .await?;

        let new_message = NewMessage {
            meta_message_id,
            business_id,
            user_id: user_phone.to_string(),
            media_id: Some(uploaded_media_id),
            media_type,
            message: stored_caption,
            status: Some(MessageStatus::Sent),
            from_user: false,
            created_at: None,
        };

        self.message_repository.save_message(&new_message).await
    }

    async fn register_incoming(&self, dto: IncomingMessageDto) -> Result<Message, String> {
        let user_phone = normalize_phone(&dto.user_phone);

        self.user_repository
            .upsert_user(&user_phone, dto.user_name.as_deref().unwrap_or(""))
            .await?;

        let business_id = match dto.business_id {
            Some(id) => id,
            None => self.chat_repository
                .find_latest_chat_business(&user_phone)
                .await?
                .ok_or_else(|| "Chat not found for user".to_string())?,
        };

        let timestamp = dto.timestamp
            .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
            .map(|dt| dt.naive_utc())
            .unwrap_or_else(|| chrono::Utc::now().naive_utc());

        self.chat_repository
            .update_last_user_message(
                business_id,
                &user_phone,
                &message_preview(dto.media_type, &dto.message),
                timestamp,
            )
            .await?;

        let new_message = NewMessage {
            meta_message_id: dto.meta_message_id,
            business_id,
            user_id: user_phone,
            media_id: dto.media_id,
            media_type: dto.media_type,
            message: dto.message,
            status: None, // los estados solo aplican a mensajes salientes
            from_user: true,
            created_at: Some(timestamp),
        };

        self.message_repository.save_message(&new_message).await
    }

    async fn register_outgoing(&self, dto: OutgoingMessageDto) -> Result<Message, String> {
        let user_phone = normalize_phone(&dto.user_phone);

        self.user_repository
            .upsert_user(&user_phone, dto.user_name.as_deref().unwrap_or(""))
            .await?;

        self.chat_repository
            .upsert_chat(dto.business_id, &user_phone)
            .await?;

        let new_message = NewMessage {
            meta_message_id: dto.meta_message_id,
            business_id: dto.business_id,
            user_id: user_phone,
            media_id: dto.media_id,
            media_type: dto.media_type,
            message: dto.message,
            status: Some(MessageStatus::Sent),
            from_user: false,
            created_at: None,
        };

        self.message_repository.save_message(&new_message).await
    }

    async fn update_status(&self, meta_message_id: &str, status: MessageStatus) -> Result<(), String> {
        // Si el mensaje no está registrado (ej. enviado antes de este
        // sistema), simplemente se ignora la actualización.
        self.message_repository
            .update_status_by_meta_id(meta_message_id, status)
            .await?;

        Ok(())
    }

    async fn fetch_media(&self, media_id: &str) -> Result<(String, Vec<u8>), String> {
        self.meta_client.fetch_media(media_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chats::dtos::{Chat, ChatSummary};
    use crate::users::dtos::User;
    use std::sync::Mutex;

    // ---------- Fakes ----------

    struct FakeMessageRepository {
        saved: Mutex<Vec<NewMessage>>,
    }

    impl FakeMessageRepository {
        fn new() -> Self {
            Self { saved: Mutex::new(Vec::new()) }
        }
    }

    #[async_trait]
    impl MessageRepositoryTrait for FakeMessageRepository {
        async fn save_message(&self, message: &NewMessage) -> Result<Message, String> {
            let row = Message {
                id: self.saved.lock().unwrap().len() as i32 + 1,
                meta_message_id: message.meta_message_id.clone(),
                business_id: message.business_id,
                user_id: message.user_id.clone(),
                media_id: message.media_id.clone(),
                media_type: message.media_type,
                message: message.message.clone(),
                status: message.status,
                from_user: message.from_user,
                created_at: message.created_at.unwrap_or_else(|| chrono::Utc::now().naive_utc()),
            };
            self.saved.lock().unwrap().push(NewMessage {
                meta_message_id: message.meta_message_id.clone(),
                business_id: message.business_id,
                user_id: message.user_id.clone(),
                media_id: message.media_id.clone(),
                media_type: message.media_type,
                message: message.message.clone(),
                status: message.status,
                from_user: message.from_user,
                created_at: message.created_at,
            });
            Ok(row)
        }

        async fn get_messages_by_chat(&self, _: i32, _: &str) -> Result<Vec<Message>, String> {
            Ok(Vec::new())
        }

        async fn update_status_by_meta_id(&self, _: &str, _: MessageStatus) -> Result<bool, String> {
            Ok(true)
        }
    }

    struct FakeChatRepository {
        chats: Mutex<Vec<Chat>>,
        last_updates: Mutex<Vec<(i32, String, String)>>,
    }

    impl FakeChatRepository {
        fn with_chats(chats: Vec<Chat>) -> Self {
            Self { chats: Mutex::new(chats), last_updates: Mutex::new(Vec::new()) }
        }
    }

    #[async_trait]
    impl ChatRepositoryTrait for FakeChatRepository {
        async fn get_chats_by_business(&self, _: i32) -> Result<Vec<ChatSummary>, String> {
            Ok(Vec::new())
        }

        async fn get_chat(&self, business_id: i32, user_id: &str) -> Result<Option<Chat>, String> {
            Ok(self.chats.lock().unwrap().iter()
                .find(|c| c.business_id == business_id && c.user_id == user_id)
                .cloned())
        }

        async fn upsert_chat(&self, business_id: i32, user_id: &str) -> Result<(), String> {
            let mut chats = self.chats.lock().unwrap();
            if !chats.iter().any(|c| c.business_id == business_id && c.user_id == user_id) {
                chats.push(Chat {
                    business_id,
                    user_id: user_id.to_string(),
                    last_user_message_timestamp: None,
                    last_user_message: None,
                    is_important: false,
                    last_guide_notification_at: None,
                });
            }
            Ok(())
        }

        async fn update_last_user_message(&self, business_id: i32, user_id: &str, message: &str, timestamp: NaiveDateTime) -> Result<(), String> {
            self.last_updates.lock().unwrap().push((business_id, user_id.to_string(), message.to_string()));
            self.upsert_chat(business_id, user_id).await?;
            let mut chats = self.chats.lock().unwrap();
            if let Some(chat) = chats.iter_mut().find(|c| c.business_id == business_id && c.user_id == user_id) {
                chat.last_user_message = Some(message.to_string());
                chat.last_user_message_timestamp = Some(timestamp);
            }
            Ok(())
        }

        async fn find_latest_chat_business(&self, user_id: &str) -> Result<Option<i32>, String> {
            Ok(self.chats.lock().unwrap().iter()
                .find(|c| c.user_id == user_id)
                .map(|c| c.business_id))
        }

        async fn set_importance(&self, _: i32, _: &str, _: bool) -> Result<bool, String> {
            Ok(true)
        }

        async fn touch_guide_notification(&self, _: &str, _: NaiveDateTime) -> Result<u64, String> {
            Ok(1)
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

    struct FakeMetaClient {
        sent: Mutex<Vec<(String, String)>>,
        uploaded: Mutex<Vec<(String, String)>>,
        sent_media: Mutex<Vec<(String, String, bool)>>,
    }

    #[async_trait]
    impl MetaClientTrait for FakeMetaClient {
        async fn send_text_message(&self, to_number: &str, text: &str) -> Result<String, String> {
            self.sent.lock().unwrap().push((to_number.to_string(), text.to_string()));
            Ok("wamid.fake".to_string())
        }

        async fn upload_media(&self, bytes: Vec<u8>, filename: &str, mime: &str) -> Result<String, String> {
            self.uploaded.lock().unwrap().push((filename.to_string(), mime.to_string()));
            Ok(format!("mid.{}", bytes.len()))
        }

        async fn send_media_message(
            &self,
            to_number: &str,
            media_id: &str,
            _kind: MediaType,
            _caption: Option<&str>,
            _filename: Option<&str>,
            voice: bool,
        ) -> Result<String, String> {
            self.sent_media.lock().unwrap().push((to_number.to_string(), media_id.to_string(), voice));
            Ok("wamid.fake-media".to_string())
        }

        async fn fetch_media(&self, _: &str) -> Result<(String, Vec<u8>), String> {
            Ok(("application/pdf".to_string(), b"%PDF-fake".to_vec()))
        }
    }

    struct FakeVoiceTranscoder {
        should_fail: bool,
        calls: Mutex<Vec<String>>,
    }

    impl FakeVoiceTranscoder {
        fn ok() -> Self {
            Self { should_fail: false, calls: Mutex::new(Vec::new()) }
        }

        fn failing() -> Self {
            Self { should_fail: true, calls: Mutex::new(Vec::new()) }
        }
    }

    #[async_trait]
    impl VoiceTranscoderTrait for FakeVoiceTranscoder {
        async fn transcode_to_voice_ogg(&self, bytes: Vec<u8>, mime_hint: &str) -> Result<(Vec<u8>, String, String), String> {
            self.calls.lock().unwrap().push(mime_hint.to_string());
            if self.should_fail {
                return Err("ffmpeg fake failure".to_string());
            }
            Ok((format!("ogg-converted-{}-bytes", bytes.len()).into_bytes(), VOICE_FILENAME.to_string(), VOICE_MIME.to_string()))
        }
    }

    fn build_service(chats: Vec<Chat>) -> (MessageService, Arc<FakeMessageRepository>, Arc<FakeChatRepository>, Arc<FakeMetaClient>) {
        build_service_with_transcoder(chats, Arc::new(FakeVoiceTranscoder::ok()))
    }

    fn build_service_with_transcoder(
        chats: Vec<Chat>,
        transcoder: Arc<FakeVoiceTranscoder>,
    ) -> (MessageService, Arc<FakeMessageRepository>, Arc<FakeChatRepository>, Arc<FakeMetaClient>) {
        let message_repo = Arc::new(FakeMessageRepository::new());
        let chat_repo = Arc::new(FakeChatRepository::with_chats(chats));
        let user_repo = Arc::new(FakeUserRepository);
        let meta_client = Arc::new(FakeMetaClient { sent: Mutex::new(Vec::new()), uploaded: Mutex::new(Vec::new()), sent_media: Mutex::new(Vec::new()) });
        let transcoder_trait: Arc<dyn VoiceTranscoderTrait> = transcoder;
        (
            MessageService::new(message_repo.clone(), chat_repo.clone(), user_repo, meta_client.clone(), transcoder_trait),
            message_repo,
            chat_repo,
            meta_client,
        )
    }

    // ---------- Tests de la ventana de 24h ----------

    #[test]
    fn window_is_closed_without_user_message() {
        assert!(!is_window_open_at(None, chrono::Utc::now().naive_utc()));
    }

    #[test]
    fn window_is_open_within_24h() {
        let now = chrono::Utc::now().naive_utc();
        assert!(is_window_open_at(Some(now - Duration::hours(23)), now));
        assert!(is_window_open_at(Some(now), now));
    }

    #[test]
    fn window_is_closed_after_24h() {
        let now = chrono::Utc::now().naive_utc();
        assert!(!is_window_open_at(Some(now - Duration::hours(24)), now));
        assert!(!is_window_open_at(Some(now - Duration::hours(25)), now));
    }

    #[test]
    fn window_is_closed_for_future_timestamps() {
        let now = chrono::Utc::now().naive_utc();
        assert!(!is_window_open_at(Some(now + Duration::hours(1)), now));
    }

    // ---------- Tests de send_free_message ----------

    #[tokio::test]
    async fn send_free_message_without_chat_fails() {
        let (service, _, _, _) = build_service(Vec::new());
        let result = service.send_free_message(1, "573003579384", SendMessageDto { message: "Hola".into() }).await;
        assert_eq!(result.unwrap_err(), "Chat not found");
    }

    #[tokio::test]
    async fn send_free_message_with_closed_window_fails() {
        let old = chrono::Utc::now().naive_utc() - Duration::hours(30);
        let chats = vec![Chat { business_id: 1, user_id: "573003579384".into(), last_user_message_timestamp: Some(old), last_user_message: None, is_important: false, last_guide_notification_at: None }];
        let (service, _, _, meta) = build_service(chats);

        let result = service.send_free_message(1, "573003579384", SendMessageDto { message: "Hola".into() }).await;
        assert!(result.unwrap_err().contains("window is closed"));
        assert!(meta.sent.lock().unwrap().is_empty()); // no se llamó a Meta
    }

    #[tokio::test]
    async fn send_free_message_with_open_window_sends_and_persists() {
        let recent = chrono::Utc::now().naive_utc() - Duration::hours(1);
        let chats = vec![Chat { business_id: 1, user_id: "573003579384".into(), last_user_message_timestamp: Some(recent), last_user_message: None, is_important: false, last_guide_notification_at: None }];
        let (service, message_repo, _, meta) = build_service(chats);

        let message = service.send_free_message(1, "573003579384", SendMessageDto { message: "Hola!".into() }).await.unwrap();

        assert_eq!(message.meta_message_id, "wamid.fake");
        assert!(!message.from_user);
        assert_eq!(message.status, Some(MessageStatus::Sent));
        assert_eq!(meta.sent.lock().unwrap().as_slice(), [("573003579384".to_string(), "Hola!".to_string())]);
        assert_eq!(message_repo.saved.lock().unwrap().len(), 1);
    }

    // ---------- Tests de register_incoming ----------

    #[tokio::test]
    async fn incoming_message_updates_chat_last_message() {
        let (service, _, chat_repo, _) = build_service(Vec::new());

        let dto = IncomingMessageDto {
            user_phone: "573003579384".into(),
            user_name: Some("Stiven".into()),
            business_id: Some(7),
            meta_message_id: "wamid.in1".into(),
            media_type: MediaType::Text,
            message: Some("Hola".into()),
            media_id: None,
            timestamp: Some(1787200338),
        };
        let message = service.register_incoming(dto).await.unwrap();

        assert!(message.from_user);
        assert_eq!(message.status, None);
        let updates = chat_repo.last_updates.lock().unwrap();
        assert_eq!(updates.as_slice(), [(7, "573003579384".to_string(), "Hola".to_string())]);
    }

    #[tokio::test]
    async fn incoming_media_message_uses_preview_placeholder() {
        let (service, _, chat_repo, _) = build_service(Vec::new());

        let dto = IncomingMessageDto {
            user_phone: "573003579384".into(),
            user_name: None,
            business_id: Some(1),
            meta_message_id: "wamid.in2".into(),
            media_type: MediaType::Audio,
            message: None,
            media_id: Some("123".into()),
            timestamp: None,
        };
        service.register_incoming(dto).await.unwrap();

        assert_eq!(chat_repo.last_updates.lock().unwrap()[0].2, "[audio]");
    }

    #[tokio::test]
    async fn incoming_without_business_and_without_chat_fails() {
        let (service, _, _, _) = build_service(Vec::new());

        let dto = IncomingMessageDto {
            user_phone: "573003579384".into(),
            user_name: None,
            business_id: None,
            meta_message_id: "wamid.in3".into(),
            media_type: MediaType::Text,
            message: Some("Hola".into()),
            media_id: None,
            timestamp: None,
        };
        assert_eq!(service.register_incoming(dto).await.unwrap_err(), "Chat not found for user");
    }

    #[tokio::test]
    async fn incoming_without_business_uses_existing_chat_business() {
        let chats = vec![Chat { business_id: 9, user_id: "573003579384".into(), last_user_message_timestamp: None, last_user_message: None, is_important: false, last_guide_notification_at: None }];
        let (service, message_repo, _, _) = build_service(chats);

        let dto = IncomingMessageDto {
            user_phone: "573003579384".into(),
            user_name: None,
            business_id: None,
            meta_message_id: "wamid.in4".into(),
            media_type: MediaType::Text,
            message: Some("Hola".into()),
            media_id: None,
            timestamp: None,
        };
        service.register_incoming(dto).await.unwrap();

        assert_eq!(message_repo.saved.lock().unwrap()[0].business_id, 9);
    }

    // ---------- Tests de register_outgoing ----------

    #[tokio::test]
    async fn outgoing_message_creates_chat_if_missing() {
        let (service, _, chat_repo, _) = build_service(Vec::new());

        let dto = OutgoingMessageDto {
            business_id: 3,
            user_phone: "573003579384".into(),
            user_name: Some("Stiven".into()),
            meta_message_id: "wamid.out1".into(),
            media_type: MediaType::Document,
            message: None,
            media_id: Some("media123".into()),
        };
        let message = service.register_outgoing(dto).await.unwrap();

        assert!(!message.from_user);
        assert_eq!(message.status, Some(MessageStatus::Sent));
        assert!(chat_repo.chats.lock().unwrap().iter().any(|c| c.business_id == 3 && c.user_id == "573003579384"));
    }

    // ---------- Tests de validación multimedia ----------

    #[test]
    fn media_validation_accepts_supported_types() {
        assert!(validate_media_upload(MediaType::Image, "image/jpeg", 1024).is_ok());
        assert!(validate_media_upload(MediaType::Video, "video/mp4", 1024).is_ok());
        assert!(validate_media_upload(MediaType::Audio, "audio/webm", 1024).is_ok());
        assert!(validate_media_upload(MediaType::Audio, "audio/ogg", 1024).is_ok());
        assert!(validate_media_upload(MediaType::Document, "application/pdf", 1024).is_ok());
        assert!(validate_media_upload(
            MediaType::Document,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            1024
        )
        .is_ok());
    }

    #[test]
    fn media_validation_rejects_wrong_mime_and_oversize() {
        assert!(validate_media_upload(MediaType::Image, "video/mp4", 100).is_err());
        assert!(validate_media_upload(MediaType::Image, "image/jpeg", MAX_IMAGE_BYTES + 1).is_err());
        assert!(validate_media_upload(MediaType::Video, "video/mp4", MAX_VIDEO_BYTES + 1).is_err());
        assert!(validate_media_upload(MediaType::Audio, "audio/mpeg", MAX_AUDIO_BYTES + 1).is_err());
        assert!(validate_media_upload(MediaType::Document, "application/pdf", 0).is_err());
        assert!(validate_media_upload(MediaType::Text, "text/plain", 10).is_err());
    }

    #[test]
    fn media_caption_trims_and_limits_length() {
        assert_eq!(validate_media_caption(None).unwrap(), None);
        assert_eq!(validate_media_caption(Some("   ")).unwrap(), None);
        assert_eq!(
            validate_media_caption(Some("  hola ")).unwrap(),
            Some("hola".to_string())
        );
        assert!(validate_media_caption(Some(&"x".repeat(1025))).is_err());
    }

    // ---------- Tests de send_free_media_message ----------

    #[tokio::test]
    async fn send_media_without_chat_fails() {
        let (service, _, _, meta) = build_service(Vec::new());
        let result = service
            .send_free_media_message(1, "573003579384", MediaType::Image, None, "foto.jpg".into(), "image/jpeg".into(), vec![1, 2, 3], false)
            .await;
        assert_eq!(result.unwrap_err(), "Chat not found");
        assert!(meta.uploaded.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn send_media_with_closed_window_fails() {
        let old = chrono::Utc::now().naive_utc() - Duration::hours(30);
        let chats = vec![Chat { business_id: 1, user_id: "573003579384".into(), last_user_message_timestamp: Some(old), last_user_message: None, is_important: false, last_guide_notification_at: None }];
        let (service, _, _, meta) = build_service(chats);
        let result = service
            .send_free_media_message(1, "573003579384", MediaType::Image, None, "foto.jpg".into(), "image/jpeg".into(), vec![1, 2], false)
            .await;
        assert!(result.unwrap_err().contains("window is closed"));
        assert!(meta.uploaded.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn send_media_with_open_window_uploads_and_persists() {
        let recent = chrono::Utc::now().naive_utc() - Duration::hours(1);
        let chats = vec![Chat { business_id: 1, user_id: "573003579384".into(), last_user_message_timestamp: Some(recent), last_user_message: None, is_important: false, last_guide_notification_at: None }];
        let (service, message_repo, _, meta) = build_service(chats);

        let message = service
            .send_free_media_message(
                1,
                "573003579384",
                MediaType::Document,
                Some("factura".into()),
                "factura.pdf".into(),
                "application/pdf".into(),
                vec![1, 2, 3, 4],
                false,
            )
            .await
            .unwrap();

        assert_eq!(message.meta_message_id, "wamid.fake-media");
        assert_eq!(message.media_type, MediaType::Document);
        assert_eq!(message.message, Some("factura".to_string()));
        assert_eq!(meta.uploaded.lock().unwrap().len(), 1);
        assert_eq!(meta.sent_media.lock().unwrap().len(), 1);
        assert_eq!(meta.sent_media.lock().unwrap()[0].2, false);
        assert_eq!(message_repo.saved.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn send_media_rejects_invalid_mime_without_calling_meta() {
        let recent = chrono::Utc::now().naive_utc() - Duration::hours(1);
        let chats = vec![Chat { business_id: 1, user_id: "573003579384".into(), last_user_message_timestamp: Some(recent), last_user_message: None, is_important: false, last_guide_notification_at: None }];
        let (service, _, _, meta) = build_service(chats);

        let result = service
            .send_free_media_message(1, "573003579384", MediaType::Image, None, "video.mp4".into(), "video/mp4".into(), vec![1], false)
            .await;
        assert!(result.is_err());
        assert!(meta.uploaded.lock().unwrap().is_empty());
    }

    #[test]
    fn voice_input_suffix_covers_browsers() {
        assert_eq!(voice_input_suffix("audio/webm;codecs=opus"), ".webm");
        assert_eq!(voice_input_suffix("video/webm"), ".webm");
        assert_eq!(voice_input_suffix("audio/mp4"), ".m4a");
        assert_eq!(voice_input_suffix("audio/mpeg"), ".mp3");
        assert_eq!(voice_input_suffix("audio/ogg"), ".ogg");
    }

    #[test]
    fn ffmpeg_voice_args_produce_mono_opus_ogg() {
        let args = ffmpeg_voice_args("/tmp/in.webm", "/tmp/out.ogg");
        assert!(args.contains(&"-ac".to_string()));
        assert!(args.contains(&"1".to_string()));
        assert!(args.contains(&"libopus".to_string()));
        assert!(args.contains(&"ogg".to_string()));
        assert_eq!(args.last().unwrap(), "/tmp/out.ogg");
    }

    #[tokio::test]
    async fn send_voice_note_transcodes_and_sends_with_voice_flag() {
        let recent = chrono::Utc::now().naive_utc() - Duration::hours(1);
        let chats = vec![Chat { business_id: 1, user_id: "573003579384".into(), last_user_message_timestamp: Some(recent), last_user_message: None, is_important: false, last_guide_notification_at: None }];
        let transcoder = Arc::new(FakeVoiceTranscoder::ok());
        let (service, message_repo, _, meta) = build_service_with_transcoder(chats, transcoder.clone());

        let message = service
            .send_free_media_message(
                1,
                "573003579384",
                MediaType::Audio,
                None,
                "audio.webm".into(),
                "audio/webm".into(),
                vec![1, 2, 3],
                true,
            )
            .await
            .unwrap();

        // Se llamó al transcodificador con el MIME original del navegador.
        assert_eq!(transcoder.calls.lock().unwrap().as_slice(), ["audio/webm"]);
        // Lo subido a Meta ya es OGG normalizado, no el webm original.
        assert_eq!(meta.uploaded.lock().unwrap().as_slice(), [(VOICE_FILENAME.to_string(), VOICE_MIME.to_string())]);
        // El envío a Meta lleva voice=true y el mensaje guardado no tiene caption.
        assert_eq!(meta.sent_media.lock().unwrap()[0].2, true);
        assert_eq!(message.media_type, MediaType::Audio);
        assert_eq!(message.message, None);
        assert_eq!(message_repo.saved.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn send_voice_note_transcode_failure_does_not_call_meta() {
        let recent = chrono::Utc::now().naive_utc() - Duration::hours(1);
        let chats = vec![Chat { business_id: 1, user_id: "573003579384".into(), last_user_message_timestamp: Some(recent), last_user_message: None, is_important: false, last_guide_notification_at: None }];
        let (service, _, _, meta) = build_service_with_transcoder(chats, Arc::new(FakeVoiceTranscoder::failing()));

        let result = service
            .send_free_media_message(1, "573003579384", MediaType::Audio, None, "audio.webm".into(), "audio/webm".into(), vec![1, 2], true)
            .await;
        assert!(result.is_err());
        assert!(meta.uploaded.lock().unwrap().is_empty());
        assert!(meta.sent_media.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn send_voice_flag_with_non_audio_fails() {
        let recent = chrono::Utc::now().naive_utc() - Duration::hours(1);
        let chats = vec![Chat { business_id: 1, user_id: "573003579384".into(), last_user_message_timestamp: Some(recent), last_user_message: None, is_important: false, last_guide_notification_at: None }];
        let (service, _, _, meta) = build_service(chats);

        let result = service
            .send_free_media_message(1, "573003579384", MediaType::Image, None, "foto.jpg".into(), "image/jpeg".into(), vec![1], true)
            .await;
        assert_eq!(result.unwrap_err(), "Voice messages must be audio");
        assert!(meta.uploaded.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn send_plain_audio_does_not_transcode() {
        let recent = chrono::Utc::now().naive_utc() - Duration::hours(1);
        let chats = vec![Chat { business_id: 1, user_id: "573003579384".into(), last_user_message_timestamp: Some(recent), last_user_message: None, is_important: false, last_guide_notification_at: None }];
        let transcoder = Arc::new(FakeVoiceTranscoder::ok());
        let (service, _, _, meta) = build_service_with_transcoder(chats, transcoder.clone());

        service
            .send_free_media_message(1, "573003579384", MediaType::Audio, None, "musica.mp3".into(), "audio/mpeg".into(), vec![1, 2], false)
            .await
            .unwrap();

        assert!(transcoder.calls.lock().unwrap().is_empty());
        assert_eq!(meta.uploaded.lock().unwrap().as_slice(), [("musica.mp3".to_string(), "audio/mpeg".to_string())]);
        assert_eq!(meta.sent_media.lock().unwrap()[0].2, false);
    }
}
