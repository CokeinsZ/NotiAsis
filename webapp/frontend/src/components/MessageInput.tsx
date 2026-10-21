"use client";

import { useEffect, useRef, useState } from "react";
import { ApiError, sendChatMedia, sendChatMessage, type SendableMediaType } from "@/lib/api";

interface Props {
  businessId: number;
  userPhone: string;
  windowOpen: boolean;
  onSent: () => void;
}

const MAX_IMAGE_BYTES = 5 * 1024 * 1024;
const MAX_VIDEO_BYTES = 16 * 1024 * 1024;
const MAX_AUDIO_BYTES = 16 * 1024 * 1024;
const MAX_DOCUMENT_BYTES = 25 * 1024 * 1024;

const IMAGE_ACCEPT = "image/jpeg,image/png,image/webp";
const VIDEO_ACCEPT = "video/mp4,video/3gpp,video/3gpp2,video/quicktime";
const DOCUMENT_ACCEPT =
  ".pdf,.txt,.csv,.xls,.xlsx,.doc,.docx,.ppt,.pptx,application/pdf,text/plain,text/csv,application/vnd.ms-excel,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,application/msword,application/vnd.openxmlformats-officedocument.wordprocessingml.document";

const RECORDER_MIME_CANDIDATES = [
  "audio/ogg;codecs=opus",
  "audio/webm;codecs=opus",
  "audio/webm",
  "audio/mp4",
  "audio/ogg",
];

