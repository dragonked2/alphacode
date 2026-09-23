---
name: backend-dev/auth
description: Authentication and authorization — JWT access/refresh token rotation, OAuth2 PKCE/client credentials, session management, RBAC/ABAC authorization, API key management, rate limiting, password hashing, MFA/TOTP, SAML/OIDC integration, CORS, CSRF protection.
---

# Authentication & Authorization

## JWT Authentication

### Token Structure

```json
// Access Token (short-lived: 15 min)
{
  "sub": "user-123",
  "email": "user@example.com",
  "roles": ["user"],
  "type": "access",
  "iat": 1700000000,
  "exp": 1700000900
}

// Refresh Token (long-lived: 7 days)
{
  "sub": "user-123",
  "type": "refresh",
  "jti": "unique-token-id",
  "iat": 1700000000,
  "exp": 1700604800
}
```

### Token Rotation

```python
# services/auth.py
from datetime import datetime, timedelta
from jose import jwt, JWTError
import secrets

class AuthService:
    def __init__(self, db, redis, config):
        self.db = db
        self.redis = redis
        self.config = config

    async def login(self, email: str, password: str) -> dict:
        user = await self.db.get_user_by_email(email)
        if not user or not verify_password(password, user.hashed_password):
            raise UnauthorizedError("Invalid credentials")

        access_token = self.create_access_token(user)
        refresh_token = await self.create_refresh_token(user)

        return {
            "access_token": access_token,
            "refresh_token": refresh_token,
            "token_type": "bearer",
            "expires_in": self.config.jwt_expires_in,
        }

    def create_access_token(self, user) -> str:
        payload = {
            "sub": str(user.id),
            "email": user.email,
            "roles": [user.role],
            "type": "access",
            "iat": datetime.utcnow(),
            "exp": datetime.utcnow() + timedelta(minutes=15),
        }
        return jwt.encode(payload, self.config.jwt_secret, algorithm="HS256")

    async def create_refresh_token(self, user) -> str:
        jti = secrets.token_urlsafe(32)
        payload = {
            "sub": str(user.id),
            "type": "refresh",
            "jti": jti,
            "iat": datetime.utcnow(),
            "exp": datetime.utcnow() + timedelta(days=7),
        }
        token = jwt.encode(payload, self.config.jwt_secret, algorithm="HS256")

        # Store in Redis for revocation
        await self.redis.setex(
            f"refresh_token:{jti}",
            timedelta(days=7),
            str(user.id),
        )

        return token

    async def refresh(self, refresh_token: str) -> dict:
        try:
            payload = jwt.decode(
                refresh_token, self.config.jwt_secret, algorithms=["HS256"]
            )
        except JWTError:
            raise UnauthorizedError("Invalid refresh token")

        if payload.get("type") != "refresh":
            raise UnauthorizedError("Invalid token type")

        jti = payload.get("jti")
        user_id = await self.redis.get(f"refresh_token:{jti}")

        if not user_id:
            raise UnauthorizedError("Refresh token revoked or expired")

        # Rotate: delete old, create new
        await self.redis.delete(f"refresh_token:{jti}")

        user = await self.db.get_user(int(user_id))
        new_access = self.create_access_token(user)
        new_refresh = await self.create_refresh_token(user)

        return {
            "access_token": new_access,
            "refresh_token": new_refresh,
            "token_type": "bearer",
        }

    async def logout(self, refresh_token: str):
        try:
            payload = jwt.decode(
                refresh_token, self.config.jwt_secret, algorithms=["HS256"]
            )
            jti = payload.get("jti")
            await self.redis.delete(f"refresh_token:{jti}")
        except JWTError:
            pass  # Token already invalid
```

### Token Revocation

```python
# Middleware to check revoked tokens
async def check_token_revoked(token: str, redis) -> bool:
    try:
        payload = jwt.decode(token, SECRET_KEY, algorithms=["HS256"])
        jti = payload.get("jti")
        if jti:
            return await redis.exists(f"revoked_token:{jti}")
        return False
    except JWTError:
        return True
```

## OAuth2

### Authorization Code with PKCE

```python
# services/oauth.py
import hashlib
import base64
import secrets
from urllib.parse import urlencode

class OAuthService:
    def __init__(self, config):
        self.config = config

    def get_authorization_url(self) -> dict:
        code_verifier = secrets.token_urlsafe(32)
        code_challenge = base64.urlsafe_b64encode(
            hashlib.sha256(code_verifier.encode()).digest()
        ).rstrip(b"=").decode()

        params = {
            "client_id": self.config.client_id,
            "redirect_uri": self.config.redirect_uri,
            "response_type": "code",
            "scope": "openid profile email",
            "state": secrets.token_urlsafe(16),
            "code_challenge": code_challenge,
            "code_challenge_method": "S256",
        }

        return {
            "url": f"{self.config.auth_url}?{urlencode(params)}",
            "state": params["state"],
            "code_verifier": code_verifier,
        }

    async def exchange_code(self, code: str, code_verifier: str) -> dict:
        data = {
            "grant_type": "authorization_code",
            "code": code,
            "redirect_uri": self.config.redirect_uri,
            "client_id": self.config.client_id,
            "client_secret": self.config.client_secret,
            "code_verifier": code_verifier,
        }

        response = httpx.post(self.config.token_url, data=data)
        return response.json()
```

