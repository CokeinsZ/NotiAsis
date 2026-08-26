import type { Business, ChatSummary, Message } from "./types";
import { clearToken, getToken, setToken } from "./auth";

// En el navegador las llamadas pasan por el proxy /api/backend
// (route handler en runtime). En el servidor (SSR) se habla directo
// con el backend porque no existe origen relativo.
const BASE =
  typeof window === "undefined"
    ? (process.env.BACKEND_URL ?? "http://localhost:3000")
    : "/api/backend";

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

function redirectToLogin(): void {
  if (typeof window !== "undefined") {
    clearToken();
    window.location.href = "/login";
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const token = getToken();
  const headers = new Headers(init?.headers);
  if (token) {
    headers.set("Authorization", `Bearer ${token}`);
  }

  const response = await fetch(`${BASE}${path}`, { ...init, headers });

  if (response.status === 401) {
    redirectToLogin();
    throw new ApiError(401, "Sesión expirada");
  }

  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new ApiError(response.status, body.message ?? `Error ${response.status}`);
  }
  return response.json();
}

// ---------------- Auth ----------------

export interface LoginResponse {
  token: string;
  expires_in: number;
  business_id: number | null;
  phone_number: string | null;
}

export async function login(username: string, password: string): Promise<LoginResponse> {
  const response = await fetch(`${BASE}/auth/login`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ username, password }),
  });

  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new ApiError(response.status, body.message ?? "Credenciales inválidas");
  }

  const data = (await response.json()) as LoginResponse & { message: string };
  setToken(data.token);
  return data;
}

// ---------------- Datos ----------------

export async function getBusiness(businessId: number): Promise<Business> {
  const data = await request<{ business: Business }>(`/businesses/${businessId}`);
  return data.business;
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

export async function setChatImportance(
  businessId: number,
  userPhone: string,
  isImportant: boolean,
): Promise<void> {
  await request<{ message: string }>(
    `/chats/${businessId}/${encodeURIComponent(userPhone)}/importance`,
    {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ is_important: isImportant }),
    },
  );
}

/**
 * Descarga multimedia a través del backend (que la obtiene de Meta en
 * memoria, sin guardarla) y la devuelve como Blob para visualizarla o
 * reproducirla localmente en el navegador.
 */
export async function fetchMediaBlob(mediaId: string): Promise<Blob> {
  const token = getToken();
  const headers = new Headers();
  if (token) {
    headers.set("Authorization", `Bearer ${token}`);
  }

  const response = await fetch(
    `${BASE}/messages/media/${encodeURIComponent(mediaId)}`,
    { headers },
  );

  if (response.status === 401) {
    redirectToLogin();
    throw new ApiError(401, "Sesión expirada");
  }
  if (!response.ok) {
    throw new ApiError(response.status, "No se pudo cargar el archivo");
  }
  return response.blob();
}

export async function sendChatMessage(
  businessId: number,
  userPhone: string,
  message: string,
): Promise<Message> {
  const data = await request<{ data: Message; renewed_token: string | null }>(
    `/chats/${businessId}/${encodeURIComponent(userPhone)}/messages`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ message }),
    },
  );

  // Cada mensaje libre renueva la sesión por otros 15 minutos.
  if (data.renewed_token) {
    setToken(data.renewed_token);
  }

  return data.data;
}
