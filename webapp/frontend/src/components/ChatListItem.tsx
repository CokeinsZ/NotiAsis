"use client";

import type { ChatSummary } from "@/lib/types";
import { remainingGrace } from "@/lib/time";

interface Props {
  chat: ChatSummary;
  selected: boolean;
  onSelect: () => void;
}

export default function ChatListItem({ chat, selected, onSelect }: Props) {
  const grace = remainingGrace(chat.last_user_message_timestamp, new Date());

  return (
    <button
      onClick={onSelect}
      className={`block w-full px-4 py-3 text-left transition-colors ${
        selected
          ? "bg-gradient-to-r from-white/25 via-white/10 to-transparent"
          : "hover:bg-white/5"
      }`}
    >
      {/* Nombre y tiempo de gracia restante */}
      <div className="flex items-baseline justify-between gap-2">
        <span className="truncate font-medium text-white">
          {chat.user_full_name || chat.user_id}
        </span>
        <span
          className={`shrink-0 text-xs ${
            grace.open ? "text-moon" : "text-neutral-600"
          }`}
        >
          {grace.label}
        </span>
      </div>

      {/* Último mensaje enviado por el usuario, en la parte inferior */}
      <p className="mt-1 truncate text-sm text-neutral-400">
        {chat.last_user_message ?? "Sin mensajes del usuario"}
      </p>
    </button>
  );
}
