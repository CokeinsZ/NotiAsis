"use client";

// Sesión del asociado: JWT en localStorage (sin cookies por simplicidad;
// no es auth "real" todavía).

const TOKEN_KEY = "notiasis_token";

export interface TokenClaims {
  sub: string;
  kind: string;
  business_id: number | null;
  phone_number: string | null;
  iat: number;
  exp: number;
}

export function getToken(): string | null {
  if (typeof window === "undefined") return null;
  return localStorage.getItem(TOKEN_KEY);
}

export function setToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token);
}

export function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY);
}

export function decodeClaims(token: string): TokenClaims | null {
  try {
    const payload = token.split(".")[1];
    const normalized = payload.replace(/-/g, "+").replace(/_/g, "/");
    return JSON.parse(atob(normalized)) as TokenClaims;
  } catch {
    return null;
  }
}

/** Token ausente, malformado o vencido -> null. */
export function getValidClaims(): TokenClaims | null {
  const token = getToken();
  if (!token) return null;

  const claims = decodeClaims(token);
  if (!claims) return null;

  if (claims.exp * 1000 <= Date.now()) {
    clearToken();
    return null;
  }
  return claims;
}
