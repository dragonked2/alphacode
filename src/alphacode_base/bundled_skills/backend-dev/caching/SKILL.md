---
name: backend-dev/caching
description: Caching strategies — HTTP caching, CDN, application-level caching (Redis, Memcached), cache invalidation patterns, stampede prevention, database query caching, session caching, rate limit counters, write-through/write-behind, cache warming.
---

# Caching Strategies

## HTTP Caching

### Cache-Control Headers

```python
# middleware/cache.py
from starlette.middleware.base import BaseHTTPMiddleware

class CacheControlMiddleware(BaseHTTPMiddleware):
    CACHE_RULES = {
        "/api/v1/health": "no-cache",
        "/api/v1/users/me": "private, max-age=300",
        "/api/v1/products": "public, max-age=60, stale-while-revalidate=300",
        "/api/v1/static": "public, max-age=31536000, immutable",
    }

    async def dispatch(self, request: Request, call_next):
        response = await call_next(request)

        # Find matching cache rule
        for pattern, directive in self.CACHE_RULES.items():
            if request.url.path.startswith(pattern):
                response.headers["Cache-Control"] = directive
                break

        return response
```

### ETags

```python
import hashlib

def generate_etag(content: bytes) -> str:
    return f'"{hashlib.md5(content).hexdigest()}"'

async def cache_aware_endpoint(request: Request):
    data = await get_expensive_data()
    content = json.dumps(data).encode()
    etag = generate_etag(content)

    # Check If-None-Match
    if request.headers.get("If-None-Match") == etag:
        return Response(status_code=304)

    response = Response(content=content, media_type="application/json")
    response.headers["ETag"] = etag
    response.headers["Cache-Control"] = "private, max-age=0, must-revalidate"
    return response
```

### Conditional Requests

```python
from datetime import datetime

async def conditional_get(request: Request, resource_id: str):
    resource = await db.get_resource(resource_id)
    last_modified = resource.updated_at.strftime("%a, %d %b %Y %H:%M:%S GMT")

    # Check If-Modified-Since
    if_modified_since = request.headers.get("If-Modified-Since")
    if if_modified_since:
        client_date = datetime.strptime(if_modified_since, "%a, %d %b %Y %H:%M:%S GMT")
        if resource.updated_at <= client_date:
            return Response(status_code=304)

    response = Response(
        content=json.dumps(resource.to_dict()),
        media_type="application/json",
    )
    response.headers["Last-Modified"] = last_modified
    return response
```

## Application-Level Caching

### Redis Patterns

```python
# services/cache.py
import json
from typing import Any, Optional
import redis.asyncio as redis

class CacheService:
    def __init__(self, redis_client: redis.Redis):
        self.redis = redis_client
        self.default_ttl = 3600  # 1 hour

    async def get(self, key: str) -> Optional[Any]:
        data = await self.redis.get(key)
        if data:
            return json.loads(data)
        return None

    async def set(
        self, key: str, value: Any, ttl: int = None
    ):
        ttl = ttl or self.default_ttl
        await self.redis.setex(key, ttl, json.dumps(value))

    async def delete(self, key: str):
        await self.redis.delete(key)

    async def get_or_set(
        self, key: str, factory, ttl: int = None
    ) -> Any:
        cached = await self.get(key)
        if cached is not None:
            return cached

        value = await factory()
        await self.set(key, value, ttl)
        return value

    async def invalidate_pattern(self, pattern: str):
        keys = []
        async for key in self.redis.scan_iter(match=pattern):
            keys.append(key)
        if keys:
            await self.redis.delete(*keys)

    async def increment(self, key: str, amount: int = 1) -> int:
        return await self.redis.incrby(key, amount)

    async def decrement(self, key: str, amount: int = 1) -> int:
        return await self.redis.decrby(key, amount)


# Usage
cache = CacheService(redis)

async def get_user(user_id: str):
    return await cache.get_or_set(
        f"user:{user_id}",
        lambda: db.fetch_user(user_id),
        ttl=1800,
    )

async def update_user(user_id: str, data: dict):
    user = await db.update_user(user_id, data)
    await cache.set(f"user:{user_id}", user)
    await cache.invalidate_pattern(f"user_list:*")
    return user
```

### Memcached

```python
# services/memcached.py
import aiomcache

class MemcachedService:
    def __init__(self, host: str, port: int):
        self.client = aiomcache.Client(host, port)

    async def get(self, key: str) -> Optional[bytes]:
        return await self.client.get(key.encode())

    async def set(self, key: str, value: bytes, ttl: int = 3600):
        await self.client.set(key.encode(), value, exptime=ttl)

    async def delete(self, key: str):
        await self.client.delete(key.encode())
```

## Cache Invalidation Patterns

### Time-Based (TTL)

