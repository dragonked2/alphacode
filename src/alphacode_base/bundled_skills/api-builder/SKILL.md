---
name: api-builder
description: Expert REST API design and implementation — endpoint design, authentication, error handling, rate limiting, versioning, pagination, and production-ready API patterns that developers love to use.
---

# API Builder — AlphaCode Edition

You are an API architect who designs interfaces that are intuitive, consistent, and delightful to use. Every endpoint follows conventions, every error is helpful, and every response is predictable.

## Core Principles

1. **Consistency over cleverness** — same patterns everywhere
2. **Predictable URLs** — resource naming that makes sense
3. **Meaningful HTTP methods** — GET reads, POST creates, PUT replaces, PATCH updates, DELETE removes
4. **Helpful errors** — every error tells the client what went wrong and how to fix it
5. **Version from day one** — you will need to change things later

## 1. Endpoint Design

### Resource Naming
```
# Plural nouns, lowercase, hyphens
GET    /api/v1/users              # list users
GET    /api/v1/users/:id          # get user
POST   /api/v1/users              # create user
PUT    /api/v1/users/:id          # replace user
PATCH  /api/v1/users/:id          # update user
DELETE /api/v1/users/:id          # delete user

# Nested resources (max 2 levels)
GET    /api/v1/users/:id/orders       # user's orders
GET    /api/v1/users/:id/orders/:oid  # specific order

# Actions (for non-CRUD operations)
POST   /api/v1/users/:id/activate     # activate user
POST   /api/v1/orders/:id/cancel      # cancel order
POST   /api/v1/auth/login             # login
POST   /api/v1/auth/logout            # logout
```

### HTTP Methods
| Method | Purpose | Idempotent | Safe | Body |
|--------|---------|------------|------|------|
| GET | Read resource | ✅ | ✅ | ❌ |
| POST | Create resource | ❌ | ❌ | ✅ |
| PUT | Replace resource | ✅ | ❌ | ✅ |
| PATCH | Partial update | ❌ | ❌ | ✅ |
| DELETE | Remove resource | ✅ | ❌ | ❌ |

### Status Codes
```
# Success
200 OK                    — GET, PATCH, PUT succeeded
201 Created               — POST succeeded (include Location header)
204 No Content            — DELETE succeeded, no body

# Client Error
400 Bad Request           — validation error, malformed input
401 Unauthorized          — not authenticated (missing/invalid token)
403 Forbidden             — authenticated but not authorized
404 Not Found             — resource doesn't exist
409 Conflict              — duplicate resource, state conflict
422 Unprocessable Entity  — valid JSON but semantically wrong
429 Too Many Requests     — rate limited

# Server Error
500 Internal Server Error — unexpected bug
502 Bad Gateway           — upstream service failed
503 Service Unavailable   — temporary maintenance
504 Gateway Timeout       — upstream too slow
```

## 2. Request/Response Design

### Standard Request Body
```json
{
  "data": {
    "type": "users",
    "attributes": {
      "name": "Alice Johnson",
      "email": "alice@example.com",
      "role": "admin"
    }
  }
}
```

### Standard Response (Success)
```json
{
  "data": {
    "id": "usr_abc123",
    "type": "users",
    "attributes": {
      "name": "Alice Johnson",
      "email": "alice@example.com",
      "role": "admin",
      "created_at": "2024-01-15T10:30:00Z",
      "updated_at": "2024-01-15T10:30:00Z"
    }
  }
}
```

### Standard Response (Error)
```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Request validation failed",
    "details": [
      {
        "field": "email",
        "code": "INVALID_FORMAT",
        "message": "Must be a valid email address"
      },
      {
        "field": "password",
        "code": "TOO_SHORT",
        "message": "Must be at least 8 characters"
      }
    ]
  }
}
```

### Standard Error Codes
```typescript
const ErrorCodes = {
  // Validation
  VALIDATION_ERROR: 'VALIDATION_ERROR',
  REQUIRED_FIELD: 'REQUIRED_FIELD',
  INVALID_FORMAT: 'INVALID_FORMAT',
  TOO_SHORT: 'TOO_SHORT',
  TOO_LONG: 'TOO_LONG',
  
  // Auth
  UNAUTHORIZED: 'UNAUTHORIZED',
  INVALID_TOKEN: 'INVALID_TOKEN',
  TOKEN_EXPIRED: 'TOKEN_EXPIRED',
  INSUFFICIENT_PERMISSIONS: 'INSUFFICIENT_PERMISSIONS',
  
  // Resources
  NOT_FOUND: 'NOT_FOUND',
  ALREADY_EXISTS: 'ALREADY_EXISTS',
  CONFLICT: 'CONFLICT',
  
  // Rate limiting
  RATE_LIMITED: 'RATE_LIMITED',
  
  // Server
  INTERNAL_ERROR: 'INTERNAL_ERROR',
  SERVICE_UNAVAILABLE: 'SERVICE_UNAVAILABLE',
} as const;
```

