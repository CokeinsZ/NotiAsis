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
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/mania-back-end ./mania-back-end

EXPOSE 3001

CMD ["./mania-back-end"]