### Client Credentials Flow

```python
async def get_client_token(client_id: str, client_secret: str) -> str:
    data = {
        "grant_type": "client_credentials",
        "client_id": client_id,
        "client_secret": client_secret,
        "scope": "api:read api:write",
    }

    response = httpx.post(token_url, data=data)
    token_data = response.json()
    return token_data["access_token"]
```

### OAuth2 Callback Handler

```python
# handlers/auth.py
from fastapi import APIRouter, Request
from fastapi.responses import RedirectResponse

router = APIRouter()

@router.get("/auth/callback")
async def oauth_callback(request: Request, code: str, state: str):
    # Validate state
    stored_state = await request.session.get("oauth_state")
    if state != stored_state:
        raise HTTPException(400, "Invalid state parameter")

    # Exchange code for tokens
    token_data = await oauth_service.exchange_code(
        code,
        request.session.get("code_verifier"),
    )

    # Get or create user from ID token
    user_info = decode_id_token(token_data["id_token"])
    user = await get_or_create_user(user_info)

    # Create our own tokens
    access_token = auth_service.create_access_token(user)
    refresh_token = await auth_service.create_refresh_token(user)

    return RedirectResponse(
        url=f"{FRONTEND_URL}/auth/callback?token={access_token}",
        status_code=302,
    )
```

## Session Management

### Server-Side Sessions

```python
# middleware/session.py
from starlette.middleware.base import BaseHTTPMiddleware
import uuid

class SessionMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        session_id = request.cookies.get("session_id")

        if session_id:
            session_data = await redis.get(f"session:{session_id}")
            if session_data:
                request.state.session = json.loads(session_data)
            else:
                session_id = None

        if not session_id:
            session_id = str(uuid.uuid4())
            request.state.session = {}

        response = await call_next(request)

        # Set session cookie
        response.set_cookie(
            "session_id",
            session_id,
            httponly=True,
            secure=True,
            samesite="lax",
            max_age=86400 * 7,  # 7 days
        )

        # Store session
        await redis.setex(
            f"session:{session_id}",
            86400 * 7,
            json.dumps(request.state.session),
        )

        return response
```

### Cookie-Based Sessions

```python
from itsdangerous import URLSafeTimedSerializer

class CookieSession:
    def __init__(self, secret_key: str):
        self.serializer = URLSafeTimedSerializer(secret_key)

    def create_session(self, user_id: int) -> str:
        data = {"user_id": user_id, "created_at": time.time()}
        return self.serializer.dumps(data)

    def verify_session(self, token: str, max_age: int = 604800) -> dict:
        try:
            return self.serializer.loads(token, max_age=max_age)
        except Exception:
            return None
```

## RBAC Authorization

```python
# models/permissions.py
from enum import Enum

class Permission(str, Enum):
    USER_READ = "user:read"
    USER_WRITE = "user:write"
    USER_DELETE = "user:delete"
    ORDER_READ = "order:read"
    ORDER_WRITE = "order:write"
    REPORT_VIEW = "report:view"
    ADMIN_ALL = "admin:*"

ROLE_PERMISSIONS = {
    "viewer": [Permission.USER_READ, Permission.ORDER_READ],
    "user": [
        Permission.USER_READ,
        Permission.USER_WRITE,
        Permission.ORDER_READ,
        Permission.ORDER_WRITE,
    ],
    "admin": [
        Permission.USER_READ,
        Permission.USER_WRITE,
        Permission.USER_DELETE,
        Permission.ORDER_READ,
        Permission.ORDER_WRITE,
        Permission.REPORT_VIEW,
    ],
    "superadmin": [Permission.ADMIN_ALL],
}

def has_permission(role: str, required_permission: Permission) -> bool:
    permissions = ROLE_PERMISSIONS.get(role, [])
    if Permission.ADMIN_ALL in permissions:
        return True
    return required_permission in permissions
```

```python
# middleware/authorization.py
from functools import wraps

def require_permission(permission: Permission):
    def decorator(func):
        @wraps(func)
        async def wrapper(*args, **kwargs):
            request = kwargs.get("request") or args[0]
            user = request.state.user

            if not has_permission(user.role, permission):
                raise HTTPException(
                    status_code=403,
                    detail=f"Permission denied: {permission.value}"
                )
            return await func(*args, **kwargs)
        return wrapper
    return decorator

# Usage
@router.get("/users")
@require_permission(Permission.USER_READ)
async def list_users(request: Request):
    ...
```

