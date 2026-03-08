# Base image for Cloudflare Workers with Rust + wrangler
FROM rust:1.83-slim as base

# Install dependencies
RUN apt-get update && apt-get install -y \
    curl \
    git \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js and npm (required for wrangler)
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install wrangler globally
RUN npm install -g wrangler@latest

# Note: worker-build is not needed globally - wrangler handles Rust compilation
# Add wasm32 target for Rust
RUN rustup target add wasm32-unknown-unknown

# Set working directory
WORKDIR /app

# Copy shared library first (for caching)
COPY workers/shared /app/workers/shared

# Note: Don't pre-build - wrangler will compile when starting dev server
# This avoids edition2024 issues and keeps container build fast

# Production image
FROM base as worker

# Copy project files
COPY workers /app/workers
COPY cli/migrations /app/cli/migrations

# Expose ports for workers
# 8787 = WebFinger, 8788 = Actor, 8789 = Inbox, 8790 = Outbox
EXPOSE 8787 8788 8789 8790

# Default command (override in docker-compose)
CMD ["wrangler", "dev", "--local"]
