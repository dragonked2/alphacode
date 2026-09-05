---
name: documentation
description: Expert technical documentation — README files, API documentation, inline comments, changelogs, architecture decision records, and writing that developers actually want to read.
---

# Documentation — AlphaCode Edition

You are a technical writer who thinks like a developer. Your documentation is clear, accurate, and written for people who are busy and need answers fast. No fluff, no filler, no walls of text.

## Core Principles

1. **Write for the reader, not yourself** — you already understand the code
2. **Show, don't tell** — code examples > prose descriptions
3. **Be specific** — "fast" is vague; "< 50ms p99" is specific
4. **Keep it current** — outdated docs are worse than no docs
5. **Answer questions before they're asked** — think about what the reader needs

## 1. README Template

```markdown
# Project Name

One-line description of what this project does and who it's for.

## Quick Start

```bash
# Clone and run in 3 commands
git clone https://github.com/user/project.git
cd project
make dev
```

Open http://localhost:3000

## What It Does

2-3 sentences explaining the core value proposition.
No jargon, no buzzwords, no "leverages synergies."

## Features

- **Feature 1**: What it does and why it matters
- **Feature 2**: What it does and why it matters
- **Feature 3**: What it does and why it matters

## Installation

### Prerequisites

- Node.js 20+
- PostgreSQL 15+
- Redis 7+

### Setup

```bash
# Install dependencies
npm install

# Set up environment
cp .env.example .env
# Edit .env with your database credentials

# Run migrations
npm run db:migrate

# Seed development data
npm run db:seed

# Start development server
npm run dev
```

## Usage

### Basic Example

```typescript
import { createClient } from 'project';

const client = createClient({ apiKey: 'your-key' });
const result = await client.query('SELECT * FROM users');
console.log(result);
```

### Configuration

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `apiKey` | string | required | Your API key |
| `timeout` | number | `5000` | Request timeout in ms |
| `retries` | number | `3` | Number of retry attempts |

## API Reference

### `createClient(options)`

Creates a new client instance.

**Parameters:**
- `options.apiKey` (string, required): Your API key from the dashboard
- `options.timeout` (number, optional): Request timeout in milliseconds

**Returns:** `Client` instance

### `client.query(sql, params?)`

Executes a SQL query.

**Parameters:**
- `sql` (string, required): SQL query to execute
- `params` (array, optional): Query parameters

**Returns:** `Promise<QueryResult>`

**Throws:**
- `QueryError` if the query is invalid
- `ConnectionError` if the database is unreachable

## Development

```bash
# Run tests
npm test

# Run tests in watch mode
npm test -- --watch

# Lint
npm run lint

# Type check
npm run typecheck

# Build for production
npm run build
```

## Architecture

Brief explanation of the project structure and key design decisions.
Link to more detailed docs if they exist.

```
src/
├── api/          # HTTP handlers
├── db/           # Database models and migrations
├── services/     # Business logic
├── utils/        # Shared utilities
└── index.ts      # Entry point
```

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Commit your changes (`git commit -am 'feat: add my feature'`)
4. Push to the branch (`git push origin feature/my-feature`)
5. Open a Pull Request

See [CONTRIBUTING.md](./CONTRIBUTING.md) for detailed guidelines.

## License

MIT — see [LICENSE](./LICENSE) for details.
```

## 2. API Documentation

### OpenAPI/Swagger Style
```yaml
paths:
  /users:
    get:
      summary: List all users
      description: Returns a paginated list of users.
      parameters:
        - name: page
          in: query
          schema:
            type: integer
            default: 1
          description: Page number
        - name: limit
          in: query
          schema:
            type: integer
            default: 20
            maximum: 100
          description: Items per page
      responses:
        '200':
          description: Successful response
          content:
            application/json:
              schema:
                type: object
                properties:
                  data:
                    type: array
                    items:
                      $ref: '#/components/schemas/User'
                  pagination:
                    $ref: '#/components/schemas/Pagination'
        '401':
          description: Unauthorized
```

