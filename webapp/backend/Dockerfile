# ---- Etapa de compilación ----
FROM rust:1.97-slim AS builder

WORKDIR /app

# Dependencias del sistema para los crates con TLS nativo
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Primero solo las dependencias (capa cacheable mientras no cambien)
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs
RUN cargo build --release && rm -rf src

# Ahora el código fuente real
COPY src ./src
RUN touch src/main.rs && cargo build --release

# ---- Etapa de ejecución ----
# OJO: debe ser la misma distro que el builder (rust:1.97-slim es
# Debian trixie). Con bookworm el binario no arranca:
# "GLIBC_2.39 not found" (trixie trae glibc 2.41, bookworm 2.36).
FROM debian:trixie-slim

# ffmpeg: normaliza audios grabados (webm/mp4) a OGG/Opus mono para
# notas de voz nativas de WhatsApp (voice:true exige .ogg OPUS).
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 ffmpeg \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/mania-back-end ./mania-back-end

EXPOSE 3001

CMD ["./mania-back-end"]
