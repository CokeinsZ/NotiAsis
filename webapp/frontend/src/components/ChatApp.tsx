"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { getBusiness, getChats } from "@/lib/api";
import { clearToken, getValidClaims } from "@/lib/auth";
import type { ChatSummary } from "@/lib/types";
import { parseUtc } from "@/lib/time";
import ChatListItem from "./ChatListItem";
import ChatWindow from "./ChatWindow";

const LIST_POLL_MS = 15000;
const TICK_MS = 30000; // re-render para actualizar los tiempos de gracia
const RECENT_MS = 24 * 60 * 60 * 1000;

type ChatFilter = "important" | "recent_replies" | "recent_notifications";

const FILTER_LABELS: Record<ChatFilter, string> = {
  important: "Importantes",
  recent_replies: "Respuestas recientes",
  recent_notifications: "Notificaciones recientes",
};

export default function ChatApp({ businessId }: { businessId: number }) {
  const router = useRouter();
  const [authorized, setAuthorized] = useState(false);
  const [businessName, setBusinessName] = useState("");
  const [chats, setChats] = useState<ChatSummary[]>([]);
  const [selectedPhone, setSelectedPhone] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchText, setSearchText] = useState("");
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [activeFilters, setActiveFilters] = useState<Set<ChatFilter>>(new Set());
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
    getBusiness(businessId)
      .then((b) => setBusinessName(b.name))
      .catch(() => setBusinessName(`Empresa ${businessId}`));
    const poll = setInterval(refreshChats, LIST_POLL_MS);
    const tick = setInterval(() => setTick((t) => t + 1), TICK_MS);
    return () => {
      clearInterval(poll);
      clearInterval(tick);
    };
  }, [authorized, businessId, refreshChats]);

  // El orden lo define el backend: importantes > respuesta reciente del
  // usuario > guía notificada recientemente.
  // Búsqueda y filtros se combinan: texto AND (cualquier filtro activo).
  const visibleChats = useMemo(() => {
    const now = Date.now();
    const query = searchText.trim().toLowerCase();

    return chats.filter((chat) => {
      if (query) {
        const matches =
          chat.user_full_name.toLowerCase().includes(query) ||
          chat.user_id.includes(query);
        if (!matches) return false;
      }

      if (activeFilters.size > 0) {
        const recentReply =
          chat.last_user_message_timestamp !== null &&
          now - parseUtc(chat.last_user_message_timestamp).getTime() < RECENT_MS;
        const recentNotification =
          chat.last_guide_notification_at !== null &&
          now - parseUtc(chat.last_guide_notification_at).getTime() < RECENT_MS;

        const matchesAny =
          (activeFilters.has("important") && chat.is_important) ||
          (activeFilters.has("recent_replies") && recentReply) ||
          (activeFilters.has("recent_notifications") && recentNotification);
        if (!matchesAny) return false;
      }

      return true;
    });
  }, [chats, searchText, activeFilters]);

  // Auto-seleccionar el primer chat visible si no hay selección válida
  useEffect(() => {
    if (visibleChats.length === 0) return;
    if (!selectedPhone || !visibleChats.some((c) => c.user_id === selectedPhone)) {
      setSelectedPhone(visibleChats[0].user_id);
    }
  }, [visibleChats, selectedPhone]);

  const selectedChat = chats.find((c) => c.user_id === selectedPhone) ?? null;

  function handleImportanceChange(userPhone: string, isImportant: boolean) {
    setChats((prev) => {
      const updated = prev.map((c) =>
        c.user_id === userPhone ? { ...c, is_important: isImportant } : c,
      );
      // Mantener el orden oficial localmente: importantes primero
      return [...updated].sort((a, b) => Number(b.is_important) - Number(a.is_important));
    });
  }

  function toggleFilter(filter: ChatFilter) {
    setActiveFilters((prev) => {
      const next = new Set(prev);
      if (next.has(filter)) {
        next.delete(filter);
      } else {
        next.add(filter);
      }
      return next;
    });
  }

  const hasActiveSearchOrFilters = searchText.trim().length > 0 || activeFilters.size > 0;

  function clearSearchAndFilters() {
    setSearchText("");
    setActiveFilters(new Set());
    setSearchOpen(false);
    setFiltersOpen(false);
  }

  if (!authorized) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-black">
        <p className="text-sm text-neutral-500">Cargando...</p>
      </main>
    );
  }

  return (
    <div className="flex h-screen bg-black">
      {/* Panel izquierdo: rectángulo vertical con la lista de chats */}
      <aside className="flex w-96 shrink-0 flex-col border-r border-moon/35">
        <header className="border-b border-moon/35 px-4 py-4">
          <div className="flex items-center justify-between">
            <div className="flex min-w-0 items-center gap-2">
              <h1 className="truncate text-lg font-semibold tracking-widest">
                {businessName || "NotiAsis"}
              </h1>
              <Link
                href={`/chats/${businessId}/dashboard`}
                title="Dashboard de notificaciones"
                className="shrink-0 text-neutral-500 transition-colors hover:text-moon"
              >
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
                  <line x1="4" y1="20" x2="4" y2="12" />
                  <line x1="10" y1="20" x2="10" y2="6" />
                  <line x1="16" y1="20" x2="16" y2="14" />
                  <line x1="22" y1="20" x2="22" y2="3" />
                </svg>
              </Link>
            </div>
            <button
              onClick={handleLogout}
              className="shrink-0 text-xs text-neutral-500 transition-colors hover:text-moon"
            >
              Salir
            </button>
          </div>

          {/* Búsqueda y filtros */}
          <div className="mt-3 flex items-center gap-3">
            <button
              onClick={() => setSearchOpen((v) => !v)}
              title="Buscar por nombre o teléfono"
              className={`transition-colors ${searchOpen || searchText ? "text-moon" : "text-neutral-500 hover:text-moon"}`}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
                <circle cx="11" cy="11" r="8" />
                <line x1="21" y1="21" x2="16.65" y2="16.65" />
              </svg>
            </button>
            <button
              onClick={() => setFiltersOpen((v) => !v)}
              title="Filtros"
              className={`transition-colors ${filtersOpen || activeFilters.size > 0 ? "text-moon" : "text-neutral-500 hover:text-moon"}`}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <polygon points="22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3" />
              </svg>
            </button>
            {hasActiveSearchOrFilters && (
              <button
                onClick={clearSearchAndFilters}
                className="ml-auto text-xs text-moon transition-colors hover:text-white"
              >
                Limpiar filtros/búsqueda
              </button>
            )}
          </div>

          {searchOpen && (
            <input
              type="text"
              value={searchText}
              onChange={(e) => setSearchText(e.target.value)}
              placeholder="Buscar por nombre o teléfono..."
              autoFocus
              className="mt-3 w-full rounded border border-moon/35 bg-black px-3 py-1.5 text-sm text-white placeholder-neutral-600 outline-none focus:border-moon"
            />
          )}

          {filtersOpen && (
            <div className="mt-3 flex flex-wrap gap-2">
              {(Object.keys(FILTER_LABELS) as ChatFilter[]).map((filter) => (
                <button
                  key={filter}
                  onClick={() => toggleFilter(filter)}
                  className={`rounded border px-2.5 py-1 text-xs transition-colors ${
                    activeFilters.has(filter)
                      ? "border-moon bg-white/15 text-white"
                      : "border-moon/35 text-neutral-400 hover:text-moon"
                  }`}
                >
                  {FILTER_LABELS[filter]}
                </button>
              ))}
            </div>
          )}
        </header>

        <div className="flex-1 divide-y divide-moon/25 overflow-y-auto">
          {visibleChats.map((chat) => (
            <ChatListItem
              key={chat.user_id}
              chat={chat}
              selected={chat.user_id === selectedPhone}
              onSelect={() => setSelectedPhone(chat.user_id)}
              onImportanceChange={handleImportanceChange}
            />
          ))}

          {visibleChats.length === 0 && !error && (
            <p className="px-4 py-6 text-sm text-neutral-500">
              {hasActiveSearchOrFilters
                ? "Ninguna conversación cumple la búsqueda/filtros."
                : "Aún no hay conversaciones."}
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
