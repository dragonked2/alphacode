---
name: backend-dev
description: Comprehensive backend development methodology covering API design, authentication, database design, caching, testing, deployment, and monitoring. Full lifecycle from requirements through production.
---

# Backend Development Skill Suite

## Overview

This skill provides a complete backend development methodology following the full lifecycle:

```
Requirements → Architecture → Implementation → Testing → Deployment → Monitoring
```

## When to Use

- Building RESTful or GraphQL APIs
- Designing microservices or monolithic backends
- Implementing authentication/authorization systems
- Database schema design and optimization
- Caching strategy implementation
- API security hardening
- Production deployment and monitoring

## Lifecycle Phases

### Phase 1: Requirements & Architecture

1. **Define API contract first** — Write OpenAPI/Swagger spec before code
2. **Choose technology stack** — Match to team expertise and project needs
3. **Design data model** — ERD before implementation
4. **Plan authentication** — JWT, OAuth2, sessions based on use case
5. **Identify integration points** — Third-party APIs, message queues, caches

### Phase 2: Implementation

1. **Project structure** — Follow language-specific conventions
2. **Error handling** — Structured errors from day one
3. **Validation** — Input validation at API boundary
4. **Logging** — Structured, correlation IDs, log levels
5. **Security** — Auth middleware, rate limiting, CORS, input sanitization

### Phase 3: Testing

1. **Unit tests** — Business logic, utilities, helpers
2. **Integration tests** — API endpoints, database operations
3. **Contract tests** — API spec compliance
4. **Load tests** — Performance baseline
5. **Security tests** — OWASP Top 10 checklist

### Phase 4: Deployment

1. **Docker** — Multi-stage builds, minimal images
2. **CI/CD** — Automated testing, linting, building
3. **Configuration** — Environment variables, secrets management
4. **Health checks** — Liveness and readiness probes
5. **Graceful shutdown** — Handle SIGTERM, drain connections

### Phase 5: Monitoring

1. **Metrics** — Request rate, error rate, latency (RED method)
2. **Logging** — Centralized, searchable, structured
3. **Tracing** — Distributed tracing across services
4. **Alerting** — On error spikes, latency degradation
5. **Dashboards** — Operational visibility

## Core Principles

- **Defense in depth** — Never trust a single layer
- **Fail fast, fail safe** — Validate early, handle errors gracefully
- **Observability by default** — Log, trace, and measure everything
- **Stateless where possible** — Horizontal scalability
- **Security is not optional** — Every endpoint must be secured

## Technology Sub-Skills

Select the appropriate sub-skill based on your stack:

| Sub-Skill | When to Use |
|-----------|-------------|
| `nodejs` | Node.js projects with Express, Fastify, or NestJS |
| `python` | Python projects with FastAPI, Django, or Flask |
| `rust` | Rust projects with Axum, Actix-web, or Rocket |
| `database` | Database schema, queries, optimization (any stack) |
| `auth` | Authentication and authorization (any stack) |
| `caching` | Caching strategies (any stack) |

## Checklist

- [ ] API contract defined (OpenAPI spec)
- [ ] Authentication strategy chosen and implemented
- [ ] Database schema designed and migrated
- [ ] Input validation on all endpoints
- [ ] Error handling with proper HTTP status codes
- [ ] Rate limiting implemented
- [ ] Structured logging with correlation IDs
- [ ] Health check endpoint
- [ ] Graceful shutdown handling
- [ ] Unit and integration tests written
- [ ] Docker build working
- [ ] CI/CD pipeline configured
- [ ] Monitoring and alerting in place