## ABAC Authorization

```python
# models/abac.py
from dataclasses import dataclass
from typing import Any

@dataclass
class PolicyContext:
    subject: dict  # User making the request
    resource: dict  # Resource being accessed
    action: str     # Action being performed
    environment: dict  # Time, IP, etc.

class PolicyEngine:
    def __init__(self):
        self.policies = []

    def add_policy(self, condition, effect="allow"):
        self.policies.append({"condition": condition, "effect": effect})

    def evaluate(self, context: PolicyContext) -> bool:
        for policy in self.policies:
            if policy["condition"](context):
                return policy["effect"] == "allow"
        return False  # Default deny

# Define policies
engine = PolicyEngine()

# Users can only access their own resources
engine.add_policy(
    lambda ctx: ctx.resource.get("owner_id") == ctx.subject.get("id"),
    effect="allow"
)

# Admins can access everything
engine.add_policy(
    lambda ctx: ctx.subject.get("role") == "admin",
    effect="allow"
)

# No access during maintenance window
engine.add_policy(
    lambda ctx: not ctx.environment.get("maintenance_mode", False),
    effect="allow"
)
```

## API Key Management

```python
# models/api_key.py
import secrets
import hashlib
from datetime import datetime

class APIKey:
    def __init__(self, db):
        self.db = db

    async def create(self, user_id: int, name: str, scopes: list[str]) -> dict:
        key = f"ak_{secrets.token_urlsafe(32)}"
        key_hash = hashlib.sha256(key.encode()).hexdigest()

        await self.db.execute(
            """INSERT INTO api_keys (user_id, name, key_hash, scopes, created_at)
               VALUES ($1, $2, $3, $4, NOW())""",
            user_id, name, key_hash, scopes,
        )

        return {"key": key, "name": name, "scopes": scopes}

    async def validate(self, key: str) -> dict | None:
        key_hash = hashlib.sha256(key.encode()).hexdigest()
        return await self.db.fetchrow(
            """SELECT * FROM api_keys
               WHERE key_hash = $1 AND revoked_at IS NULL""",
            key_hash,
        )
```

```python
# middleware/api_key.py
async def authenticate_api_key(request: Request):
    api_key = request.headers.get("X-API-Key")
    if api_key:
        key_data = await api_key_service.validate(api_key)
        if not key_data:
            raise HTTPException(401, "Invalid API key")
        request.state.user = await get_user(key_data["user_id"])
        request.state.scopes = key_data["scopes"]
```

## Rate Limiting

### Sliding Window

```python
# services/rate_limit.py
import time

class SlidingWindowRateLimiter:
    def __init__(self, redis):
        self.redis = redis

    async def is_allowed(
        self, key: str, limit: int, window_seconds: int
    ) -> bool:
        now = time.time()
        window_start = now - window_seconds

        pipe = self.redis.pipeline()
        pipe.zremrangebyscore(key, 0, window_start)  # Remove old entries
        pipe.zadd(key, {str(now): now})  # Add current request
        pipe.zcard(key)  # Count requests
        pipe.expire(key, window_seconds)  # Set TTL
        results = await pipe.execute()

        request_count = results[2]
        return request_count <= limit
```

### Token Bucket

```python
class TokenBucketRateLimiter:
    def __init__(self, redis):
        self.redis = redis

    async def is_allowed(
        self, key: str, capacity: int, refill_rate: float
    ) -> bool:
        now = time.time()
        script = """
        local key = KEYS[1]
        local capacity = tonumber(ARGV[1])
        local refill_rate = tonumber(ARGV[2])
        local now = tonumber(ARGV[3])

        local bucket = redis.call('hmget', key, 'tokens', 'last_refill')
        local tokens = tonumber(bucket[1]) or capacity
        local last_refill = tonumber(bucket[2]) or now

        -- Refill tokens
        local elapsed = now - last_refill
        local new_tokens = math.min(capacity, tokens + (elapsed * refill_rate))

        if new_tokens >= 1 then
            redis.call('hmset', key, 'tokens', new_tokens - 1, 'last_refill', now)
            redis.call('expire', key, math.ceil(capacity / refill_rate))
            return 1
        else
            return 0
        end
        """
        result = await self.redis.eval(script, 1, key, capacity, refill_rate, now)
        return bool(result)
```

## Password Hashing

