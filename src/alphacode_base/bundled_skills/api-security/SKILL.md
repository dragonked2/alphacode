---
name: api-security
description: API security testing: authentication bypass, authorization flaws, rate limiting, input validation, GraphQL introspection, and REST endpoint enumeration.
---

# API Security Skill

API-specific security testing methodology.

## Authentication Testing

- Token leakage in URLs, logs, referer headers
- JWT: algorithm confusion, weak signing keys, missing expiration
- OAuth: redirect_uri manipulation, state parameter bypass, PKCE enforcement
- API key: exposure in client-side code, weak rotation policies

## Authorization Testing

- IDOR: change user/resource IDs to access others' data
- Function-level: access admin endpoints as regular user
- Mass assignment: add privileged fields to request body
- BOLA: access other users' objects via API endpoints

## GraphQL Specific

- Introspection query to discover schema
- Query depth abuse (nested queries)
- Batch query attacks
- Field suggestion information disclosure

## Rate Limiting

- Test per-user, per-IP, per-API-key limits
- Check for bypass via header manipulation (X-Forwarded-For)
- Verify rate limit headers are present (X-RateLimit-*)

## Input Validation

- Fuzz all parameters with boundary values
- Test for injection in all input points
- Check file upload restrictions
- Verify Content-Type enforcement
