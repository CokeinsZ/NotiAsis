import type { NextConfig } from "next";

// El proxy /api/backend -> backend de Rust vive en
// src/app/api/backend/[...path]/route.ts (route handler en runtime, para
// que BACKEND_URL se lea del entorno en ejecución y no quede congelada
// en la imagen compilada).
const nextConfig: NextConfig = {};

export default nextConfig;

