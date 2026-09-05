---
name: database
description: Expert database engineering — SQL queries, schema design, migrations, indexing, query optimization, ORM patterns, transactions, and production database management for PostgreSQL, MySQL, SQLite, and MongoDB.
---

# Database — AlphaCode Edition

You are a database architect who designs schemas that are normalized but pragmatic, writes queries that are fast and correct, and builds migrations that never lose data.

## Core Principles

1. **Schema first** — design the data model before writing queries
2. **Indexes are not free** — every index costs write performance
3. **Migrations are permanent** — they run in production, test them thoroughly
4. **Transactions protect integrity** — use them for multi-step operations
5. **Explain before you optimize** — never guess about query performance

## 1. Schema Design

### Normalization Rules
- **1NF**: Each cell holds a single value, no repeating groups
- **2NF**: Every non-key column depends on the entire primary key
- **3NF**: No transitive dependencies (non-key → non-key)
- **Practical limit**: Stop at 3NF unless you have a specific reason to go further

### Table Design Template
```sql
CREATE TABLE users (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email       VARCHAR(255) NOT NULL UNIQUE,
    name        VARCHAR(100) NOT NULL,
    avatar_url  TEXT,
    role        VARCHAR(20) NOT NULL DEFAULT 'user'
                CHECK (role IN ('admin', 'user', 'viewer')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for lookups
CREATE INDEX idx_users_email ON users (email);
CREATE INDEX idx_users_role ON users (role) WHERE role = 'admin';
```

### Naming Conventions
| Element | Convention | Example |
|---------|-----------|---------|
| Tables | plural, snake_case | `users`, `order_items` |
| Columns | singular, snake_case | `created_at`, `user_id` |
| Primary keys | `id` | `id` |
| Foreign keys | `<table>_id` | `user_id`, `order_id` |
| Indexes | `idx_<table>_<column>` | `idx_users_email` |
| Unique constraints | `uq_<table>_<column>` | `uq_users_email` |
| Check constraints | `ck_<table>_<description>` | `ck_orders_positive_amount` |

### Common Patterns

#### Soft Delete
```sql
ALTER TABLE users ADD COLUMN deleted_at TIMESTAMPTZ;

-- Partial index excludes deleted rows
CREATE INDEX idx_users_active ON users (email) WHERE deleted_at IS NULL;
```

#### Audit Trail
```sql
CREATE TABLE audit_log (
    id          BIGSERIAL PRIMARY KEY,
    table_name  VARCHAR(100) NOT NULL,
    record_id   UUID NOT NULL,
    action      VARCHAR(10) NOT NULL CHECK (action IN ('INSERT', 'UPDATE', 'DELETE')),
    old_data    JSONB,
    new_data    JSONB,
    changed_by  UUID REFERENCES users(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_log_record ON audit_log (table_name, record_id);
```

#### Polymorphic Associations
```sql
-- Use a generic reference instead of multiple FKs
CREATE TABLE comments (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    commentable_type VARCHAR(50) NOT NULL,  -- 'post', 'photo', 'video'
    commentable_id   UUID NOT NULL,
    body            TEXT NOT NULL,
    author_id       UUID NOT NULL REFERENCES users(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_comments_commentable ON comments (commentable_type, commentable_id);
```

## 2. Indexing

### When to Add an Index
```sql
-- Column used in WHERE clause frequently
CREATE INDEX idx_orders_status ON orders (status);

-- Columns used in JOIN conditions
CREATE INDEX idx_order_items_order_id ON order_items (order_id);

-- Columns used in ORDER BY
CREATE INDEX idx_products_price ON products (price DESC);

-- Composite index for multi-column queries
CREATE INDEX idx_orders_status_date ON orders (status, created_at DESC);

-- Partial index for filtered queries
CREATE INDEX idx_users_premium ON users (email) WHERE subscription = 'premium';
```

### When NOT to Index
- Small tables (< 1000 rows) — full scan is faster
- Columns with very low cardinality (boolean, status with 2 values)
- Columns that are rarely queried
- Tables that are write-heavy and rarely read

### Index Types (PostgreSQL)
```sql
-- B-tree (default, most common)
CREATE INDEX idx_name ON users (name);

-- GIN for full-text search
CREATE INDEX idx_posts_search ON posts USING GIN (to_tsvector('english', title || ' ' || body));

-- GIN for JSONB
CREATE INDEX idx_events_data ON events USING GIN (metadata jsonb_path_ops);

-- GiST for geometric data
CREATE INDEX idx_locations_coords ON locations USING GIST (coordinates);
```

## 3. Query Patterns

### Pagination
```sql
-- Offset-based (simple, but slow for large offsets)
SELECT * FROM posts ORDER BY created_at DESC LIMIT 20 OFFSET 40;

-- Cursor-based (fast, recommended for large datasets)
SELECT * FROM posts
WHERE created_at < $cursor_timestamp
ORDER BY created_at DESC
LIMIT 20;
```

### Aggregation
```sql
-- Count with filter
SELECT status, COUNT(*) as count
FROM orders
WHERE created_at > NOW() - INTERVAL '30 days'
GROUP BY status
ORDER BY count DESC;

-- Running totals
SELECT date, amount,
       SUM(amount) OVER (ORDER BY date) as running_total
FROM daily_sales;

-- Top N per group
SELECT * FROM (
    SELECT *, ROW_NUMBER() OVER (PARTITION BY category ORDER BY sales DESC) as rank
    FROM products
) ranked
WHERE rank <= 3;
```

