// Los timestamps del backend son UTC sin zona horaria ("2026-08-21T07:01:19").
// Se interpretan siempre como UTC para que la hora local la ponga el navegador.
export function parseUtc(timestamp: string): Date {
  const hasZone = /([zZ]|[+-]\d{2}:?\d{2})$/.test(timestamp);
  return new Date(hasZone ? timestamp : `${timestamp}Z`);
}

const WINDOW_MS = 24 * 60 * 60 * 1000;

export interface GraceWindow {
  open: boolean;
  label: string;
}

/**
 * Tiempo de gracia restante para enviar mensajes libres (ventana de 24h de
 * Meta desde el último mensaje del usuario).
 */
export function remainingGrace(lastUserMessageTimestamp: string | null, now: Date): GraceWindow {
  if (!lastUserMessageTimestamp) {
    return { open: false, label: "Sin mensajes del usuario" };
  }

  const elapsed = now.getTime() - parseUtc(lastUserMessageTimestamp).getTime();
  // Se capea al máximo de la ventana: un timestamp "futuro" (relojes
  // desincronizados) no debe mostrar más de 24h.
  const remaining = Math.min(WINDOW_MS - elapsed, WINDOW_MS);

  if (remaining <= 0) {
    return { open: false, label: "Ventana cerrada" };
  }

  const totalMinutes = Math.floor(remaining / 60000);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;

  if (hours > 0) {
    return { open: true, label: `${hours}h ${minutes}m restantes` };
  }
  return { open: true, label: `${minutes}m restantes` };
}

export function formatMessageTime(timestamp: string): string {
  return parseUtc(timestamp).toLocaleTimeString("es-CO", {
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatMessageDay(timestamp: string): string {
  return parseUtc(timestamp).toLocaleDateString("es-CO", {
    day: "numeric",
    month: "short",
  });
}

/**
 * Formatea una fecha "YYYY-MM-DD" en español completo:
 * "Viernes, 28 de agosto de 2026"
 */
export function formatDayDateString(dateStr: string): string {
  const [y, m, d] = dateStr.split("-").map(Number);
  const date = new Date(y, m - 1, d);
  const formatted = date.toLocaleDateString("es-CO", {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
  });
  return formatted.charAt(0).toUpperCase() + formatted.slice(1);
}

/**
 * Fecha y hora completa en español para tooltips y hover de mensajes.
 * Ej: "Lunes, 24 de agosto de 2026, 02:33 p. m."
 */
export function formatFullDateTime(timestamp: string): string {
  const date = parseUtc(timestamp);
  const formattedDate = date.toLocaleDateString("es-CO", {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
  });
  const formattedTime = date.toLocaleTimeString("es-CO", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: true,
  });
  const capitalized = formattedDate.charAt(0).toUpperCase() + formattedDate.slice(1);
  return `${capitalized}, ${formattedTime}`;
}