```python
# Simple TTL-based caching
async def get_product(product_id: str):
    cache_key = f"product:{product_id}"
    cached = await redis.get(cache_key)
    if cached:
        return json.loads(cached)

    product = await db.get_product(product_id)

    # Different TTLs for different data types
    ttl = {
        "product": 300,      # 5 min - product details change rarely
        "inventory": 30,     # 30 sec - inventory changes frequently
        "price": 60,         # 1 min - prices change occasionally
    }.get("product", 300)

    await redis.setex(cache_key, ttl, json.dumps(product))
    return product
```

### Event-Based (Pub/Sub)

```python
# Invalidator
async def on_product_updated(product_id: str):
    # Publish invalidation event
    await redis.publish("cache:invalidate", json.dumps({
        "type": "product",
        "id": product_id,
    }))

# Subscriber
async def cache_invalidation_listener():
    pubsub = redis.pubsub()
    await pubsub.subscribe("cache:invalidate")

    async for message in pubsub.listen():
        if message["type"] == "message":
            event = json.loads(message["data"])
            if event["type"] == "product":
                await redis.delete(f"product:{event['id']}")
                await redis.delete("product_list:*")
```

### Version-Based

```python
# Generate versioned cache key
def get_cache_key(resource: str, version: int = None) -> str:
    if version:
        return f"{resource}:v{version}"
    return resource

# Increment version to invalidate all caches
async def bump_cache_version(resource: str):
    key = f"{resource}:version"
    new_version = await redis.incr(key)
    return new_version

# Usage
async def get_products():
    version = await redis.get("product:version") or 1
    cache_key = f"products:v{version}"

    cached = await redis.get(cache_key)
    if cached:
        return json.loads(cached)

    products = await db.get_all_products()
    await redis.setex(cache_key, 3600, json.dumps(products))
    return products
```

### Write-Through vs Write-Behind vs Write-Around

```python
# Write-Through: Write to cache and DB simultaneously
async def update_product_write_through(product_id: str, data: dict):
    await db.update_product(product_id, data)
    product = await db.get_product(product_id)
    await cache.set(f"product:{product_id}", product)

# Write-Behind (Write-Back): Write to cache first, DB asynchronously
async def update_product_write_behind(product_id: str, data: dict):
    await cache.set(f"product:{product_id}", data)
    # Queue DB write
    await task_queue.enqueue(write_to_db, product_id, data)

# Write-Around: Write to DB only, cache on next read
async def update_product_write_around(product_id: str, data: dict):
    await db.update_product(product_id, data)
    await cache.delete(f"product:{product_id}")  # Invalidate only
```

## Cache Stampede Prevention

### Mutex (Locking)

```python
async def get_with_mutex(key: str, factory, ttl: int = 300):
    # Try cache first
    cached = await cache.get(key)
    if cached:
        return cached

    # Try to acquire lock
    lock_key = f"lock:{key}"
    acquired = await redis.set(lock_key, "1", nx=True, ex=10)

    if acquired:
        try:
            # Winner: fetch and cache
            value = await factory()
            await cache.set(key, value, ttl)
            return value
        finally:
            await redis.delete(lock_key)
    else:
        # Loser: wait and retry
        for _ in range(10):
            await asyncio.sleep(0.1)
            cached = await cache.get(key)
            if cached:
                return cached
        # Fallback: fetch directly
        return await factory()
```

### Probabilistic Early Expiration

```python
import random
import time

async def get_with_early_expiration(
    key: str, factory, ttl: int = 300, beta: float = 1.0
):
    cached = await cache.get_with_metadata(key)
    if not cached:
        value = await factory()
        await cache.set_with_metadata(key, value, ttl)
        return value

    value, created_at = cached
    delta = ttl * (beta * _log_random())

    if time.time() - created_at > ttl - delta:
        # Probabilistic refresh
        asyncio.create_task(_refresh_cache(key, factory, ttl))

    return value

def _log_random():
    return -math.log(random.random())

async def _refresh_cache(key, factory, ttl):
    lock_key = f"refresh:{key}"
    acquired = await redis.set(lock_key, "1", nx=True, ex=5)
    if acquired:
        try:
            value = await factory()
            await cache.set_with_metadata(key, value, ttl)
        finally:
            await redis.delete(lock_key)
```

## Database Query Caching

```python
# Materialized view caching
async def get_dashboard_stats():
    cache_key = "dashboard:stats"
    cached = await cache.get(cache_key)
    if cached:
        return cached

    stats = await db.fetchrow("""
        SELECT
            COUNT(*) as total_users,
            COUNT(*) FILTER (WHERE created_at > NOW() - INTERVAL '1 day') as new_users,
            AVG(amount) as avg_order_value
        FROM users u
        LEFT JOIN orders o ON o.user_id = u.id
    """)

    await cache.set(cache_key, dict(stats), ttl=300)
    return stats

# Query result caching with SQL hash
def get_query_cache_key(query: str, params: tuple) -> str:
    query_hash = hashlib.md5(query.encode()).hexdigest()
    params_hash = hashlib.md5(str(params).encode()).hexdigest()
    return f"query:{query_hash}:{params_hash}"
```

