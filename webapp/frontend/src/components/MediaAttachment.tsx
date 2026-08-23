"use client";

import { useEffect, useState } from "react";
import { fetchMediaBlob } from "@/lib/api";
import type { MediaType } from "@/lib/types";

interface Props {
  mediaId: string;
  mediaType: MediaType;
  caption?: string | null;
}

const LABELS: Record<string, string> = {
  document: "Ver documento",
  image: "Ver imagen",
  audio: "Cargar audio",
};

/**
 * Carga multimedia bajo demanda y la visualiza/reproduce en memoria con
 * un blob URL: el archivo nunca se guarda en el servidor ni en disco del
 * cliente, solo vive en la memoria del navegador.
 */
export default function MediaAttachment({ mediaId, mediaType, caption }: Props) {
  const [blobUrl, setBlobUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);

  // Liberar la memoria del blob al desmontar o cambiar de media
  useEffect(() => {
    return () => {
      if (blobUrl) URL.revokeObjectURL(blobUrl);
    };
  }, [blobUrl]);

  async function load() {
    setLoading(true);
    setError(false);
    try {
      const blob = await fetchMediaBlob(mediaId);
      setBlobUrl(URL.createObjectURL(blob));
    } catch {
      setError(true);
    } finally {
      setLoading(false);
    }
  }

  if (error) {
    return <span className="text-xs text-moon/80">No se pudo cargar el archivo</span>;
  }

  if (!blobUrl) {
    return (
      <button
        onClick={load}
        disabled={loading}
        className="mt-1 inline-block rounded border border-moon/50 px-3 py-1 text-xs text-moon transition-colors hover:bg-white/10 hover:text-white disabled:opacity-40"
      >
        {loading ? "Cargando..." : LABELS[mediaType] ?? "Ver archivo"}
      </button>
    );
  }

  if (mediaType === "image") {
    return (
      <a href={blobUrl} target="_blank" rel="noreferrer">
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img
          src={blobUrl}
          alt={caption ?? "Imagen"}
          className="mt-1 max-w-xs rounded border border-moon/30"
        />
      </a>
    );
  }

  if (mediaType === "audio") {
    return (
      // eslint-disable-next-line jsx-a11y/media-has-caption
      <audio controls src={blobUrl} className="mt-1 h-9 max-w-full" />
    );
  }

  // document (y cualquier otro tipo): visor del navegador en pestaña nueva
  return (
    <a
      href={blobUrl}
      target="_blank"
      rel="noreferrer"
      className="mt-1 inline-block rounded border border-moon/50 px-3 py-1 text-xs text-moon transition-colors hover:bg-white/10 hover:text-white"
    >
      Abrir documento
    </a>
  );
}
