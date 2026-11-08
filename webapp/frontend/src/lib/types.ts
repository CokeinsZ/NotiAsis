export interface Business {
  id: number;
  name: string;
  state: "Active" | "Inactive" | "Blocked" | "Deleted";
  created_at: string;
  updated_at: string;
}

export interface ChatSummary {
  business_id: number;
  user_id: string;
  user_full_name: string;
  last_user_message: string | null;
  last_user_message_timestamp: string | null;
  last_activity: string | null;
  is_important: boolean;
  last_guide_notification_at: string | null;
  window_open: boolean;
}

export type MediaType = "text" | "document" | "audio" | "image" | "video";
export type MessageStatus = "sent" | "delivered" | "read";

/** Fila cruda del backend: notificaciones por día, tipo y notification_count. */
export interface NotificationStatRow {
  day: string; // "2026-08-30"
  /** "office": escalación de la guía; "tracking": seguimiento del envío de Dropi. */
  kind: "office" | "tracking";
  /** office: 1=inicial, 2=recordatorio, 3=final, 4+=diario · tracking: nivel 1 o 2. */
  notification_count: number;
  total: number;
}

/** Un día de la gráfica, con los seis tipos ya pivotados. */
export interface DayStats {
  day: string;
  /** Seguimiento nivel 1 (pedido_enviado_1): listo para envío. */
  shipped: number;
  /** Seguimiento nivel 2 (pedido_camino_1): en camino. */
  inTransit: number;
  initial: number;
  reminder: number;
  final: number;
  /** Seguimiento diario (pedido_oficina): notificación 4 en adelante, solo pedidos de Dropi. */
  daily: number;
}

export interface Message {
  id: number;
  meta_message_id: string;
  business_id: number;
  user_id: string;
  media_id: string | null;
  media_type: MediaType;
  message: string | null;
  status: MessageStatus | null;
  from_user: boolean;
  created_at: string;
  /** Guía que notifica (saliente) o de la notificación que el cliente citó (entrante). */
  guide_number: string | null;
}
