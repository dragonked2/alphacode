---
name: backend-dev/database
description: Database patterns — schema design, indexing strategies, query optimization, connection pooling, migration management, ORM vs raw SQL trade-offs, transaction isolation, Redis caching, full-text search, database seeding.
---

# Database Patterns

## Schema Design

### Normalization Rules

1. **First Normal Form (1NF)** — Each cell contains atomic values, no repeating groups
2. **Second Normal Form (2NF)** — 1NF + no partial dependencies on composite keys
3. **Third Normal Form (3NF)** — 2NF + no transitive dependencies

### When to Denormalize

- Read-heavy tables with expensive JOINs
- Reporting/analytics queries
- Caching computed aggregates
- High-throughput systems where JOINs are bottleneck

### Naming Conventions

```sql
-- Tables: plural, snake_case
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL UNIQUE,
    name VARCHAR(100) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Junction tables: table1_table2
CREATE TABLE user_roles (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, role_id)
);

-- Columns: snake_case, descriptive
-- Foreign keys: {table}_id
-- Indexes: idx_{table}_{columns}
-- Unique constraints: uq_{table}_{columns}
```

### UUID vs Auto-Increment

| Strategy | Pros | Cons |
|----------|------|------|
| UUID v4 | Globally unique, no coordination | 16 bytes, not sequential, index fragmentation |
| UUID v7 | Time-ordered, K-sortable | Requires library |
| BigInt | Sequential, small, fast | Requires coordination, enumerable |
| ULID | Time-ordered, sortable | 128-bit, requires library |

**Recommendation:** UUID v7 for distributed systems, BigInt for single-database.

## Indexing Strategies

### Index Types (PostgreSQL)

```sql
-- B-tree: Default, equality and range queries
CREATE INDEX idx_users_email ON users (email);
CREATE INDEX idx_orders_created_at ON orders (created_at DESC);

-- Hash: Equality only, faster than B-tree for exact matches
CREATE INDEX idx_sessions_token_hash ON sessions USING hash (token);

-- GIN: Full-text search, arrays, JSONB
CREATE INDEX idx_posts_content_fts ON posts USING gin (to_tsvector('english', content));
CREATE INDEX idx_products_tags ON products USING gin (tags);

-- GiST: Geometric, full-text search, range types
CREATE INDEX idx_locations_coords ON locations USING gist (coordinates);

-- Partial: Index only subset of rows
CREATE INDEX idx_active_users ON users (email) WHERE deleted_at IS NULL;
CREATE INDEX idx_pending_orders ON orders (created_at) WHERE status = 'pending';

-- Covering (INCLUDE): Index includes extra columns
CREATE INDEX idx_users_email_covering ON users (email) INCLUDE (name, role);

-- Expression: Index on computed value
CREATE INDEX idx_users_lower_email ON users (lower(email));
```

### Index Design Rules

1. **One index per query pattern** — Don't create composite indexes hoping they'll work
2. **Column order matters** — Put equality columns first, then range, then sort
3. **Selective columns first** — Most distinct values first for better filtering
4. **Monitor unused indexes** — Drop indexes that aren't used
5. **Avoid over-indexing** — Each index slows writes

### Composite Index Examples

```sql
-- Query: SELECT * FROM orders WHERE user_id = ? AND status = ? ORDER BY created_at DESC
CREATE INDEX idx_orders_user_status_created
ON orders (user_id, status, created_at DESC);

-- Query: SELECT * FROM products WHERE category = ? AND price > ? AND price < ?
CREATE INDEX idx_products_category_price
ON products (category_id, price);
```

## Query Optimization

### EXPLAIN ANALYZE

```sql
-- Basic analysis
EXPLAIN ANALYZE
SELECT u.name, COUNT(o.id) as order_count
FROM users u
JOIN orders o ON o.user_id = u.id
WHERE u.created_at > '2024-01-01'
GROUP BY u.id
ORDER BY order_count DESC
LIMIT 10;

-- Look for:
-- Seq Scan → Add index
-- Nested Loop with high row count → Consider JOIN reordering
-- Sort → Add index on ORDER BY columns
-- HashAggregate → Consider adding WHERE columns to index
```

### N+1 Prevention

```sql
-- BAD: N+1 queries
SELECT * FROM users;
-- Then for each user:
SELECT * FROM orders WHERE user_id = ?;

-- GOOD: Single query with JOIN
SELECT u.*, o.*
FROM users u
LEFT JOIN orders o ON o.user_id = u.id;

-- GOOD: Subquery for counts
SELECT u.*, (
    SELECT COUNT(*) FROM orders WHERE user_id = u.id
) as order_count
FROM users u;

-- GOOD: Lateral join for complex subqueries
SELECT u.*, latest_order.*
FROM users u
LEFT JOIN LATERAL (
    SELECT * FROM orders
    WHERE user_id = u.id
    ORDER BY created_at DESC
    LIMIT 1
) latest_order ON true;
```