```python
# services/password.py
import bcrypt
import argon2
from argon2 import PasswordHasher
from argon2.exceptions import VerifyMismatchError

class PasswordService:
    def __init__(self):
        self.argon2 = PasswordHasher(
            time_cost=3,
            memory_cost=65536,  # 64MB
            parallelism=4,
            hash_len=32,
            salt_len=16,
        )

    def hash_password(self, password: str) -> str:
        return self.argon2.hash(password)

    def verify_password(self, password: str, hashed: str) -> bool:
        try:
            self.argon2.verify(hashed, password)
            return True
        except VerifyMismatchError:
            return False

    def check_needs_rehash(self, hashed: str) -> bool:
        return self.argon2.check_needs_rehash(hashed)
```

## MFA/TOTP

```python
# services/mfa.py
import pyotp
import qrcode
import io
import base64

class MFAService:
    def __init__(self, db):
        self.db = db

    async def enable_mfa(self, user_id: int) -> dict:
        secret = pyotp.random_base32()
        totp = pyotp.TOTP(secret)

        # Store secret (not enabled yet)
        await self.db.execute(
            "UPDATE users SET mfa_secret = $1, mfa_enabled = false WHERE id = $2",
            secret, user_id,
        )

        # Generate QR code
        provisioning_uri = totp.provisioning_uri(
            name=str(user_id),
            issuer_name="MyApp",
        )

        qr = qrcode.make(provisioning_uri)
        buffer = io.BytesIO()
        qr.save(buffer, format="PNG")
        qr_base64 = base64.b64encode(buffer.getvalue()).decode()

        return {
            "secret": secret,
            "qr_code": f"data:image/png;base64,{qr_base64}",
            "provisioning_uri": provisioning_uri,
        }

    async def verify_and_enable(self, user_id: int, code: str) -> bool:
        user = await self.db.get_user(user_id)
        totp = pyotp.TOTP(user.mfa_secret)

        if totp.verify(code, valid_window=1):
            await self.db.execute(
                "UPDATE users SET mfa_enabled = true WHERE id = $1", user_id
            )
            return True
        return False

    async def verify_mfa(self, user_id: int, code: str) -> bool:
        user = await self.db.get_user(user_id)
        if not user.mfa_enabled:
            return True
        totp = pyotp.TOTP(user.mfa_secret)
        return totp.verify(code, valid_window=1)

    async def generate_backup_codes(self, user_id: int) -> list[str]:
        codes = [secrets.token_urlsafe(8) for _ in range(10)]
        hashed_codes = [bcrypt.hashpw(c.encode(), bcrypt.gensalt()) for c in codes]

        await self.db.execute(
            "UPDATE users SET backup_codes = $1 WHERE id = $2",
            hashed_codes, user_id,
        )
        return codes
```

## CORS Configuration

```python
# config/cors.py
from fastapi.middleware.cors import CORSMiddleware

CORS_CONFIG = {
    "allow_origins": [
        "https://app.example.com",
        "https://staging.example.com",
    ],
    "allow_credentials": True,
    "allow_methods": ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"],
    "allow_headers": [
        "Content-Type",
        "Authorization",
        "X-Request-ID",
        "X-API-Key",
    ],
    "expose_headers": ["X-Request-ID", "X-RateLimit-Limit", "X-RateLimit-Remaining"],
    "max_age": 600,
}

app.add_middleware(CORSMiddleware, **CORS_CONFIG)
```

## CSRF Protection

```python
# middleware/csrf.py
import secrets
from starlette.middleware.base import BaseHTTPMiddleware

class CSRFMiddleware(BaseHTTPMiddleware):
    def __init__(self, app, secret_key: str):
        super().__init__(app)
        self.secret_key = secret_key

    async def dispatch(self, request: Request, call_next):
        if request.method in ("POST", "PUT", "PATCH", "DELETE"):
            token = request.headers.get("X-CSRF-Token")
            cookie_token = request.cookies.get("csrf_token")

            if not token or not cookie_token:
                raise HTTPException(403, "CSRF token missing")

            if token != cookie_token:
                raise HTTPException(403, "CSRF token mismatch")

        response = await call_next(request)

        # Set CSRF cookie on safe methods
        if request.method == "GET":
            csrf_token = secrets.token_urlsafe(32)
            response.set_cookie(
                "csrf_token",
                csrf_token,
                httponly=False,  # JavaScript needs access
                secure=True,
                samesite="strict",
            )

        return response
```

## Checklist

- [ ] JWT access + refresh tokens implemented
- [ ] Token rotation on refresh
- [ ] Token revocation via Redis
- [ ] OAuth2 PKCE flow (if applicable)
- [ ] Password hashing with Argon2
- [ ] MFA/TOTP support
- [ ] RBAC/ABAC authorization
- [ ] API key management
- [ ] Rate limiting (sliding window or token bucket)
- [ ] CORS configured
- [ ] CSRF protection
- [ ] Session management (if using sessions)
