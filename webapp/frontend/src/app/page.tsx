import Link from "next/link";
import { getBusinesses } from "@/lib/api";
import type { Business } from "@/lib/types";

export const dynamic = "force-dynamic";

export default async function Home() {
  let businesses: Business[] = [];
  let loadError: string | null = null;

  try {
    businesses = await getBusinesses();
  } catch {
    loadError = "No se pudo conectar con el backend.";
  }

  return (
    <main className="flex min-h-screen flex-col items-center justify-center bg-black px-6">
      <h1 className="mb-2 text-4xl font-semibold tracking-widest text-white">
        NotiAsis
      </h1>
      <p className="mb-10 text-sm text-moon/70">
        Selecciona una empresa para ver sus conversaciones
      </p>

      {loadError && (
        <p className="rounded border border-moon/35 px-4 py-3 text-sm text-moon">
          {loadError}
        </p>
      )}

      <div className="flex w-full max-w-sm flex-col divide-y divide-moon/25 border border-moon/35">
        {businesses.map((business) => (
          <Link
            key={business.id}
            href={`/chats/${business.id}`}
            className="group px-5 py-4 transition-colors hover:bg-white/5"
          >
            <span className="block font-medium text-white transition-all group-hover:bg-gradient-to-r group-hover:from-white group-hover:to-moon group-hover:bg-clip-text group-hover:text-transparent">
              {business.name}
            </span>
            <span className="mt-1 block text-xs text-neutral-500">
              Ver conversaciones
            </span>
          </Link>
        ))}

        {!loadError && businesses.length === 0 && (
          <p className="px-5 py-4 text-sm text-neutral-500">
            No hay empresas registradas todavía.
          </p>
        )}
      </div>
    </main>
  );
}
