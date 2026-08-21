// Proxy transparente /api/backend/* -> BACKEND_URL/*
//
// Se implementa como route handler (runtime) en vez de `rewrites` en
// next.config.ts porque los rewrites se congelan en tiempo de compilación
// y la URL del backend debe poder configurarse al ejecutar el contenedor.

const BACKEND_URL = () => process.env.BACKEND_URL ?? "http://localhost:3000";

const HOP_BY_HOP_HEADERS = [
  "host",
  "connection",
  "content-length",
  "transfer-encoding",
  "keep-alive",
];

async function proxy(
  request: Request,
  { params }: { params: Promise<{ path: string[] }> },
) {
  const { path } = await params;
  const url = new URL(request.url);
  const target = `${BACKEND_URL()}/${path.join("/")}${url.search}`;

  const headers = new Headers(request.headers);
  for (const header of HOP_BY_HOP_HEADERS) {
    headers.delete(header);
  }

  const hasBody = !["GET", "HEAD"].includes(request.method);
  const response = await fetch(target, {
    method: request.method,
    headers,
    body: hasBody ? request.body : undefined,
    // @ts-expect-error requerido por undici para bodies streaming
    duplex: hasBody ? "half" : undefined,
  });

  const responseHeaders = new Headers(response.headers);
  for (const header of HOP_BY_HOP_HEADERS) {
    responseHeaders.delete(header);
  }

  return new Response(response.body, {
    status: response.status,
    headers: responseHeaders,
  });
}

export { proxy as GET, proxy as POST, proxy as PATCH, proxy as PUT, proxy as DELETE };
