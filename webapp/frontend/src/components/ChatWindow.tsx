"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { getChatMessages } from "@/lib/api";
import type { ChatSummary, Message } from "@/lib/types";
import { remainingGrace } from "@/lib/time";
import MessageBubble from "./MessageBubble";
import MessageInput from "./MessageInput";

const MESSAGES_POLL_MS = 5000;

interface Props {
  businessId: number;
  chat: ChatSummary;
}

export default function ChatWindow({ businessId, chat }: Props) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [error, setError] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  const refreshMessages = useCallback(async () => {
    try {
      setMessages(await getChatMessages(businessId, chat.user_id));
      setError(null);
    } catch {
      setError("No se pudieron cargar los mensajes.");
    }
  }, [businessId, chat.user_id]);

  useEffect(() => {
    setMessages([]);
    refreshMessages();
    const poll = setInterval(refreshMessages, MESSAGES_POLL_MS);
    return () => clearInterval(poll);
  }, [refreshMessages]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages.length]);

  const grace = remainingGrace(chat.last_user_message_timestamp, new Date());

  return (
    <>
      {/* Encabezado del chat */}
      <header className="flex items-baseline justify-between border-b border-moon/35 px-5 py-4">
        <div>
          <h2 className="font-medium text-white">
            {chat.user_full_name || chat.user_id}
          </h2>
          <p className="text-xs text-neutral-500">{chat.user_id}</p>
        </div>
        <span className={`text-xs ${grace.open ? "text-moon" : "text-neutral-600"}`}>
          {grace.label}
        </span>
      </header>

      {/* Historial de mensajes */}
      <div className="flex-1 space-y-2 overflow-y-auto px-5 py-4">
        {messages.map((message) => (
          <MessageBubble key={message.id} message={message} />
        ))}

        {messages.length === 0 && !error && (
          <p className="pt-10 text-center text-sm text-neutral-600">
            Sin mensajes todavía
          </p>
        )}
        {error && (
          <p className="pt-10 text-center text-sm text-moon">{error}</p>
        )}
        <div ref={bottomRef} />
      </div>

      {/* Envío de mensaje libre (ventana de 24h) */}
      <MessageInput
        businessId={businessId}
        userPhone={chat.user_id}
        windowOpen={grace.open}
        onSent={refreshMessages}
      />
    </>
  );
}
