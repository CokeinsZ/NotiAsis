"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { getValidClaims } from "@/lib/auth";

export default function ChatsIndexPage() {
  const router = useRouter();

  useEffect(() => {
    const claims = getValidClaims();
    if (!claims) {
      router.replace("/login");
    } else if (claims.business_id) {
      router.replace(`/chats/${claims.business_id}`);
    } else {
      router.replace("/login");
    }
  }, [router]);

  return (
    <main className="flex min-h-screen items-center justify-center bg-black">
      <p className="text-sm text-neutral-500">Cargando...</p>
    </main>
  );
}
