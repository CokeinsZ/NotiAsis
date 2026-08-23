"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { getChats } from "@/lib/api";
import { clearToken, getValidClaims } from "@/lib/auth";
import type { ChatSummary } from "@/lib/types";
import { parseUtc } from "@/lib/time";
import ChatListItem from "./ChatListItem";
import ChatWindow from "./ChatWindow";

const LIST_POLL_MS = 15000;
const TICK_MS = 30000; // re-render para actualizar los tiempos de gracia

export default function ChatApp({ businessId }: { businessId: number }) {
  const router = useRouter();
  const [authorized, setAuthorized] = useState(false);
  const [chats, setChats] = useState<ChatSummary[]>([]);
  const [selectedPhone, setSelectedPhone] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [, setTick] = useState(0);

  // Guard: sin JWT válido -> login; JWT de otro business -> su propio panel.
  useEffect(() => {
    const claims = getValidClaims();
    if (!claims) {
      router.replace("/login");
      return;
    }
    if (claims.kind === "associate" && claims.business_id !== businessId) {
      router.replace(claims.business_id ? `/chats/${claims.business_id}` : "/login");
      return;
    }
    setAuthorized(true);
  }, [businessId, router]);

  function handleLogout() {
    clearToken();
    router.replace("/login");
  }

  const refreshChats = useCallback(async () => {
    try {
      setChats(await getChats(businessId));
      setError(null);
    } catch {
      setError("No se pudieron cargar las conversaciones.");
    }
  }, [businessId]);

  useEffect(() => {
    if (!authorized) return;
    refreshChats();
    const poll = setInterval(refreshChats, LIST_POLL_MS);
    const tick = setInterval(() => setTick((t) => t + 1), TICK_MS);
    return () => {
      clearInterval(poll);
      clearInterval(tick);
    };
  }, [authorized, refreshChats]);

  // Orden: último mensaje DEL USUARIO más reciente primero; los chats sin
  // mensajes del usuario van al final (por última actividad).
  const sortedChats = useMemo(() => {
    return [...chats].sort((a, b) => {
      const aTs = a.last_user_message_timestamp ?? a.last_activity;
      const bTs = b.last_user_message_timestamp ?? b.last_activity;
      if (!aTs && !bTs) return 0;
      if (!aTs) return 1;
      if (!bTs) return -1;
      return parseUtc(bTs).getTime() - parseUtc(aTs).getTime();
    });
  }, [chats]);

  // Auto-seleccionar el primer chat si no hay selección válida
  useEffect(() => {
    if (sortedChats.length === 0) return;
    if (!selectedPhone || !sortedChats.some((c) => c.user_id === selectedPhone)) {
      setSelectedPhone(sortedChats[0].user_id);
    }
  }, [sortedChats, selectedPhone]);

  const selectedChat = sortedChats.find((c) => c.user_id === selectedPhone) ?? null;

  if (!authorized) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-black">
        <p className="text-sm text-neutral-500">Cargando...</p>
      </main>
    );
  }

  return (
    <div className="flex h-screen bg-black">
      {/* Panel izquierdo: rectángulo vertical delgado con la lista de chats */}
      <aside className="flex w-80 shrink-0 flex-col border-r border-moon/35">
        <header className="flex items-center justify-between border-b border-moon/35 px-4 py-4">
          <div>
            <h1 className="text-lg font-semibold tracking-widest">NotiAsis</h1>
            <p className="mt-0.5 text-xs text-moon/70">Conversaciones</p>
          </div>
          <button
            onClick={handleLogout}
            className="text-xs text-neutral-500 transition-colors hover:text-moon"
          >
            Salir
          </button>
        </header>

        <div className="flex-1 divide-y divide-moon/25 overflow-y-auto">
          {sortedChats.map((chat) => (
            <ChatListItem
              key={chat.user_id}
              chat={chat}
              selected={chat.user_id === selectedPhone}
              onSelect={() => setSelectedPhone(chat.user_id)}
            />
          ))}

          {sortedChats.length === 0 && !error && (
            <p className="px-4 py-6 text-sm text-neutral-500">
              Aún no hay conversaciones.
            </p>
          )}
          {error && (
            <p className="px-4 py-6 text-sm text-moon">{error}</p>
          )}
        </div>
      </aside>

      {/* Panel derecho: historial del chat seleccionado */}
      <main className="flex min-w-0 flex-1 flex-col">
        {selectedChat ? (
          <ChatWindow businessId={businessId} chat={selectedChat} />
        ) : (
          <div className="flex flex-1 items-center justify-center">
            <p className="text-sm text-neutral-500">
              Selecciona una conversación
            </p>
          </div>
        )}
      </main>
    </div>
  );
}