### Common Performance Patterns

```sql
-- Cursor-based pagination (instead of OFFSET)
SELECT * FROM orders
WHERE id > $last_id  -- Bookmark from previous query
ORDER BY id
LIMIT 20;

-- Batch updates
UPDATE orders
SET status = 'archived'
WHERE id = ANY($1::uuid[]);

-- Materialized views for complex aggregates
CREATE MATERIALIZED VIEW monthly_sales AS
SELECT
    date_trunc('month', created_at) as month,
    SUM(amount) as total_sales,
    COUNT(*) as order_count
FROM orders
GROUP BY date_trunc('month', created_at);

-- Refresh periodically
REFRESH MATERIALIZED VIEW CONCURRENTLY monthly_sales;
```

## Connection Pooling

### PgBouncer Config

```ini
[databases]
mydb = host=localhost port=5432 dbname=mydb

[pgbouncer]
listen_addr = 0.0.0.0
listen_port = 6432
auth_type = md5
auth_file = /etc/pgbouncer/userlist.txt

# Pool settings
pool_mode = transaction  # or session
default_pool_size = 20
max_client_conn = 1000
min_pool_size = 5
reserve_pool_size = 5
reserve_pool_timeout = 3

# Timeouts
server_connect_timeout = 5
server_login_retry = 1
client_idle_timeout = 0
client_login_timeout = 60
query_timeout = 0
query_wait_timeout = 120
```

### Application-Level Pooling (sqlx)

```rust
// Rust
let pool = PgPoolOptions::new()
    .max_connections(20)        // Match PgBouncer pool size
    .min_connections(5)
    .acquire_timeout(Duration::from_secs(5))
    .idle_timeout(Duration::from_secs(300))
    .max_lifetime(Duration::from_secs(1800))
    .connect(&database_url)
    .await?;
```

```python
# Python (SQLAlchemy)
engine = create_async_engine(
    DATABASE_URL,
    pool_size=20,
    max_overflow=10,
    pool_timeout=5,
    pool_recycle=1800,
    pool_pre_ping=True,
)
```

```typescript
// Node.js (Prisma)
datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

generator client {
  provider = "prisma-client-js"
}

// prisma/
model User {
  id    String @id @default(uuid())
  email String @unique
}
```

## Migration Management

### Version Control Strategy

```
migrations/
├── 001_initial/
│   ├── up.sql
│   └── down.sql
├── 002_add_user_roles/
│   ├── up.sql
│   └── down.sql
└── 003_create_orders/
    ├── up.sql
    └── down.sql
```

### Alembic (Python)

```bash
# Generate migration
alembic revision --autogenerate -m "add_user_roles"

# Apply
alembic upgrade head

# Rollback one
alembic downgrade -1

# Rollback to specific version
alembic downgrade 002

# Show current
alembic current

# Show history
alembic history
```

### Rollback Strategy

```sql
-- Always write reversible migrations
-- up.sql
CREATE TABLE orders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    amount DECIMAL(10,2) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_orders_user_id ON orders (user_id);
CREATE INDEX idx_orders_status ON orders (status);

-- down.sql
DROP INDEX IF EXISTS idx_orders_status;
DROP INDEX IF EXISTS idx_orders_user_id;
DROP TABLE IF EXISTS orders;
```

### Migration Rules

1. **Never modify applied migrations** — Create new ones
2. **Always include down.sql** — Enables rollback
3. **Test rollbacks** — Before deploying
4. **Separate schema and data** — Schema migrations vs data migrations
5. **Add columns as nullable** — Then backfill, then set NOT NULL
6. **Deploy zero-downtime** — Use expand/contract pattern

## ORM vs Raw SQL Trade-offs

| Aspect | ORM | Raw SQL |
|--------|-----|---------|
| Development speed | Fast | Slow |
| Type safety | Good | Excellent |
| Complex queries | Limited | Full control |
| Performance | Optimized by library | Manual optimization |
| Learning curve | Low-medium | High |
| Debugging | Harder (generated SQL) | Easier |
| Database portability | Good | Manual |

**Recommendation:** Use ORM for CRUD, raw SQL for complex queries and performance-critical paths.

## Transaction Isolation Levels

