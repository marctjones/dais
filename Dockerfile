# Base image for Cloudflare Workers with Rust + wrangler
FROM rust:1.75-slim as base

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

# Install worker-build for Rust Workers
RUN cargo install worker-build

# Set working directory
WORKDIR /app

# Copy shared library first (for caching)
COPY workers/shared /app/workers/shared

# Build shared library
RUN cd workers/shared && cargo build --release

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
