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
  window_open: boolean;
}

export type MediaType = "text" | "document" | "audio" | "image";
export type MessageStatus = "sent" | "delivered" | "read";

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
}
