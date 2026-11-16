//! Transcribe un archivo de audio con la misma configuración y el mismo
//! cliente que usa el agente (`GOOGLE_AI_API_KEY` y `GEMINI_*` del entorno
//! o del `.env`). Sirve para probar Gemini sin pasar por WhatsApp.
//!
//! ```bash
//! cargo run --example transcribe_audio -- nota.ogg [vocabulario...]
//! ```

use ai_agents::config::{Env, ProcessEnv, models};
use ai_agents::transcription::gemini::GeminiTranscriber;
use ai_agents::transcription::{AudioClip, Transcriber, TranscriptionHints};

const USAGE: &str = "uso: cargo run --example transcribe_audio -- <archivo> [vocabulario...]";

#[tokio::main]
async fn main() {
    ai_agents::config::load_dotenv();
    let mut args = std::env::args().skip(1);
    let path = args.next().expect(USAGE);
    let vocabulary: Vec<String> = args.collect();

    let config = models::gemini(&Env::new(&ProcessEnv)).expect("configuración de Gemini inválida");
    let transcriber = GeminiTranscriber::new(config).expect("no se pudo crear el cliente");
    let clip =
        AudioClip { bytes: std::fs::read(&path).expect("no se pudo leer el archivo"), mime_type: mime_type(&path) };

    match transcriber.transcribe(&clip, &TranscriptionHints { vocabulary }).await {
        Ok(text) => println!("{text}"),
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    }
}

fn mime_type(path: &str) -> String {
    let extension = path.rsplit('.').next().unwrap_or_default().to_lowercase();
    match extension.as_str() {
        "mp3" => "audio/mpeg",
        "m4a" | "mp4" => "audio/mp4",
        "wav" => "audio/wav",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        _ => "audio/ogg",
    }
    .to_string()
}
