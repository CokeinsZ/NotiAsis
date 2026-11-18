"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getChatAiActivity, getChatMessages } from "@/lib/api";
import type { ChatAiActivity, ChatSummary, Message } from "@/lib/types";
import { remainingGrace } from "@/lib/time";
import { buildTimeline, type TimelineEntry } from "@/lib/timeline";
import DaySeparator from "./DaySeparator";
import EscalationNote from "./EscalationNote";
import GuideUpdateNote from "./GuideUpdateNote";
import MessageBubble from "./MessageBubble";
import MessageInput from "./MessageInput";

const MESSAGES_POLL_MS = 5000;
const NO_AI_ACTIVITY: ChatAiActivity = { escalations: [], guide_updates: [] };

interface Props {
  businessId: number;
  chat: ChatSummary;
}

function TimelineRow({ entry }: { entry: TimelineEntry }) {
  switch (entry.type) {
    case "message":
      return <MessageBubble message={entry.message} />;
    case "guide_update":
      return <GuideUpdateNote update={entry.update} />;
    case "escalation":
      return <EscalationNote escalation={entry.escalation} />;
  }
}

export default function ChatWindow({ businessId, chat }: Props) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [aiActivity, setAiActivity] = useState<ChatAiActivity>(NO_AI_ACTIVITY);
  const [error, setError] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  const refreshMessages = useCallback(async () => {
    const [messagesResult, activityResult] = await Promise.allSettled([
      getChatMessages(businessId, chat.user_id),
      getChatAiActivity(businessId, chat.user_id),
    ]);

    if (messagesResult.status === "fulfilled") {
      setMessages(messagesResult.value);
      setError(null);
    } else {
      setError("No se pudieron cargar los mensajes.");
    }
    // Las notas y escalaciones de la IA complementan el historial: si
    // fallan, los mensajes se siguen viendo (con las últimas que llegaron).
    if (activityResult.status === "fulfilled") {
      setAiActivity(activityResult.value);
    }
  }, [businessId, chat.user_id]);

  useEffect(() => {
    setMessages([]);
    setAiActivity(NO_AI_ACTIVITY);
    refreshMessages();
    const poll = setInterval(refreshMessages, MESSAGES_POLL_MS);
    return () => clearInterval(poll);
  }, [refreshMessages]);

  const days = useMemo(() => buildTimeline(messages, aiActivity), [messages, aiActivity]);
  const entryCount = days.reduce((count, day) => count + day.entries.length, 0);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [entryCount]);

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

      {/* Historial: mensajes, notas y escalaciones de la IA, agrupados por día */}
      {/* El padding va en el contenedor interno: sticky respeta el del que
          hace scroll, y el día quedaría despegado del borde superior. */}
      <div className="flex-1 overflow-y-auto">
        <div className="space-y-2 px-5 pt-8 pb-4">
          {/* Una sección por día: el separador se queda arriba solo mientras se ven los mensajes de su día */}
          {days.map((day) => (
            <section key={day.key} className="space-y-2">
              <DaySeparator label={day.label} />
              {day.entries.map((entry) => (
                <TimelineRow key={entry.key} entry={entry} />
              ))}
            </section>
          ))}

          {days.length === 0 && !error && (
            <p className="pt-10 text-center text-sm text-neutral-600">
              Sin mensajes todavía
            </p>
          )}
          {error && (
            <p className="pt-10 text-center text-sm text-moon">{error}</p>
          )}
          <div ref={bottomRef} />
        </div>
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
