"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { getBusiness, getNotificationStats } from "@/lib/api";
import { getValidClaims } from "@/lib/auth";
import type { DayStats, NotificationStatRow } from "@/lib/types";
import { parseUtc } from "@/lib/time";

const DAY_OPTIONS = [7, 30, 90];

const SERIES = [
  { key: "initial", label: "Notificación inicial", color: "bg-moon" },
  { key: "reminder", label: "Recordatorio", color: "bg-moon/50" },
  { key: "final", label: "Recordatorio final", color: "bg-white" },
] as const;

/** Pivota las filas (day × notification_count) y rellena los días sin datos. */
function pivotStats(rows: NotificationStatRow[], days: number): DayStats[] {
  const byDay = new Map<string, DayStats>();
  const today = new Date();

  for (let i = days - 1; i >= 0; i--) {
    const d = new Date(today);
    d.setDate(d.getDate() - i);
    const key = d.toISOString().slice(0, 10);
    byDay.set(key, { day: key, initial: 0, reminder: 0, final: 0 });
  }

  for (const row of rows) {
    const entry = byDay.get(row.day);
    if (!entry) continue;
    if (row.notification_count === 1) entry.initial += row.total;
    else if (row.notification_count === 2) entry.reminder += row.total;
    else entry.final += row.total; // 3 o más: iban con la plantilla final
  }

  return [...byDay.values()];
}

function formatDay(day: string): string {
  return parseUtc(`${day}T00:00:00`).toLocaleDateString("es-CO", {
    day: "numeric",
    month: "short",
  });
}

export default function DashboardApp({ businessId }: { businessId: number }) {
  const router = useRouter();
  const [authorized, setAuthorized] = useState(false);
  const [businessName, setBusinessName] = useState("");
  const [days, setDays] = useState(30);
  const [rows, setRows] = useState<NotificationStatRow[]>([]);
  const [error, setError] = useState<string | null>(null);

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

  const load = useCallback(async () => {
    try {
      setRows(await getNotificationStats(businessId, days));
      setError(null);
    } catch {
      setError("No se pudieron cargar las estadísticas.");
    }
  }, [businessId, days]);

  useEffect(() => {
    if (!authorized) return;
    load();
    getBusiness(businessId)
      .then((b) => setBusinessName(b.name))
      .catch(() => setBusinessName(`Empresa ${businessId}`));
  }, [authorized, businessId, load]);

  const stats = useMemo(() => pivotStats(rows, days), [rows, days]);
  const maxTotal = Math.max(1, ...stats.map((s) => s.initial + s.reminder + s.final));
  const totals = useMemo(
    () =>
      stats.reduce(
        (acc, s) => ({
          initial: acc.initial + s.initial,
          reminder: acc.reminder + s.reminder,
          final: acc.final + s.final,
        }),
        { initial: 0, reminder: 0, final: 0 },
      ),
    [stats],
  );
  const labelEvery = Math.max(1, Math.ceil(days / 10));

  if (!authorized) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-black">
        <p className="text-sm text-neutral-500">Cargando...</p>
      </main>
    );
  }

  return (
    <main className="flex min-h-screen flex-col bg-black px-6 py-6">
      {/* Header */}
      <div className="mx-auto w-full max-w-5xl">
        <div className="flex items-center justify-between border-b border-moon/35 pb-4">
          <div>
            <h1 className="text-lg font-semibold tracking-widest">{businessName || "Dashboard"}</h1>
            <p className="mt-0.5 text-xs text-moon/70">Notificaciones de guías por día</p>
          </div>
          <div className="flex items-center gap-2">
            {DAY_OPTIONS.map((option) => (
              <button
                key={option}
                onClick={() => setDays(option)}
                className={`rounded border px-3 py-1 text-xs transition-colors ${
                  days === option
                    ? "border-moon bg-white/15 text-white"
                    : "border-moon/35 text-neutral-400 hover:text-moon"
                }`}
              >
                {option}d
              </button>
            ))}
            <Link
              href={`/chats/${businessId}`}
              className="ml-2 text-xs text-neutral-500 transition-colors hover:text-moon"
            >
              ← Volver a chats
            </Link>
          </div>
        </div>

        {/* Totales del período */}
        <div className="mt-5 grid grid-cols-3 gap-3">
          {SERIES.map((s) => (
            <div key={s.key} className="border border-moon/35 px-4 py-3">
              <p className="text-xs text-neutral-500">{s.label}</p>
              <p className="mt-1 text-2xl font-semibold text-white">{totals[s.key]}</p>
            </div>
          ))}
        </div>

        {error && <p className="mt-4 text-sm text-moon">{error}</p>}

        {/* Gráfica de barras apiladas */}
        <div className="mt-6 border border-moon/35 p-4">
          <div className="flex h-64 items-end gap-[2px]">
            {stats.map((stat, i) => {
              const total = stat.initial + stat.reminder + stat.final;
              return (
                <div
                  key={stat.day}
                  className="group flex h-full flex-1 flex-col items-center justify-end"
                  title={`${formatDay(stat.day)}: ${total} notificaciones (inicial ${stat.initial}, recordatorio ${stat.reminder}, final ${stat.final})`}
                >
                  <div className="flex w-full flex-col-reverse gap-px" style={{ height: `${(total / maxTotal) * 100}%` }}>
                    {stat.initial > 0 && (
                      <div className="w-full rounded-t-sm bg-moon" style={{ height: `${(stat.initial / total) * 100}%` }} />
                    )}
                    {stat.reminder > 0 && (
                      <div className="w-full bg-moon/50" style={{ height: `${(stat.reminder / total) * 100}%` }} />
                    )}
                    {stat.final > 0 && (
                      <div className="w-full bg-white" style={{ height: `${(stat.final / total) * 100}%` }} />
                    )}
                  </div>
                  {(i % labelEvery === 0 || i === stats.length - 1) && (
                    <span className="mt-2 hidden text-[10px] text-neutral-600 sm:block">
                      {formatDay(stat.day)}
                    </span>
                  )}
                </div>
              );
            })}
          </div>

          {/* Leyenda */}
          <div className="mt-4 flex items-center justify-center gap-5 border-t border-moon/25 pt-3">
            {SERIES.map((s) => (
              <span key={s.key} className="flex items-center gap-1.5 text-xs text-neutral-400">
                <span className={`inline-block h-2.5 w-2.5 rounded-sm ${s.color}`} />
                {s.label}
              </span>
            ))}
          </div>
        </div>
      </div>
    </main>
  );
}
