import type { Business, ChatSummary, Message } from "./types";

// En el navegador las llamadas pasan por el proxy /api/backend
// (ver next.config.ts), así no hay CORS. En el servidor (SSR) se habla
// directo con el backend porque no existe origen relativo.
const BASE =
  typeof window === "undefined"
    ? (process.env.BACKEND_URL ?? "http://localhost:3000")
    : "/api/backend";

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${BASE}${path}`, init);
  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new ApiError(response.status, body.message ?? `Error ${response.status}`);
  }
  return response.json();
}

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

export async function getBusinesses(): Promise<Business[]> {
  const data = await request<{ businesses: Business[] }>("/businesses");
  return data.businesses;
}

export async function getChats(businessId: number): Promise<ChatSummary[]> {
  const data = await request<{ chats: ChatSummary[] }>(`/chats?business_id=${businessId}`);
  return data.chats;
}

export async function getChatMessages(businessId: number, userPhone: string): Promise<Message[]> {
  const data = await request<{ data: Message[] }>(
    `/chats/${businessId}/${encodeURIComponent(userPhone)}/messages`,
  );
  return data.data;
}

export async function sendChatMessage(
  businessId: number,
  userPhone: string,
  message: string,
): Promise<Message> {
  const data = await request<{ data: Message }>(
    `/chats/${businessId}/${encodeURIComponent(userPhone)}/messages`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ message }),
    },
  );
  return data.data;
}
