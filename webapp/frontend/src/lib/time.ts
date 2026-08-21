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
  const remaining = WINDOW_MS - elapsed;

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
