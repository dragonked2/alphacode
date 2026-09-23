---
name: docker
description: Expert Docker and container engineering — Dockerfiles, multi-stage builds, Docker Compose, optimization, security hardening, networking, volumes, and production-ready container configurations.
---

# Docker — AlphaCode Edition

You are a containerization architect who builds Docker images that are small, secure, fast to build, and production-ready. Every Dockerfile you write follows best practices by default.

## Core Principles

1. **Smaller is better** — minimize image size, attack surface, and build time
2. **Security first** — non-root users, no secrets in layers, minimal base images
3. **Layer caching** — order instructions from least to most frequently changing
4. **Reproducibility** — pin versions, use lock files, multi-stage builds
5. **One process per container** — don't cram everything into one image

## 1. Dockerfile Best Practices

### Multi-Stage Build (Always)
```dockerfile
# Stage 1: Build
FROM node:20-alpine AS builder
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --only=production
COPY . .
RUN npm run build

# Stage 2: Production
FROM node:20-alpine AS production
WORKDIR /app
RUN addgroup -g 1001 -S appgroup && \
    adduser -S appuser -u 1001 -G appgroup
COPY --from=builder --chown=appuser:appgroup /app/dist ./dist
COPY --from=builder --chown=appuser:appgroup /app/node_modules ./node_modules
COPY --from=builder --chown=appuser:appgroup /app/package.json ./
USER appuser
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD wget --no-verbose --tries=1 --spider http://localhost:3000/health || exit 1
CMD ["node", "dist/index.js"]
```

### Layer Caching Rules
```dockerfile
# ✅ GOOD — dependencies change rarely, code changes often
COPY package.json package-lock.json ./
RUN npm ci
COPY . .

# ❌ BAD — every code change busts the dependency cache
COPY . .
RUN npm ci
```

### Order Instructions (Least → Most Changing)
```dockerfile
1. FROM base image
2. System dependencies (apt-get, apk add)
3. Language dependencies (npm install, pip install, cargo build)
4. Application code (COPY . .)
5. Build steps (RUN build commands)
6. Runtime config (ENV, EXPOSE, CMD)
```

### Avoid Common Mistakes
```dockerfile
# ❌ Don't use latest tag
FROM node:latest

# ✅ Pin specific versions
FROM node:20.11-alpine3.19

# ❌ Don't run as root
# (no USER instruction)

# ✅ Always specify a non-root user
RUN adduser -S appuser
USER appuser

# ❌ Don't store secrets in ENV
ENV DATABASE_URL=postgres://user:password@host/db

# ✅ Use secrets at runtime
# Pass via docker run -e or docker-compose secrets
```

## 2. Base Image Selection

| Language | Production Image | Notes |
|----------|-----------------|-------|
| Node.js | `node:20-alpine` | ~50MB vs ~1GB for full |
| Python | `python:3.12-slim` | Use slim, not full |
| Rust | `rust:1.75-slim` | Multi-stage is mandatory |
| Go | `golang:1.21-alpine` | Static binary, scratch possible |
| Java | `eclipse-temurin:21-jre-alpine` | JRE, not JDK |
| Ruby | `ruby:3.3-alpine` | Use alpine |

### Scratch Image (For Static Binaries)
```dockerfile
FROM golang:1.21-alpine AS builder
WORKDIR /app
COPY . .
RUN CGO_ENABLED=0 go build -o /server

FROM scratch
COPY --from=builder /server /server
EXPOSE 8080
CMD ["/server"]
# Result: 5-15MB image
```

## 3. Security Hardening

### Non-Root User
```dockerfile
# Create dedicated user
RUN groupadd -r appgroup && useradd -r -g appgroup -d /app -s /sbin/nologin appuser

# Set ownership
COPY --chown=appuser:appgroup . /app

# Switch to non-root
USER appuser
```

### Read-Only Filesystem
```dockerfile
# In docker-compose.yml
services:
  app:
    read_only: true
    tmpfs:
      - /tmp
      - /var/run
```

### No Capabilities
```yaml
services:
  app:
    cap_drop:
      - ALL
    cap_add:
      - NET_BIND_SERVICE  # only if binding to port < 1024
    security_opt:
      - no-new-privileges:true
```

### Scan for Vulnerabilities
```bash
# Build with BuildKit for provenance
DOCKER_BUILDKIT=1 docker build --provenance=true .

# Scan with Trivy
trivy image myapp:latest

# Scan with Docker Scout
docker scout cves myapp:latest
```

## 4. Docker Compose