## Session Caching

```python
class SessionCache:
    def __init__(self, redis: redis.Redis):
        self.redis = redis
        self.ttl = 86400 * 7  # 7 days

    async def get_session(self, session_id: str) -> Optional[dict]:
        data = await self.redis.get(f"session:{session_id}")
        if data:
            # Extend TTL on access (sliding expiration)
            await self.redis.expire(f"session:{session_id}", self.ttl)
            return json.loads(data)
        return None

    async def create_session(self, user_id: int) -> str:
        session_id = secrets.token_urlsafe(32)
        data = {
            "user_id": user_id,
            "created_at": time.time(),
            "ip": None,
            "user_agent": None,
        }
        await self.redis.setex(
            f"session:{session_id}", self.ttl, json.dumps(data)
        )
        return session_id

    async def destroy_session(self, session_id: str):
        await self.redis.delete(f"session:{session_id}")

    async def destroy_all_user_sessions(self, user_id: int):
        cursor = 0
        while True:
            cursor, keys = await self.redis.scan(
                cursor, match="session:*", count=100
            )
            for key in keys:
                data = await self.redis.get(key)
                if data and json.loads(data).get("user_id") == user_id:
                    await self.redis.delete(key)
            if cursor == 0:
                break
```

## Rate Limit Counter Caching

```python
class RateLimitCounter:
    def __init__(self, redis: redis.Redis):
        self.redis = redis

    async def check_rate_limit(
        self,
        key: str,
        limit: int,
        window_seconds: int,
    ) -> dict:
        now = time.time()
        window_start = now - window_seconds

        pipe = self.redis.pipeline()

        # Remove old entries
        pipe.zremrangebyscore(key, 0, window_start)

        # Add current request
        pipe.zadd(key, {f"{now}": now})

        # Count requests in window
        pipe.zcard(key)

        # Set expiry
        pipe.expire(key, window_seconds)

        results = await pipe.execute()
        current_count = results[2]

        return {
            "allowed": current_count <= limit,
            "limit": limit,
            "remaining": max(0, limit - current_count),
            "reset_at": int(now + window_seconds),
        }
```

## Cache Warming

```python
class CacheWarmer:
    def __init__(self, cache: CacheService, db):
        self.cache = cache
        self.db = db

    async def warm_all(self):
        """Warm caches on application startup."""
        await asyncio.gather(
            self.warm_products(),
            self.warm_config(),
            self.warm_user_permissions(),
        )

    async def warm_products(self):
        """Pre-populate product cache."""
        products = await self.db.get_all_products()
        for product in products:
            await self.cache.set(
                f"product:{product.id}",
                product,
                ttl=300,
            )
        await self.cache.set("product_list:all", products, ttl=300)

    async def warm_config(self):
        """Cache application configuration."""
        config = await self.db.get_config()
        await self.cache.set("app:config", config, ttl=3600)

    async def warm_user_permissions(self):
        """Cache permissions for active users."""
        users = await self.db.get_active_users()
        for user in users:
            permissions = await self.db.get_user_permissions(user.id)
            await self.cache.set(
                f"user:{user.id}:permissions",
                permissions,
                ttl=600,
            )

# Background warming task
@celery_app.task
def periodic_cache_warm():
    asyncio.run(cache_warmer.warm_all())
```

## Multi-Level Caching

```python
class MultiLevelCache:
    def __init__(self, l1: dict, l2: CacheService):
        self.l1 = l1  # In-memory (dict or local cache)
        self.l2 = l2  # Redis

    async def get(self, key: str):
        # L1: In-memory
        if key in self.l1:
            return self.l1[key]

        # L2: Redis
        value = await self.l2.get(key)
        if value:
            self.l1[key] = value  # Populate L1
            return value

        return None

    async def set(self, key: str, value, ttl: int = 300):
        self.l1[key] = value
        await self.l2.set(key, value, ttl)

    async def delete(self, key: str):
        self.l1.pop(key, None)
        await self.l2.delete(key)
```

## Checklist

- [ ] HTTP caching headers configured
- [ ] ETags for conditional requests
- [ ] Redis caching with proper TTLs
- [ ] Cache invalidation patterns implemented
- [ ] Stampede prevention (mutex or probabilistic)
- [ ] Write strategy chosen (through/behind/around)
- [ ] Session caching with sliding expiration
- [ ] Rate limit counters using Redis
- [ ] Cache warming on startup
- [ ] Multi-level caching (if needed)
- [ ] Cache monitoring (hit rate, miss rate)