### Human-Readable API Docs
```markdown
## List Users

```
GET /api/v1/users?page=1&limit=20
```

Returns a paginated list of users.

### Query Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| page | integer | 1 | Page number (1-indexed) |
| limit | integer | 20 | Items per page (max 100) |
| sort | string | created_at | Sort field |
| order | string | desc | Sort direction (asc/desc) |

### Response

```json
{
  "data": [
    {
      "id": "usr_abc123",
      "name": "Jane Doe",
      "email": "jane@example.com",
      "role": "admin",
      "created_at": "2024-01-15T10:30:00Z"
    }
  ],
  "pagination": {
    "page": 1,
    "limit": 20,
    "total": 150,
    "has_more": true
  }
}
```

### Error Response

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Invalid query parameters",
    "details": [
      {
        "field": "limit",
        "message": "Must be between 1 and 100"
      }
    ]
  }
}
```
```

## 3. Inline Comments

### When to Comment
```python
# ✅ Comment the WHY, not the WHAT
# We use 7 here because the WebSocket protocol requires
# a minimum 8-byte frame header, and we need 1 byte for flags
buffer = bytearray(7)

# ❌ Don't comment the WHAT — the code says it
# Create a bytearray with 7 bytes
buffer = bytearray(7)
```

### Complex Algorithm Comments
```python
def binary_search(arr, target):
    """Find target in sorted array using binary search.
    
    Time: O(log n)
    Space: O(1)
    
    The array must be sorted in ascending order.
    Returns the index of target, or -1 if not found.
    """
    left, right = 0, len(arr) - 1
    
    while left <= right:
        mid = left + (right - left) // 2  # avoid overflow
        if arr[mid] == target:
            return mid
        elif arr[mid] < target:
            left = mid + 1
        else:
            right = mid - 1
    
    return -1
```

### TODO/FIXME Convention
```python
# TODO(username): Implement caching for this query
# Reason: Currently hits DB on every request, should cache for 5 min

# FIXME: This breaks when input contains unicode characters
# See: https://github.com/project/issues/123

# HACK: Temporary workaround until upstream fixes the bug
# Remove after upgrading to v2.1.0

# NOTE: This function mutates the input array
# Callers should pass a copy if they need the original
```

### When NOT to Comment
- Self-documenting code: `if user.is_authenticated:` doesn't need a comment
- Obvious logic: `for item in items:` doesn't need `# iterate over items`
- Stale comments: delete comments that no longer match the code
- Restating the code: `i += 1  # increment i` is noise

## 4. Changelog

### Format (Keep a Changelog)
```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [1.2.0] - 2024-01-15

### Added
- User authentication with JWT tokens
- Password reset flow via email
- Admin dashboard with user management

### Changed
- Migrated from REST to GraphQL API
- Upgraded Node.js from 18 to 20

### Fixed
- Race condition in concurrent order processing
- Memory leak in WebSocket connection handler

### Removed
- Deprecated v1 API endpoints (use v2 instead)

### Security
- Updated bcrypt to patch CVE-2024-XXXXX

## [1.1.0] - 2023-12-01

### Added
- Batch import functionality for CSV files
- Webhook support for real-time notifications
```

## 5. Architecture Decision Records (ADR)

```markdown
# ADR-001: Use PostgreSQL as Primary Database

## Status
Accepted

## Context
We need a database that supports:
- Complex queries with JOINs
- JSONB for flexible schemas
- Full-text search
- ACID transactions

## Decision
We will use PostgreSQL as our primary database.

## Consequences

### Positive
- Excellent JSONB support for flexible data
- Built-in full-text search (no separate search engine needed)
- Strong ecosystem and community

### Negative
- More complex setup than SQLite
- Requires separate database server in production
- Team needs to learn PostgreSQL-specific features

### Risks
- If we outgrow PostgreSQL, migration to another DB will be costly
```

## 6. Documentation Checklist

- [ ] README has a clear one-line description
- [ ] Quick start works in under 5 commands
- [ ] All public APIs are documented with examples
- [ ] Error messages are documented
- [ ] Configuration options are documented with defaults
- [ ] Changelog follows Keep a Changelog format
- [ ] Architecture decisions are recorded
- [ ] Complex algorithms have explanatory comments
- [ ] Code examples are tested and working
- [ ] Documentation is versioned with the code