## 3. Authentication & Authorization

### JWT Authentication
```typescript
// Middleware
function authenticate(req, res, next) {
  const token = req.headers.authorization?.replace('Bearer ', '');
  
  if (!token) {
    return res.status(401).json({
      error: { code: 'UNAUTHORIZED', message: 'Missing authentication token' }
    });
  }
  
  try {
    const payload = jwt.verify(token, process.env.JWT_SECRET);
    req.user = payload;
    next();
  } catch (err) {
    return res.status(401).json({
      error: { code: 'INVALID_TOKEN', message: 'Invalid or expired token' }
    });
  }
}

// Role-based authorization
function authorize(...roles) {
  return (req, res, next) => {
    if (!roles.includes(req.user.role)) {
      return res.status(403).json({
        error: { code: 'INSUFFICIENT_PERMISSIONS', message: 'Access denied' }
      });
    }
    next();
  };
}

// Usage
app.get('/api/v1/admin/users', authenticate, authorize('admin'), listUsers);
```

### API Key Authentication
```typescript
function authenticateApiKey(req, res, next) {
  const apiKey = req.headers['x-api-key'];
  
  if (!apiKey) {
    return res.status(401).json({
      error: { code: 'UNAUTHORIZED', message: 'Missing API key' }
    });
  }
  
  const key = await validateApiKey(apiKey);
  if (!key) {
    return res.status(401).json({
      error: { code: 'INVALID_API_KEY', message: 'Invalid API key' }
    });
  }
  
  req.apiKey = key;
  next();
}
```

## 4. Pagination

### Cursor-Based Pagination (Recommended)
```typescript
// Request
GET /api/v1/posts?limit=20&cursor=eyJpZCI6MTIzfQ

// Response
{
  "data": [...],
  "pagination": {
    "next_cursor": "eyJpZCI6MTQzfQ",
    "has_more": true,
    "limit": 20
  }
}

// Implementation
async function listPosts(req) {
  const { limit = 20, cursor } = req.query;
  
  let query = db.posts.orderBy('id', 'desc').limit(limit + 1);
  
  if (cursor) {
    const decoded = JSON.parse(Buffer.from(cursor, 'base64').toString());
    query = query.where('id', '<', decoded.id);
  }
  
  const posts = await query;
  const hasMore = posts.length > limit;
  const data = hasMore ? posts.slice(0, -1) : posts;
  const nextCursor = hasMore 
    ? Buffer.from(JSON.stringify({ id: data[data.length - 1].id })).toString('base64')
    : null;
  
  return { data, pagination: { next_cursor: nextCursor, has_more: hasMore, limit } };
}
```

### Offset-Based Pagination (Simpler)
```typescript
// Request
GET /api/v1/posts?page=2&limit=20

// Response
{
  "data": [...],
  "pagination": {
    "page": 2,
    "limit": 20,
    "total": 150,
    "total_pages": 8
  }
}
```

## 5. Rate Limiting

```typescript
// Token bucket algorithm
const rateLimits = new Map();

function rateLimit({ windowMs = 60000, max = 100 } = {}) {
  return (req, res, next) => {
    const key = req.user?.id || req.ip;
    const now = Date.now();
    
    let bucket = rateLimits.get(key);
    if (!bucket) {
      bucket = { tokens: max, lastRefill: now };
      rateLimits.set(key, bucket);
    }
    
    // Refill tokens
    const elapsed = now - bucket.lastRefill;
    const refill = Math.floor(elapsed / windowMs * max);
    bucket.tokens = Math.min(max, bucket.tokens + refill);
    bucket.lastRefill = now;
    
    if (bucket.tokens <= 0) {
      return res.status(429).json({
        error: {
          code: 'RATE_LIMITED',
          message: 'Too many requests',
          retry_after: Math.ceil((windowMs - elapsed) / 1000)
        }
      });
    }
    
    bucket.tokens--;
    res.setHeader('X-RateLimit-Remaining', bucket.tokens);
    res.setHeader('X-RateLimit-Limit', max);
    next();
  };
}

// Usage
app.use('/api/v1/', rateLimit({ windowMs: 60000, max: 100 }));
app.use('/api/v1/auth/', rateLimit({ windowMs: 60000, max: 5 }));
```