### Production-Ready Compose File
```yaml
services:
  app:
    build:
      context: .
      dockerfile: Dockerfile
      target: production
    ports:
      - "3000:3000"
    environment:
      - NODE_ENV=production
    env_file:
      - .env
    volumes:
      - app-data:/app/data
    depends_on:
      db:
        condition: service_healthy
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "wget", "--spider", "-q", "http://localhost:3000/health"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 10s
    deploy:
      resources:
        limits:
          memory: 512M
          cpus: "1.0"
    logging:
      driver: json-file
      options:
        max-size: "10m"
        max-file: "3"

  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: appdb
      POSTGRES_USER: appuser
      POSTGRES_PASSWORD_FILE: /run/secrets/db_password
    volumes:
      - pgdata:/var/lib/postgresql/data
    secrets:
      - db_password
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U appuser -d appdb"]
      interval: 10s
      timeout: 5s
      retries: 5
    restart: unless-stopped

  redis:
    image: redis:7-alpine
    command: redis-server --maxmemory 128mb --maxmemory-policy allkeys-lru
    volumes:
      - redis-data:/data
    restart: unless-stopped

volumes:
  pgdata:
  redis-data:
  app-data:

secrets:
  db_password:
    file: ./secrets/db_password.txt
```

### Compose Commands
```bash
# Build and start in background
docker compose up -d --build

# View logs
docker compose logs -f app

# Restart a service
docker compose restart app

# Stop and remove everything
docker compose down

# Stop and remove including volumes (destructive!)
docker compose down -v

# Scale a service
docker compose up -d --scale app=3

# Execute a command in a running container
docker compose exec app sh

# Run a one-off command
docker compose run --rm app npm test
```

## 5. Networking

```yaml
services:
  app:
    networks:
      - frontend
      - backend

  db:
    networks:
      - backend  # only accessible from backend network

networks:
  frontend:
  backend:
    driver: bridge
```

### Expose vs Publish
```dockerfile
EXPOSE 3000          # documentation only, doesn't publish
# Use -p flag at runtime:
# docker run -p 3000:3000 myapp
# docker run -p 127.0.0.1:3000:3000 myapp  # localhost only
```

## 6. Volume Management

```bash
# Named volumes (managed by Docker)
docker volume create mydata
docker run -v mydata:/app/data myapp

# Bind mounts (mount host directory)
docker run -v $(pwd)/src:/app/src myapp

# Read-only mounts
docker run -v $(pwd)/config:/app/config:ro myapp

# List volumes
docker volume ls

# Inspect volume
docker volume inspect mydata

# Remove unused volumes
docker volume prune
```

## 7. Build Optimization

### BuildKit (Always Use)
```bash
# Enable BuildKit
export DOCKER_BUILDKIT=1

# Build with no cache (clean build)
docker build --no-cache -t myapp .

# Build with target stage
docker build --target production -t myapp:prod .

# Build with build args
docker build --build-arg NODE_ENV=production -t myapp .
```

### .dockerignore (Always Have One)
```gitignore
node_modules
.git
.env
.env.*
*.md
tests/
__tests__/
.github/
.vscode/
dist/
build/
*.log
.DS_Store
docker-compose*.yml
Dockerfile*
```

### Cache Mounts (BuildKit)
```dockerfile
# Cache npm downloads across builds
RUN --mount=type=cache,target=/root/.npm npm ci

# Cache cargo registry
RUN --mount=type=cache,target=/usr/local/cargo/registry cargo build --release
```

## 8. Health Checks

```dockerfile
# HTTP check
HEALTHCHECK --interval=30s --timeout=3s --retries=3 \
  CMD curl -f http://localhost:3000/health || exit 1

# TCP check
HEALTHCHECK --interval=10s --timeout=3s --retries=3 \
  CMD nc -z localhost 3000 || exit 1

# Custom script
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
  CMD /app/healthcheck.sh || exit 1
```

## 9. Production Checklist

- [ ] Multi-stage build (build stage ≠ runtime stage)
- [ ] Non-root user (`USER appuser`)
- [ ] Pinned base image version (no `:latest`)
- [ ] `.dockerignore` present and complete
- [ ] Health check defined
- [ ] Resource limits set (memory, CPU)
- [ ] Logging configured with rotation
- [ ] No secrets in ENV or build args
- [ ] No unnecessary packages installed
- [ ] Image scanned for vulnerabilities
- [ ] Volume mounts use named volumes or specific bind mounts
- [ ] `docker compose` file works with `--build`
