---
name: api-design
description: RESTful and GraphQL API design: OpenAPI specs, pagination, rate limiting, error schemas, versioning, and backwards compatibility.
---

# API Design Skill

Design clean, consistent, and maintainable APIs.

## RESTful Design Principles

- Use nouns for resources, HTTP verbs for actions
- Consistent URL patterns: `/resources/{id}/sub-resources`
- Proper HTTP status codes (200, 201, 204, 400, 401, 403, 404, 409, 422, 500)
- Stateless: each request contains all needed info
- HATEOAS: include links for discoverability

## API Response Schema

```json
{
  "data": {},
  "meta": { "page": 1, "per_page": 20, "total": 100 },
  "links": { "self": "...", "next": "...", "prev": "..." },
  "errors": []
}
```

## Error Schema

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Human-readable description",
    "details": [{"field": "email", "issue": "invalid format"}]
  }
}
```

## Key Patterns

- **Pagination**: cursor-based for large datasets, offset for small
- **Filtering**: query params `?status=active&created_after=2024-01-01`
- **Sorting**: `?sort=-created_at,name`
- **Rate Limiting**: 429 + Retry-After header
- **Versioning**: URL path (`/v2/`) or header
- **Idempotency**: idempotency keys for safe retries