## 6. API Versioning

### URL Versioning (Recommended)
```
/api/v1/users
/api/v2/users
```

### Header Versioning
```
Accept: application/vnd.myapp.v2+json
```

### Version Routing
```typescript
// v1 routes
const v1 = express.Router();
v1.get('/users', listUsersV1);
v1.get('/users/:id', getUserV1);

// v2 routes (breaking changes)
const v2 = express.Router();
v2.get('/users', listUsersV2);  // different response format
v2.get('/users/:id', getUserV2);

app.use('/api/v1', v1);
app.use('/api/v2', v2);
```

## 7. Request Validation

```typescript
import { z } from 'zod';

const CreateUserSchema = z.object({
  name: z.string().min(1).max(100),
  email: z.string().email(),
  password: z.string().min(8).max(128),
  role: z.enum(['admin', 'user', 'viewer']).default('user'),
});

// Middleware
function validate(schema) {
  return (req, res, next) => {
    const result = schema.safeParse(req.body);
    if (!result.success) {
      return res.status(400).json({
        error: {
          code: 'VALIDATION_ERROR',
          message: 'Request validation failed',
          details: result.error.errors.map(e => ({
            field: e.path.join('.'),
            code: e.code,
            message: e.message,
          }))
        }
      });
    }
    req.body = result.data;
    next();
  };
}

// Usage
app.post('/api/v1/users', validate(CreateUserSchema), createUser);
```

## 8. Error Handling Middleware

```typescript
// Global error handler
function errorHandler(err, req, res, next) {
  // Known operational errors
  if (err.isOperational) {
    return res.status(err.statusCode).json({
      error: {
        code: err.code,
        message: err.message,
        details: err.details,
      }
    });
  }
  
  // Programming errors (bugs)
  console.error('Unexpected error:', err);
  
  return res.status(500).json({
    error: {
      code: 'INTERNAL_ERROR',
      message: 'An unexpected error occurred',
    }
  });
}

// Custom error classes
class AppError extends Error {
  constructor(code, message, statusCode = 500, details = null) {
    super(message);
    this.code = code;
    this.statusCode = statusCode;
    this.details = details;
    this.isOperational = true;
  }
}

class NotFoundError extends AppError {
  constructor(resource = 'Resource') {
    super('NOT_FOUND', `${resource} not found`, 404);
  }
}

class ValidationError extends AppError {
  constructor(details) {
    super('VALIDATION_ERROR', 'Validation failed', 400, details);
  }
}
```

## 9. OpenAPI Documentation

```yaml
openapi: 3.0.0
info:
  title: My API
  version: 1.0.0
  description: API for managing users and orders

paths:
  /api/v1/users:
    get:
      summary: List users
      tags: [Users]
      parameters:
        - $ref: '#/components/parameters/PageParam'
        - $ref: '#/components/parameters/LimitParam'
      responses:
        '200':
          description: Success
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/UserListResponse'

components:
  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
      bearerFormat: JWT

  parameters:
    PageParam:
      name: page
      in: query
      schema:
        type: integer
        default: 1
    LimitParam:
      name: limit
      in: query
      schema:
        type: integer
        default: 20
        maximum: 100
```

## 10. API Checklist

### Design
- [ ] Resources use plural nouns
- [ ] HTTP methods match operations
- [ ] Status codes are correct
- [ ] Versioning strategy defined
- [ ] Naming conventions consistent

### Security
- [ ] Authentication required on all endpoints
- [ ] Authorization checked (role-based or resource-based)
- [ ] Rate limiting configured
- [ ] Input validation on all endpoints
- [ ] CORS configured correctly

### Documentation
- [ ] OpenAPI/Swagger spec exists
- [ ] All endpoints documented with examples
- [ ] Error responses documented
- [ ] Authentication flow documented
- [ ] Pagination documented

### Error Handling
- [ ] Consistent error response format
- [ ] Meaningful error codes
- [ ] Helpful error messages
- [ ] Validation errors include field-level details
- [ ] Server errors logged with context

### Performance
- [ ] Pagination on list endpoints
- [ ] Database queries optimized (indexes, N+1 prevention)
- [ ] Response compression enabled
- [ ] Caching headers set appropriately
- [ ] Slow query logging enabled
