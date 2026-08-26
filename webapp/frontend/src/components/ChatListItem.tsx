"use client";

import { useState } from "react";
import { setChatImportance } from "@/lib/api";
import type { ChatSummary } from "@/lib/types";
import { remainingGrace } from "@/lib/time";

interface Props {
  chat: ChatSummary;
  selected: boolean;
  onSelect: () => void;
  onImportanceChange: (userPhone: string, isImportant: boolean) => void;
}

export default function ChatListItem({ chat, selected, onSelect, onImportanceChange }: Props) {
  const grace = remainingGrace(chat.last_user_message_timestamp, new Date());
  const [toggling, setToggling] = useState(false);

  async function toggleImportance(e: React.MouseEvent) {
    e.stopPropagation(); // no seleccionar el chat al marcar importancia
    if (toggling) return;

    setToggling(true);
    const next = !chat.is_important;
    onImportanceChange(chat.user_id, next); // update optimista
    try {
      await setChatImportance(chat.business_id, chat.user_id, next);
    } catch {
      onImportanceChange(chat.user_id, chat.is_important); // revertir
    } finally {
      setToggling(false);
    }
  }

  return (
    <button
      onClick={onSelect}
      className={`block w-full px-4 py-3 text-left transition-colors ${
        selected
          ? "bg-gradient-to-r from-white/25 via-white/10 to-transparent"
          : "hover:bg-white/5"
      }`}
    >
      <div className="flex items-baseline gap-2">
        {/* Icono de importancia: ○ vacío = normal, ! relleno blanco = importante */}
        <span
          role="button"
          title={chat.is_important ? "Quitar importancia" : "Marcar como importante"}
          onClick={toggleImportance}
          className={`shrink-0 cursor-pointer select-none text-base leading-none transition-all ${
            chat.is_important
              ? "font-bold text-white"
              : "text-moon/60 hover:text-moon"
          }`}
        >
          {chat.is_important ? "!" : "○"}
        </span>

        {/* Nombre y tiempo de gracia restante */}
        <span className="truncate font-medium text-white">
          {chat.user_full_name || chat.user_id}
        </span>
        <span
          className={`ml-auto shrink-0 text-xs ${
            grace.open ? "text-moon" : "text-neutral-600"
          }`}
        >
          {grace.label}
        </span>
      </div>

      {/* Último mensaje enviado por el usuario, en la parte inferior */}
      <p className="mt-1 truncate pl-6 text-sm text-neutral-400">
        {chat.last_user_message ?? "Sin mensajes del usuario"}
      </p>
    </button>
  );
}