function pickRecorderMime(): string {
  if (typeof MediaRecorder === "undefined" || !MediaRecorder.isTypeSupported) return "";
  return RECORDER_MIME_CANDIDATES.find((m) => MediaRecorder.isTypeSupported(m)) ?? "";
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatElapsed(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function validateFile(kind: SendableMediaType, file: File): string | null {
  const mime = (file.type || "").toLowerCase();
  if (file.size === 0) return "El archivo está vacío.";
  if (kind === "image") {
    if (!["image/jpeg", "image/png", "image/webp"].includes(mime)) {
      return "Formato de imagen no soportado. Usa JPG, PNG o WEBP.";
    }
    if (file.size > MAX_IMAGE_BYTES) return "La imagen supera los 5MB permitidos.";
  } else if (kind === "video") {
    if (!["video/mp4", "video/3gpp", "video/3gpp2", "video/quicktime"].includes(mime)) {
      return "Formato de video no soportado. Usa MP4.";
    }
    if (file.size > MAX_VIDEO_BYTES) return "El video supera los 16MB permitidos.";
  } else if (kind === "audio") {
    const ok =
      mime.startsWith("audio/") || mime === "video/webm" || mime === "application/ogg" || mime === "";
    if (!ok) return "Formato de audio no soportado.";
    if (file.size > MAX_AUDIO_BYTES) return "El audio supera los 16MB permitidos.";
  } else {
    if (file.size > MAX_DOCUMENT_BYTES) return "El documento supera los 25MB permitidos.";
  }
  return null;
}

export default function MessageInput({ businessId, userPhone, windowOpen, onSent }: Props) {
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [attachOpen, setAttachOpen] = useState(false);
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [selectedKind, setSelectedKind] = useState<SendableMediaType | null>(null);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);

  const [isRecording, setIsRecording] = useState(false);
  const [recordSeconds, setRecordSeconds] = useState(0);
  const [recordedBlob, setRecordedBlob] = useState<Blob | null>(null);
  const [recordedUrl, setRecordedBlobUrl] = useState<string | null>(null);
  const [recordedMime, setRecordedMime] = useState<string>("");

  const imageInputRef = useRef<HTMLInputElement>(null);
  const videoInputRef = useRef<HTMLInputElement>(null);
  const documentInputRef = useRef<HTMLInputElement>(null);
  const recorderRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const streamRef = useRef<MediaStream | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Liberar blob URLs al cambiar o desmontar
  useEffect(() => {
    return () => {
      if (previewUrl) URL.revokeObjectURL(previewUrl);
    };
  }, [previewUrl]);

  useEffect(() => {
    return () => {
      if (recordedUrl) URL.revokeObjectURL(recordedUrl);
      if (timerRef.current) clearInterval(timerRef.current);
      recorderRef.current?.stream.getTracks().forEach((t) => t.stop());
      streamRef.current?.getTracks().forEach((t) => t.stop());
    };
  }, [recordedUrl]);

  function clearAttachment() {
    if (previewUrl) URL.revokeObjectURL(previewUrl);
    setSelectedFile(null);
    setSelectedKind(null);
    setPreviewUrl(null);
    setAttachOpen(false);
    if (imageInputRef.current) imageInputRef.current.value = "";
    if (videoInputRef.current) videoInputRef.current.value = "";
    if (documentInputRef.current) documentInputRef.current.value = "";
  }

  function clearRecording() {
    if (recordedUrl) URL.revokeObjectURL(recordedUrl);
    setRecordedBlob(null);
    setRecordedBlobUrl(null);
    setRecordedMime("");
    setRecordSeconds(0);
  }

  function handleFilePicked(kind: SendableMediaType, file: File | undefined) {
    if (!file) return;
    const validationError = validateFile(kind, file);
    if (validationError) {
      setError(validationError);
      return;
    }
    if (previewUrl) URL.revokeObjectURL(previewUrl);
    clearRecording();
    setError(null);
    setSelectedFile(file);
    setSelectedKind(kind);
    setAttachOpen(false);
    if (kind === "image" || kind === "video") {
      setPreviewUrl(URL.createObjectURL(file));
    } else {
      setPreviewUrl(null);
    }
  }

  async function startRecording() {
    if (isRecording || sending || !windowOpen) return;
    setError(null);
    try {
      if (typeof navigator === "undefined" || !navigator.mediaDevices?.getUserMedia) {
        setError("Tu navegador no permite grabar audio.");
        return;
      }
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      streamRef.current = stream;
      const mimeType = pickRecorderMime();
      const recorder = mimeType ? new MediaRecorder(stream, { mimeType }) : new MediaRecorder(stream);
      chunksRef.current = [];
      recorderRef.current = recorder;

      recorder.ondataavailable = (e) => {
        if (e.data.size > 0) chunksRef.current.push(e.data);
      };
      recorder.onstop = () => {
        const type = recorder.mimeType || mimeType || "audio/webm";
        const blob = new Blob(chunksRef.current, { type });
        chunksRef.current = [];
        stream.getTracks().forEach((t) => t.stop());
        if (timerRef.current) clearInterval(timerRef.current);
        setIsRecording(false);
        if (blob.size === 0) {
          setError("La grabación quedó vacía, intenta de nuevo.");
          return;
        }
        if (blob.size > MAX_AUDIO_BYTES) {
          setError("El audio supera los 16MB permitidos.");
          return;
        }
        if (recordedUrl) URL.revokeObjectURL(recordedUrl);
        clearAttachment();
        setRecordedBlob(blob);
        setRecordedMime(type);
        setRecordedBlobUrl(URL.createObjectURL(blob));
      };

      recorder.start();
      setIsRecording(true);
      setRecordSeconds(0);
      timerRef.current = setInterval(() => setRecordSeconds((s) => s + 1), 1000);
    } catch {
      setError("No se pudo acceder al micrófono. Revisa los permisos del navegador.");
    }
  }

  function stopRecording() {
    recorderRef.current?.stop();
  }

  function cancelRecording() {
    if (isRecording) {
      // Descarta los chunks al detener
      const recorder = recorderRef.current;
      if (recorder) {
        recorder.onstop = () => {
          chunksRef.current = [];
          streamRef.current?.getTracks().forEach((t) => t.stop());
          if (timerRef.current) clearInterval(timerRef.current);
          setIsRecording(false);
          setRecordSeconds(0);
        };
        recorder.stop();
      }
    } else {
      clearRecording();
    }
  }

  const hasAttachment = selectedFile !== null && selectedKind !== null;
  const hasRecording = recordedBlob !== null;
  const canSend = windowOpen && !sending && !isRecording && (text.trim().length > 0 || hasAttachment || hasRecording);

  async function handleSend() {
    if (!canSend) return;
    setSending(true);
    setError(null);
    try {
      if (hasAttachment && selectedFile && selectedKind) {
        await sendChatMedia(businessId, userPhone, selectedFile, selectedKind, text.trim() || undefined, selectedFile.name);
      } else if (hasRecording && recordedBlob) {
        const ext = recordedMime.includes("mp4") ? "m4a" : recordedMime.includes("ogg") ? "ogg" : "webm";
        // Las grabaciones se envían como nota de voz: el backend las
        // convierte a OGG/Opus mono (lo único que las apps nativas
        // reproducen como PTT) y las manda con voice:true.
        await sendChatMedia(businessId, userPhone, recordedBlob, "audio", undefined, `audio.${ext}`, { voice: true });
      } else {
        await sendChatMessage(businessId, userPhone, text.trim());
      }
      setText("");
      clearAttachment();
      clearRecording();
      onSent();
    } catch (e) {
      if (e instanceof ApiError && e.status === 422) {
        setError("La ventana de 24h está cerrada. Solo puedes enviar plantillas.");
      } else if (e instanceof ApiError) {
        setError(e.message || "No se pudo enviar el mensaje.");
      } else {
        setError("No se pudo enviar el mensaje.");
      }
    } finally {
      setSending(false);
    }
  }

  const captionPlaceholder = hasAttachment
    ? selectedKind === "audio"
      ? "Audio seleccionado"
      : "Agrega un texto (opcional)..."
    : windowOpen
      ? "Escribe un mensaje..."
      : "Ventana cerrada";

  return (
    <footer className="border-t border-moon/35 px-5 py-4">
      {!windowOpen && (
        <p className="mb-2 text-xs text-neutral-500">
          La ventana de 24h está cerrada: solo se pueden enviar plantillas.
        </p>
      )}
      {error && <p className="mb-2 text-xs text-moon">{error}</p>}

      {/* Vista previa del adjunto */}
      {hasAttachment && selectedFile && selectedKind && (
        <div className="mb-3 flex items-start gap-3 rounded border border-moon/40 bg-neutral-900/60 p-3">
          {selectedKind === "image" && previewUrl ? (
            // eslint-disable-next-line @next/next/no-img-element
            <img src={previewUrl} alt="Vista previa" className="h-16 w-16 rounded object-cover" />
          ) : selectedKind === "video" && previewUrl ? (
            <video src={previewUrl} className="h-16 w-24 rounded object-cover" muted />
          ) : (
            <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded border border-moon/40 text-lg text-moon">
              📄
            </div>
          )}
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm text-white">{selectedFile.name}</p>
            <p className="text-xs text-neutral-500">
              {selectedKind} · {formatBytes(selectedFile.size)}
            </p>
          </div>
          <button
            onClick={clearAttachment}
            disabled={sending}
            title="Quitar archivo"
            className="shrink-0 text-neutral-500 transition-colors hover:text-moon"
          >
            ✕
          </button>
        </div>
      )}

      {/* Grabación en curso */}
      {isRecording && (
        <div className="mb-3 flex items-center gap-3 rounded border border-red-500/50 bg-red-950/30 p-3">
          <span className="h-3 w-3 animate-pulse rounded-full bg-red-500" />
          <span className="text-sm text-white">Grabando {formatElapsed(recordSeconds)}</span>
          <div className="ml-auto flex gap-2">
            <button
              onClick={cancelRecording}
              className="rounded border border-neutral-600 px-3 py-1 text-xs text-neutral-300 hover:text-white"
            >
              Cancelar
            </button>
            <button
              onClick={stopRecording}
              className="rounded border border-moon/60 px-3 py-1 text-xs text-moon hover:text-white"
            >
              Detener
            </button>
          </div>
        </div>
      )}

      {/* Audio grabado listo para enviar */}
      {!isRecording && hasRecording && recordedUrl && (
        <div className="mb-3 flex items-center gap-3 rounded border border-moon/40 bg-neutral-900/60 p-3">
          <audio controls src={recordedUrl} className="h-9 max-w-full flex-1" />
          <span className="shrink-0 text-xs text-neutral-500">{formatElapsed(recordSeconds)}</span>
          <button
            onClick={clearRecording}
            disabled={sending}
            title="Descartar audio"
            className="shrink-0 text-neutral-500 transition-colors hover:text-moon"
          >
            ✕
          </button>
        </div>
      )}

      <div className="flex items-center gap-2">
        {/* Adjuntar */}
        <div className="relative">
          <button
            onClick={() => setAttachOpen((v) => !v)}
            disabled={!windowOpen || sending || isRecording}
            title="Adjuntar archivo"
            className="rounded border border-moon/40 px-3 py-2 text-sm text-moon transition-colors hover:bg-white/10 hover:text-white disabled:cursor-not-allowed disabled:opacity-30"
          >
            📎
          </button>
          {attachOpen && (
            <div className="absolute bottom-full left-0 z-10 mb-2 w-44 overflow-hidden rounded border border-moon/40 bg-black shadow-xl">
              <button
                onClick={() => imageInputRef.current?.click()}
                className="block w-full px-4 py-2 text-left text-sm text-neutral-200 hover:bg-white/10 hover:text-white"
              >
                🖼️ Imagen
              </button>
              <button
                onClick={() => videoInputRef.current?.click()}
                className="block w-full px-4 py-2 text-left text-sm text-neutral-200 hover:bg-white/10 hover:text-white"
              >
                🎬 Video
              </button>
              <button
                onClick={() => documentInputRef.current?.click()}
                className="block w-full px-4 py-2 text-left text-sm text-neutral-200 hover:bg-white/10 hover:text-white"
              >
                📄 Documento
              </button>
            </div>
          )}
        </div>

        {/* Grabar audio */}
        {!hasRecording ? (
          <button
            onClick={startRecording}
            disabled={!windowOpen || sending || isRecording || hasAttachment}
            title="Grabar audio"
            className="rounded border border-moon/40 px-3 py-2 text-sm text-moon transition-colors hover:bg-white/10 hover:text-white disabled:cursor-not-allowed disabled:opacity-30"
          >
            🎙️
          </button>
        ) : null}

        <input
          type="text"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSend()}
          disabled={!windowOpen || sending || isRecording || hasRecording}
          placeholder={hasRecording ? "El audio se envía sin texto" : captionPlaceholder}
          className="flex-1 rounded border border-moon/35 bg-black px-4 py-2 text-sm text-white placeholder-neutral-600 outline-none focus:border-moon disabled:opacity-40"
        />
        <button
          onClick={handleSend}
          disabled={!canSend}
          className="rounded border border-moon/60 px-5 py-2 text-sm font-medium text-moon transition-all hover:bg-gradient-to-r hover:from-white/20 hover:to-transparent hover:text-white disabled:cursor-not-allowed disabled:opacity-30"
        >
          {sending ? "..." : "Enviar"}
        </button>
      </div>

      {/* Inputs ocultos */}
      <input
        ref={imageInputRef}
        type="file"
        accept={IMAGE_ACCEPT}
        className="hidden"
        onChange={(e) => handleFilePicked("image", e.target.files?.[0])}
      />
      <input
        ref={videoInputRef}
        type="file"
        accept={VIDEO_ACCEPT}
        className="hidden"
        onChange={(e) => handleFilePicked("video", e.target.files?.[0])}
      />
      <input
        ref={documentInputRef}
        type="file"
        accept={DOCUMENT_ACCEPT}
        className="hidden"
        onChange={(e) => handleFilePicked("document", e.target.files?.[0])}
      />
    </footer>
  );
}