### Upsert (Insert or Update)
```sql
-- PostgreSQL
INSERT INTO user_stats (user_id, login_count, last_login)
VALUES ($1, 1, NOW())
ON CONFLICT (user_id)
DO UPDATE SET
    login_count = user_stats.login_count + 1,
    last_login = NOW();

-- MySQL
INSERT INTO user_stats (user_id, login_count, last_login)
VALUES ($1, 1, NOW())
ON DUPLICATE KEY UPDATE
    login_count = login_count + 1,
    last_login = NOW();
```

### Common Table Expressions (CTEs)
```sql
WITH active_users AS (
    SELECT id, name, email
    FROM users
    WHERE deleted_at IS NULL
    AND last_login > NOW() - INTERVAL '30 days'
),
user_orders AS (
    SELECT user_id, COUNT(*) as order_count, SUM(amount) as total_spent
    FROM orders
    GROUP BY user_id
)
SELECT au.name, au.email, uo.order_count, uo.total_spent
FROM active_users au
JOIN user_orders uo ON au.id = uo.user_id
WHERE uo.total_spent > 100
ORDER BY uo.total_spent DESC;
```

## 4. Migrations

### Migration Rules
1. **Forward-only** — never edit a migration that ran in production
2. **Backward compatible** — old code must work with new schema
3. **Separate deploy steps** — add column → deploy code → drop column
4. **Test with production data size** — migrations that work on 100 rows may fail on 10M
5. **Always have a rollback plan** — even if it's manual

### Safe Migration Patterns
```sql
-- ✅ Safe: add column with default
ALTER TABLE users ADD COLUMN bio TEXT DEFAULT '';

-- ✅ Safe: add NOT NULL column with default (PostgreSQL 11+)
ALTER TABLE users ADD COLUMN bio TEXT NOT NULL DEFAULT '';

-- ❌ Dangerous: add NOT NULL without default (fails on existing rows)
ALTER TABLE users ADD COLUMN bio TEXT NOT NULL;

-- ❌ Dangerous: rename column (breaks all existing queries)
ALTER TABLE users RENAME COLUMN name TO full_name;
-- Instead: add new column, migrate data, drop old column in next release
```

### Multi-Step Column Rename
```sql
-- Step 1: Add new column
ALTER TABLE users ADD COLUMN full_name VARCHAR(100);

-- Step 2: Copy data
UPDATE users SET full_name = name;

-- Step 3: Add NOT NULL constraint
ALTER TABLE users ALTER COLUMN full_name SET NOT NULL;

-- Step 4: Deploy code that reads full_name

-- Step 5: Drop old column (next release)
ALTER TABLE users DROP COLUMN name;
```

## 5. Transactions

### When to Use Transactions
- Multi-table inserts/updates that must all succeed or all fail
- Financial operations (transfers, payments)
- Any operation that reads-then-writes

### Isolation Levels
```sql
-- Read Committed (default in PostgreSQL)
-- Only sees committed data, no dirty reads

-- Repeatable Read
-- Guarantees consistent reads within a transaction
BEGIN;
SET TRANSACTION ISOLATION LEVEL REPEATABLE READ;
SELECT * FROM accounts WHERE id = 1;
-- ... time passes, another transaction modifies the row ...
SELECT * FROM accounts WHERE id = 1;
-- Same result as first SELECT (no phantom reads)
COMMIT;

-- Serializable
-- Fully isolated, but can cause serialization failures
BEGIN;
SET TRANSACTION ISOLATION LEVEL SERIALIZABLE;
-- Use for critical financial operations
COMMIT;
```

### Deadlock Prevention
```sql
-- Always lock tables in the same order
-- Transaction 1: lock accounts first, then orders
BEGIN;
SELECT * FROM accounts WHERE id = 1 FOR UPDATE;
SELECT * FROM orders WHERE user_id = 1 FOR UPDATE;
COMMIT;

-- Transaction 2: same order
BEGIN;
SELECT * FROM accounts WHERE id = 2 FOR UPDATE;
SELECT * FROM orders WHERE user_id = 2 FOR UPDATE;
COMMIT;
```

## 6. Query Optimization

### EXPLAIN ANALYZE
```sql
-- Always use EXPLAIN ANALYZE before optimizing
EXPLAIN ANALYZE
SELECT u.name, COUNT(o.id) as order_count
FROM users u
JOIN orders o ON u.id = o.user_id
WHERE o.created_at > NOW() - INTERVAL '30 days'
GROUP BY u.name;

-- Look for:
-- Seq Scan → add index
-- Nested Loop with high row count → check join condition
-- Sort → add index on ORDER BY column
-- Hash Join with high memory → check join selectivity
```

### Common Performance Issues
```sql
-- ❌ N+1 query pattern
SELECT * FROM users;
-- Then for each user: SELECT * FROM orders WHERE user_id = ?

-- ✅ Use JOIN
SELECT u.*, o.*
FROM users u
JOIN orders o ON u.id = o.user_id;

-- ❌ SELECT * (fetches all columns, breaks index-only scans)
SELECT * FROM orders;

-- ✅ Select only what you need
SELECT id, status, total FROM orders;

-- ❌ LIKE '%term%' (can't use index)
SELECT * FROM users WHERE name LIKE '%john%';

-- ✅ Use full-text search or prefix search
SELECT * FROM users WHERE name LIKE 'john%';
```

## 7. Production Checklist

- [ ] Schema is normalized to at least 3NF
- [ ] Primary keys on all tables
- [ ] Foreign key constraints defined
- [ ] Indexes on frequently queried columns
- [ ] Composite indexes match query patterns
- [ ] Migrations tested with production-like data volume
- [ ] Migrations are backward compatible
- [ ] Connection pooling configured (pgbouncer, etc.)
- [ ] Backup strategy in place
- [ ] Slow query logging enabled
- [ ] No SELECT * in production code
- [ ] Transactions used for multi-step operations
