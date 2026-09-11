"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { getBusiness, getNotificationStats } from "@/lib/api";
import { getValidClaims } from "@/lib/auth";
import type { DayStats, NotificationStatRow } from "@/lib/types";
import { formatDayDateString } from "@/lib/time";

const PRESET_DAYS = [
  { label: "7 días", days: 7 },
  { label: "30 días", days: 30 },
  { label: "90 días", days: 90 },
] as const;

const SERIES = [
  { key: "initial", label: "Notificación inicial", color: "bg-moon" },
  { key: "reminder", label: "Recordatorio", color: "bg-moon/50" },
  { key: "final", label: "Recordatorio final", color: "bg-white" },
] as const;

const MONTHS_SHORT = [
  "ene", "feb", "mar", "abr", "may", "jun",
  "jul", "ago", "sep", "oct", "nov", "dic",
];

const WEEKDAYS_SHORT = ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"];

function toIsoDate(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/**
 * Genera la lista completa de fechas consecutivas "YYYY-MM-DD"
 * entre startDate y endDate (ambas inclusive).
 */
function generateDateRange(startDateStr: string, endDateStr: string): string[] {
  const dates: string[] = [];
  const [sy, sm, sd] = startDateStr.split("-").map(Number);
  const [ey, em, ed] = endDateStr.split("-").map(Number);

  const current = new Date(sy, sm - 1, sd);
  const end = new Date(ey, em - 1, ed);

  // Seguridad para evitar bucles infinitos si las fechas están invertidas
  if (current > end) return [startDateStr];

  while (current <= end) {
    dates.push(toIsoDate(current));
    current.setDate(current.getDate() + 1);
  }
  return dates;
}

/** Pivota las filas del backend agrupadas por día y garantiza que todos los días del rango existan. */
function buildDailyStats(rows: NotificationStatRow[], dateList: string[]): DayStats[] {
  const map = new Map<string, DayStats>();
  for (const dateStr of dateList) {
    map.set(dateStr, { day: dateStr, initial: 0, reminder: 0, final: 0 });
  }

  for (const row of rows) {
    const entry = map.get(row.day);
    if (!entry) continue;
    if (row.notification_count === 1) entry.initial += row.total;
    else if (row.notification_count === 2) entry.reminder += row.total;
    else entry.final += row.total;
  }

  return dateList.map((d) => map.get(d)!);
}

export default function DashboardApp({ businessId }: { businessId: number }) {
  const router = useRouter();
  const [authorized, setAuthorized] = useState(false);
  const [businessName, setBusinessName] = useState("");

  // Modo de selección de fecha: 'preset' (7d/30d/90d) o 'custom' (rango / día puntual)
  const [activePreset, setActivePreset] = useState<number | null>(30);
  const [customStart, setCustomStart] = useState<string>("");
  const [customEnd, setCustomEnd] = useState<string>("");
  const [showCustomPicker, setShowCustomPicker] = useState(false);

  const [rows, setRows] = useState<NotificationStatRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Día hovereado en la gráfica para mostrar desglose detallado
  const [hoveredDay, setHoveredDay] = useState<DayStats | null>(null);

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

  // Carga de estadísticas según el filtro activo
  const loadStats = useCallback(async () => {
    setLoading(true);
    try {
      let data: NotificationStatRow[];
      if (customStart && customEnd) {
        data = await getNotificationStats(businessId, {
          startDate: customStart,
          endDate: customEnd,
        });
      } else {
        const days = activePreset ?? 30;
        data = await getNotificationStats(businessId, { days });
      }
      setRows(data);
      setError(null);
    } catch {
      setError("No se pudieron cargar las estadísticas.");
    } finally {
      setLoading(false);
    }
  }, [businessId, activePreset, customStart, customEnd]);

  useEffect(() => {
    if (!authorized) return;
    loadStats();
    getBusiness(businessId)
      .then((b) => setBusinessName(b.name))
      .catch(() => setBusinessName(`Empresa ${businessId}`));
  }, [authorized, businessId, loadStats]);

  // Lista de días que se deben mostrar en el eje X
  const activeDateList = useMemo(() => {
    if (customStart && customEnd) {
      return generateDateRange(customStart, customEnd);
    }
    const days = activePreset ?? 30;
    const today = new Date();
    const start = new Date(today);
    start.setDate(today.getDate() - (days - 1));
    return generateDateRange(toIsoDate(start), toIsoDate(today));
  }, [customStart, customEnd, activePreset]);

  const stats = useMemo(() => buildDailyStats(rows, activeDateList), [rows, activeDateList]);

  // Cálculo del total máximo para escalar las barras proporcionalmente
  const maxTotal = useMemo(() => {
    const highest = Math.max(0, ...stats.map((s) => s.initial + s.reminder + s.final));
    return Math.max(highest, 1);
  }, [stats]);

  // Totales acumulados del período seleccionado
  const totals = useMemo(() => {
    return stats.reduce(
      (acc, s) => ({
        total: acc.total + s.initial + s.reminder + s.final,
        initial: acc.initial + s.initial,
        reminder: acc.reminder + s.reminder,
        final: acc.final + s.final,
      }),
      { total: 0, initial: 0, reminder: 0, final: 0 },
    );
  }, [stats]);

  // Acciones rápidas de fecha
  function handlePresetClick(days: number) {
    setActivePreset(days);
    setCustomStart("");
    setCustomEnd("");
    setShowCustomPicker(false);
  }

  function handleSelectSingleDate(dateStr: string) {
    setActivePreset(null);
    setCustomStart(dateStr);
    setCustomEnd(dateStr);
    setShowCustomPicker(true);
  }

  function handleApplyCustomRange() {
    if (!customStart) return;
    const end = customEnd || customStart;
    setActivePreset(null);
    setCustomStart(customStart <= end ? customStart : end);
    setCustomEnd(customStart <= end ? end : customStart);
  }

  function handleResetFilters() {
    handlePresetClick(30);
  }

  const todayStr = useMemo(() => toIsoDate(new Date()), []);
  const yesterdayStr = useMemo(() => {
    const y = new Date();
    y.setDate(y.getDate() - 1);
    return toIsoDate(y);
  }, []);

  if (!authorized) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-black">
        <p className="text-sm text-neutral-500">Cargando...</p>
      </main>
    );
  }

  return (
    <main className="flex min-h-screen flex-col bg-black px-4 py-6 sm:px-8">
      <div className="mx-auto w-full max-w-5xl">
        {/* Encabezado */}
        <header className="flex flex-col gap-3 border-b border-moon/35 pb-5 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <div className="flex items-center gap-3">
              <h1 className="text-xl font-semibold tracking-widest text-white">
                {businessName || "Victor Store"}
              </h1>
              <span className="rounded border border-moon/40 bg-moon/10 px-2 py-0.5 text-[11px] font-medium text-moon">
                Dashboard
              </span>
            </div>
            <p className="mt-1 text-xs text-neutral-400">
              Notificaciones de guías enviadas a clientes
            </p>
          </div>

          <Link
            href={`/chats/${businessId}`}
            className="inline-flex items-center gap-1.5 self-start text-xs text-neutral-400 transition-colors hover:text-moon sm:self-auto"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="19" y1="12" x2="5" y2="12" />
              <polyline points="12 19 5 12 12 5" />
            </svg>
            Volver a chats
          </Link>
        </header>

        {/* Barra de control de rango de fechas */}
        <section className="mt-5 flex flex-col gap-3 rounded border border-moon/35 bg-black p-4">
          <div className="flex flex-wrap items-center justify-between gap-2.5">
            <span className="text-xs font-medium text-neutral-400">
              Rango a visualizar:
            </span>

            {/* Presets de días */}
            <div className="flex flex-wrap items-center gap-1.5">
              {PRESET_DAYS.map((preset) => (
                <button
                  key={preset.days}
                  onClick={() => handlePresetClick(preset.days)}
                  className={`rounded border px-3 py-1 text-xs font-medium transition-all ${
                    activePreset === preset.days
                      ? "border-moon bg-white/20 text-white shadow-sm"
                      : "border-moon/35 text-neutral-400 hover:border-moon/70 hover:text-white"
                  }`}
                >
                  {preset.label}
                </button>
              ))}

              <button
                onClick={() => handleSelectSingleDate(todayStr)}
                className={`rounded border px-3 py-1 text-xs font-medium transition-all ${
                  customStart === todayStr && customEnd === todayStr
                    ? "border-moon bg-white/20 text-white shadow-sm"
                    : "border-moon/35 text-neutral-400 hover:border-moon/70 hover:text-white"
                }`}
              >
                Hoy
              </button>

              <button
                onClick={() => handleSelectSingleDate(yesterdayStr)}
                className={`rounded border px-3 py-1 text-xs font-medium transition-all ${
                  customStart === yesterdayStr && customEnd === yesterdayStr
                    ? "border-moon bg-white/20 text-white shadow-sm"
                    : "border-moon/35 text-neutral-400 hover:border-moon/70 hover:text-white"
                }`}
              >
                Ayer
              </button>

              <button
                onClick={() => setShowCustomPicker((v) => !v)}
                className={`flex items-center gap-1 rounded border px-3 py-1 text-xs font-medium transition-all ${
                  showCustomPicker || (customStart && customEnd && customStart !== todayStr && customStart !== yesterdayStr)
                    ? "border-moon bg-moon/15 text-moon"
                    : "border-moon/35 text-neutral-400 hover:border-moon/70 hover:text-white"
                }`}
              >
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
                  <rect x="3" y="4" width="18" height="18" rx="2" ry="2" />
                  <line x1="16" y1="2" x2="16" y2="6" />
                  <line x1="8" y1="2" x2="8" y2="6" />
                  <line x1="3" y1="10" x2="21" y2="10" />
                </svg>
                Personalizado
              </button>
            </div>
          </div>

          {/* Selector de rango de fechas personalizado (input Desde / Hasta) */}
          {showCustomPicker && (
            <div className="flex flex-wrap items-center gap-3 border-t border-moon/20 pt-3">
              <div className="flex items-center gap-2">
                <label htmlFor="custom-start" className="text-xs text-neutral-400">
                  Desde:
                </label>
                <input
                  id="custom-start"
                  type="date"
                  value={customStart}
                  max={customEnd || todayStr}
                  onChange={(e) => setCustomStart(e.target.value)}
                  className="rounded border border-moon/40 bg-black px-2.5 py-1 text-xs text-white outline-none transition-colors focus:border-moon [color-scheme:dark]"
                />
              </div>

              <div className="flex items-center gap-2">
                <label htmlFor="custom-end" className="text-xs text-neutral-400">
                  Hasta:
                </label>
                <input
                  id="custom-end"
                  type="date"
                  value={customEnd}
                  min={customStart}
                  max={todayStr}
                  onChange={(e) => setCustomEnd(e.target.value)}
                  className="rounded border border-moon/40 bg-black px-2.5 py-1 text-xs text-white outline-none transition-colors focus:border-moon [color-scheme:dark]"
                />
              </div>

              <button
                onClick={handleApplyCustomRange}
                disabled={!customStart}
                className="rounded border border-moon/60 bg-white/10 px-3.5 py-1 text-xs font-medium text-white transition-all hover:bg-white/20 disabled:cursor-not-allowed disabled:opacity-30"
              >
                Aplicar filtro
              </button>

              <button
                onClick={handleResetFilters}
                className="text-xs text-neutral-500 transition-colors hover:text-moon"
              >
                Restablecer
              </button>
            </div>
          )}
        </section>

        {/* Tarjetas de totales del período seleccionado */}
        <section className="mt-4 grid grid-cols-2 gap-3 sm:grid-cols-4">
          <div className="rounded border border-moon/35 bg-black px-4 py-3">
            <span className="text-[11px] font-medium text-neutral-400">
              Total notificaciones
            </span>
            <p className="mt-1 font-mono text-2xl font-bold text-white">
              {totals.total}
            </p>
          </div>

          <div className="rounded border border-moon/35 bg-black px-4 py-3">
            <span className="flex items-center gap-1.5 text-[11px] font-medium text-neutral-400">
              <span className="inline-block h-2 w-2 rounded-full bg-moon" />
              Notificación inicial
            </span>
            <p className="mt-1 font-mono text-2xl font-bold text-moon">
              {totals.initial}
            </p>
          </div>

          <div className="rounded border border-moon/35 bg-black px-4 py-3">
            <span className="flex items-center gap-1.5 text-[11px] font-medium text-neutral-400">
              <span className="inline-block h-2 w-2 rounded-full bg-moon/50" />
              Recordatorio
            </span>
            <p className="mt-1 font-mono text-2xl font-bold text-moon/80">
              {totals.reminder}
            </p>
          </div>

          <div className="rounded border border-moon/35 bg-black px-4 py-3">
            <span className="flex items-center gap-1.5 text-[11px] font-medium text-neutral-400">
              <span className="inline-block h-2 w-2 rounded-full bg-white" />
              Recordatorio final
            </span>
            <p className="mt-1 font-mono text-2xl font-bold text-white">
              {totals.final}
            </p>
          </div>
        </section>

        {error && (
          <p className="mt-4 rounded border border-moon/35 bg-moon/10 px-4 py-2 text-sm text-moon">
            {error}
          </p>
        )}

        {/* Contenedor principal de la gráfica */}
        <section className="mt-5 rounded border border-moon/35 bg-black p-4">
          {/* Header de la gráfica + Leyenda */}
          <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex items-baseline gap-2">
              <h2 className="text-sm font-semibold text-white">
                Frecuencia diaria de envíos
              </h2>
              <span className="text-xs text-neutral-500">
                ({stats.length} {stats.length === 1 ? "día" : "días"} visualizados)
              </span>
            </div>

            {/* Leyenda de colores */}
            <div className="flex flex-wrap items-center gap-4 text-xs text-neutral-400">
              {SERIES.map((s) => (
                <span key={s.key} className="flex items-center gap-1.5">
                  <span className={`inline-block h-2.5 w-2.5 rounded-sm ${s.color}`} />
                  {s.label}
                </span>
              ))}
            </div>
          </div>

          {/* Detalle interactivo del día seleccionado o en hover */}
          <div className="mt-3 flex min-h-[32px] items-center justify-between rounded border border-moon/20 bg-white/[0.02] px-3 py-1.5 text-xs">
            {hoveredDay ? (
              <>
                <span className="font-medium text-white">
                  {formatDayDateString(hoveredDay.day)}
                </span>
                <span className="flex items-center gap-3 text-neutral-300">
                  <span>
                    Total: <strong className="font-mono text-white">{hoveredDay.initial + hoveredDay.reminder + hoveredDay.final}</strong>
                  </span>
                  <span className="text-moon">
                    Inicial: <strong className="font-mono">{hoveredDay.initial}</strong>
                  </span>
                  <span className="text-moon/70">
                    Recordatorio: <strong className="font-mono">{hoveredDay.reminder}</strong>
                  </span>
                  <span className="text-white">
                    Final: <strong className="font-mono">{hoveredDay.final}</strong>
                  </span>
                </span>
              </>
            ) : (
              <span className="text-neutral-500">
                Pasa el cursor sobre cualquier barra para ver el desglose exacto de ese día.
              </span>
            )}
          </div>

          {/* Gráfica con scroll horizontal inteligente para ver TODOS los días con claridad */}
          <div className="mt-4 overflow-x-auto pb-7 pt-2">
            <div
              className="flex min-w-[650px] flex-col sm:min-w-full"
              style={{
                minWidth: stats.length > 31 ? `${stats.length * 36}px` : undefined,
              }}
            >
              {/* TIER 1: Área de barras (Plano estricto superior).
                  La línea inferior `border-b border-moon/40` actúa como
                  suelo inamovible: NINGUNA barra puede pasar por debajo ni
                  tapar las fechas. */}
              <div className="relative flex h-60 items-end border-b border-moon/40 pb-1">
                {/* Líneas guía sutiles de fondo (25%, 50%, 75%, 100%) */}
                <div className="pointer-events-none absolute inset-0 flex flex-col justify-between opacity-15">
                  <div className="border-b border-dashed border-moon" />
                  <div className="border-b border-dashed border-moon" />
                  <div className="border-b border-dashed border-moon" />
                  <div className="border-b border-dashed border-moon" />
                </div>

                {stats.map((stat) => {
                  const dayTotal = stat.initial + stat.reminder + stat.final;
                  const barHeightPercent = dayTotal > 0
                    ? Math.max(3, (dayTotal / maxTotal) * 100)
                    : 0;

                  return (
                    <div
                      key={stat.day}
                      onMouseEnter={() => setHoveredDay(stat)}
                      onMouseLeave={() => setHoveredDay(null)}
                      className="group relative flex h-full flex-1 flex-col items-center justify-end px-0.5 transition-colors hover:bg-white/[0.04]"
                    >
                      {/* Valor total arriba de la barra cuando hay notificaciones */}
                      {dayTotal > 0 && (
                        <span className="mb-1 font-mono text-[10px] text-neutral-400 transition-colors group-hover:text-white">
                          {dayTotal}
                        </span>
                      )}

                      {/* Barra apilada: strictly inside the h-60 plot area */}
                      {dayTotal > 0 ? (
                        <div
                          className="flex w-full max-w-[20px] flex-col-reverse overflow-hidden rounded-t-sm shadow-sm transition-all group-hover:brightness-110 sm:max-w-[24px]"
                          style={{ height: `${barHeightPercent}%` }}
                        >
                          {stat.initial > 0 && (
                            <div
                              className="w-full bg-moon transition-all"
                              style={{ height: `${(stat.initial / dayTotal) * 100}%` }}
                            />
                          )}
                          {stat.reminder > 0 && (
                            <div
                              className="w-full bg-moon/50 transition-all"
                              style={{ height: `${(stat.reminder / dayTotal) * 100}%` }}
                            />
                          )}
                          {stat.final > 0 && (
                            <div
                              className="w-full bg-white transition-all"
                              style={{ height: `${(stat.final / dayTotal) * 100}%` }}
                            />
                          )}
                        </div>
                      ) : (
                        // Indicador sutil de 0 notificaciones
                        <div className="h-0.5 w-1 rounded-full bg-neutral-800" />
                      )}
                    </div>
                  );
                })}
              </div>

              {/* TIER 2: Eje X con las fechas (Estrictamente debajo del suelo del plano).
                  Separadas por el borde, nunca se solapan con las barras.
                  Cada columna muestra: número de día, mes, y día de la semana. */}
              <div className="flex pt-3 pb-1">
                {stats.map((stat) => {
                  const [, monthStr, dayStr] = stat.day.split("-");
                  const dayNum = parseInt(dayStr, 10);
                  const monthName = MONTHS_SHORT[parseInt(monthStr, 10) - 1];

                  const [y, m, d] = stat.day.split("-").map(Number);
                  const weekdayName = WEEKDAYS_SHORT[new Date(y, m - 1, d).getDay()];
                  const isToday = stat.day === todayStr;

                  return (
                    <div
                      key={stat.day}
                      onMouseEnter={() => setHoveredDay(stat)}
                      onMouseLeave={() => setHoveredDay(null)}
                      className="group flex flex-1 cursor-pointer flex-col items-center justify-start text-center"
                    >
                      {/* Día del mes */}
                      <span
                        className={`text-xs font-semibold leading-tight transition-colors ${
                          isToday
                            ? "rounded bg-moon/20 px-1 font-bold text-moon"
                            : "text-white group-hover:text-moon"
                        }`}
                      >
                        {dayNum}
                      </span>

                      {/* Mes en formato corto (ago, sep, etc.) */}
                      <span className="mt-0.5 font-mono text-[9px] uppercase tracking-wider text-neutral-400 leading-tight group-hover:text-neutral-200">
                        {monthName}
                      </span>

                      {/* Día de la semana (lun, mar, etc.) */}
                      <span className="text-[8px] text-neutral-500 leading-tight sm:text-[9px]">
                        {weekdayName}
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>
          </div>
        </section>
      </div>
    </main>
  );
}
