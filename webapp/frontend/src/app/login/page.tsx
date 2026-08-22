"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { login } from "@/lib/api";
import { decodeClaims } from "@/lib/auth";

export default function LoginPage() {
  const router = useRouter();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    setError(null);

    try {
      const response = await login(username.trim(), password);
      const claims = decodeClaims(response.token);

      if (claims?.business_id) {
        router.replace(`/chats/${claims.business_id}`);
      } else {
        router.replace("/chats");
      }
    } catch {
      setError("Usuario o contraseña inválidos.");
    } finally {
      setLoading(false);
    }
  }

  return (
    <main className="flex min-h-screen flex-col items-center justify-center bg-black px-6">
      <h1 className="mb-2 text-4xl font-semibold tracking-widest text-white">
        NotiAsis
      </h1>
      <p className="mb-10 text-sm text-moon/70">Inicia sesión para continuar</p>

      <form
        onSubmit={handleSubmit}
        className="flex w-full max-w-sm flex-col gap-4 border border-moon/35 p-6"
      >
        <div>
          <label htmlFor="username" className="mb-1 block text-xs text-neutral-400">
            Usuario
          </label>
          <input
            id="username"
            type="text"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            autoComplete="username"
            required
            className="w-full rounded border border-moon/35 bg-black px-4 py-2 text-sm text-white outline-none focus:border-moon"
          />
        </div>

        <div>
          <label htmlFor="password" className="mb-1 block text-xs text-neutral-400">
            Contraseña
          </label>
          <input
            id="password"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="current-password"
            required
            className="w-full rounded border border-moon/35 bg-black px-4 py-2 text-sm text-white outline-none focus:border-moon"
          />
        </div>

        {error && <p className="text-xs text-moon">{error}</p>}

        <button
          type="submit"
          disabled={loading}
          className="mt-2 rounded border border-moon/60 px-5 py-2 text-sm font-medium text-moon transition-all hover:bg-gradient-to-r hover:from-white/20 hover:to-transparent hover:text-white disabled:opacity-30"
        >
          {loading ? "Ingresando..." : "Ingresar"}
        </button>
      </form>
    </main>
  );
}
