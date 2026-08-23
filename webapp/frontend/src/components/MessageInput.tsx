"use client";

import { useState } from "react";
import { ApiError, sendChatMessage } from "@/lib/api";

interface Props {
  businessId: number;
  userPhone: string;
  windowOpen: boolean;
  onSent: () => void;
}

export default function MessageInput({ businessId, userPhone, windowOpen, onSent }: Props) {
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const canSend = windowOpen && text.trim().length > 0 && !sending;

  async function handleSend() {
    if (!canSend) return;

    setSending(true);
    setError(null);
    try {
      await sendChatMessage(businessId, userPhone, text.trim());
      setText("");
      onSent();
    } catch (e) {
      if (e instanceof ApiError && e.status === 422) {
        setError("La ventana de 24h está cerrada. Solo puedes enviar plantillas.");
      } else {
        setError("No se pudo enviar el mensaje.");
      }
    } finally {
      setSending(false);
    }
  }

  return (
    <footer className="border-t border-moon/35 px-5 py-4">
      {!windowOpen && (
        <p className="mb-2 text-xs text-neutral-500">
          La ventana de 24h está cerrada: solo se pueden enviar plantillas.
        </p>
      )}
      {error && <p className="mb-2 text-xs text-moon">{error}</p>}

      <div className="flex items-center gap-3">
        <input
          type="text"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSend()}
          disabled={!windowOpen || sending}
          placeholder={windowOpen ? "Escribe un mensaje..." : "Ventana cerrada"}
          className="flex-1 rounded border border-moon/35 bg-black px-4 py-2 text-sm text-white placeholder-neutral-600 outline-none focus:border-moon disabled:opacity-40"
        />
        <button
          onClick={handleSend}
          disabled={!canSend}
          className="rounded border border-moon/60 px-5 py-2 text-sm font-medium text-moon transition-all hover:bg-gradient-to-r hover:from-white/20 hover:to-transparent hover:text-white disabled:cursor-not-allowed disabled:opacity-30"
        >
          Enviar
        </button>
      </div>
    </footer>
  );
}