```sql
-- Read Uncommitted: Dirty reads allowed (rarely used)
SET TRANSACTION ISOLATION LEVEL READ UNCOMMITTED;

-- Read Committed: Default in PostgreSQL
-- Non-repeatable reads possible
SET TRANSACTION ISOLATION LEVEL READ COMMITTED;

-- Repeatable Read: Prevents non-repeatable reads
-- Phantom reads possible
SET TRANSACTION ISOLATION LEVEL REPEATABLE READ;

-- Serializable: Full isolation, prevents all anomalies
-- May cause performance issues
SET TRANSACTION ISOLATION LEVEL SERIALIZABLE;
```

### When to Use Each Level

| Level | Use Case |
|-------|----------|
| Read Committed | Most applications |
| Repeatable Read | Financial calculations, inventory checks |
| Serializable | Auction systems, booking systems |

## Redis Caching Patterns

```python
# Cache-Aside Pattern
async def get_user(user_id: str):
    # 1. Check cache
    cache_key = f"user:{user_id}"
    cached = await redis.get(cache_key)
    if cached:
        return json.loads(cached)

    # 2. Query database
    user = await db.fetch_user(user_id)

    # 3. Populate cache
    await redis.setex(cache_key, 3600, json.dumps(user))

    return user


# Write-Through Pattern
async def update_user(user_id: str, data: dict):
    # 1. Update database
    await db.update_user(user_id, data)

    # 2. Update cache
    cache_key = f"user:{user_id}"
    user = await db.fetch_user(user_id)
    await redis.setex(cache_key, 3600, json.dumps(user))


# Cache Invalidation
async def delete_user(user_id: str):
    await db.delete_user(user_id)
    await redis.delete(f"user:{user_id}")


# Cache Stampede Prevention (Mutex)
async def get_user_with_lock(user_id: str):
    cache_key = f"user:{user_id}"
    lock_key = f"lock:{cache_key}"

    cached = await redis.get(cache_key)
    if cached:
        return json.loads(cached)

    # Try to acquire lock
    acquired = await redis.set(lock_key, "1", nx=True, ex=10)
    if acquired:
        try:
            user = await db.fetch_user(user_id)
            await redis.setex(cache_key, 3600, json.dumps(user))
            return user
        finally:
            await redis.delete(lock_key)
    else:
        # Wait and retry
        await asyncio.sleep(0.1)
        return await get_user_with_lock(user_id)
```

## Full-Text Search

### PostgreSQL

```sql
-- Add tsvector column
ALTER TABLE posts ADD COLUMN content_tsv tsvector;

-- Populate
UPDATE posts SET content_tsv = to_tsvector('english', content);

-- Create GIN index
CREATE INDEX idx_posts_content_tsv ON posts USING gin (content_tsv);

-- Query
SELECT * FROM posts
WHERE content_tsv @@ to_tsquery('english', 'backend & development');

-- Rank results
SELECT *, ts_rank(content_tsv, to_tsquery('english', 'backend & development')) as rank
FROM posts
WHERE content_tsv @@ to_tsquery('english', 'backend & development')
ORDER BY rank DESC;
```

### Elasticsearch (for complex search)

```python
from elasticsearch import AsyncElasticsearch

es = AsyncElasticsearch(["http://localhost:9200"])

# Index document
await es.index(index="posts", id=post.id, document={
    "title": post.title,
    "content": post.content,
    "created_at": post.created_at.isoformat(),
})

# Search
results = await es.search(
    index="posts",
    query={
        "multi_match": {
            "query": "backend development",
            "fields": ["title^2", "content"],
            "fuzziness": "AUTO",
        }
    },
    highlight={
        "fields": {
            "content": {"fragment_size": 150, "number_of_fragments": 3}
        }
    },
)
```

## Database Seeding

```python
# seeds/seed.py
import asyncio
from faker import Faker
from app.db.session import AsyncSessionLocal
from app.models.database import User, Product

fake = Faker()

async def seed():
    async with AsyncSessionLocal() as db:
        # Create admin user
        admin = User(
            email="admin@example.com",
            name="Admin User",
            role="admin",
            hashed_password=hash_password("admin123"),
        )
        db.add(admin)

        # Create test users
        for _ in range(100):
            user = User(
                email=fake.email(),
                name=fake.name(),
                role="user",
                hashed_password=hash_password("password123"),
            )
            db.add(user)

        await db.commit()
        print("Seeded 100 users")

if __name__ == "__main__":
    asyncio.run(seed())
```

## Checklist

- [ ] Schema follows normalization rules (or justified denormalization)
- [ ] Indexes match query patterns
- [ ] EXPLAIN ANALYZE used on critical queries
- [ ] Connection pooling configured
- [ ] Migrations are reversible
- [ ] Migration rollbacks tested
- [ ] Cache patterns implemented
- [ ] Full-text search indexed
- [ ] Seed data available for dev/test
