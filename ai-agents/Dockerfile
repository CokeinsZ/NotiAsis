# ---- Etapa de compilación ----
FROM rust:1.97-slim AS builder

WORKDIR /app

# Dependencias del sistema para los crates con TLS nativo
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Primero solo las dependencias (capa cacheable mientras no cambien)
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && touch src/lib.rs
RUN cargo build --release && rm -rf src

# Ahora el código fuente real (los prompts .md van dentro del binario)
COPY src ./src
RUN touch src/main.rs src/lib.rs && cargo build --release --bin ai-agents

# ---- Etapa de ejecución ----
# Misma distro que el builder (rust:1.97-slim es Debian trixie), como el backend.
FROM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/ai-agents ./ai-agents

EXPOSE 3002

CMD ["./ai-agents"]
