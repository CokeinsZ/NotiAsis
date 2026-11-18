import type { AiEscalation, ChatAiActivity, GuideCustomerUpdate, Message } from "./types";
import { formatShortDay, localDayKey, parseUtc } from "./time";

/** Una fila del historial del chat. */
export type TimelineEntry =
  | { type: "message"; key: string; message: Message }
  | { type: "guide_update"; key: string; update: GuideCustomerUpdate }
  | { type: "escalation"; key: string; escalation: AiEscalation };

/** Un día del historial: su etiqueta ("Dom 15 Nov") y lo que pasó ese día. */
export interface TimelineDay {
  key: string;
  label: string;
  entries: TimelineEntry[];
}

interface TimedEntry {
  at: Date;
  entry: TimelineEntry;
}

/**
 * Intercala los mensajes con lo que registró el agente de IA (novedades
 * y escalaciones) en orden cronológico y los agrupa por día, en la hora
 * local del navegador. A igual hora va primero el mensaje, luego la
 * novedad y luego la escalación (sort es estable).
 */
export function buildTimeline(
  messages: Message[],
  activity: ChatAiActivity,
  now: Date = new Date(),
): TimelineDay[] {
  const timed: TimedEntry[] = [
    ...messages.map((message) => ({
      at: parseUtc(message.created_at),
      entry: { type: "message" as const, key: `message-${message.id}`, message },
    })),
    ...activity.guide_updates.map((update) => ({
      at: parseUtc(update.created_at),
      entry: { type: "guide_update" as const, key: `guide-update-${update.id}`, update },
    })),
    ...activity.escalations.map((escalation) => ({
      at: parseUtc(escalation.created_at),
      entry: { type: "escalation" as const, key: `escalation-${escalation.id}`, escalation },
    })),
  ];
  timed.sort((a, b) => a.at.getTime() - b.at.getTime());

  const days: TimelineDay[] = [];
  for (const { at, entry } of timed) {
    const key = localDayKey(at);
    let day = days.at(-1);
    if (day?.key !== key) {
      day = { key, label: formatShortDay(at, now), entries: [] };
      days.push(day);
    }
    day.entries.push(entry);
  }
  return days;
}
