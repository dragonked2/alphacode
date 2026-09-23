---
name: backend-dev/python
description: Python/FastAPI/Django backend development — async patterns, Pydantic models, dependency injection, middleware, background tasks, ORM patterns, migration management, rate limiting, WebSocket, OpenAPI documentation, test patterns.
---

# Python Backend Development

## FastAPI Project Structure

```
app/
├── core/
│   ├── config.py         # Settings with pydantic-settings
│   ├── security.py       # Auth utilities
│   ├── deps.py           # Dependency injection
│   ├── exceptions.py     # Custom exceptions
│   └── middleware.py      # Middleware setup
├── models/
│   ├── domain.py         # Domain models (Pydantic)
│   └── database.py       # SQLAlchemy/Django models
├── api/
│   └── v1/
│       ├── router.py     # API router
│       ├── endpoints/
│       │   ├── auth.py
│       │   ├── users.py
│       │   └── health.py
│       └── dependencies.py
├── services/
│   ├── user.py           # Business logic
│   └── email.py
├── repositories/
│   └── user.py           # Data access layer
├── tasks/
│   ├── celery_app.py     # Celery configuration
│   └── email.py          # Background tasks
├── db/
│   ├── session.py        # Database session
│   ├── base.py           # Base model
│   └── migrations/       # Alembic
├── tests/
│   ├── conftest.py       # Test fixtures
│   ├── test_api/
│   └── test_services/
├── main.py               # Application entry
└── pyproject.toml
```

## FastAPI Application Setup

```python
# main.py
from contextlib import asynccontextmanager
from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.middleware.gzip import GZipMiddleware
from prometheus_fastapi_instrumentator import Instrumentator

from app.core.config import settings
from app.core.exceptions import register_exception_handlers
from app.api.v1.router import api_router
from app.db.session import engine, close_engine
from app.core.middleware import RequestIDMiddleware, LoggingMiddleware


@asynccontextmanager
async def lifespan(app: FastAPI):
    # Startup
    yield
    # Shutdown
    await close_engine()


app = FastAPI(
    title=settings.PROJECT_NAME,
    version=settings.VERSION,
    docs_url="/docs" if settings.DEBUG else None,
    redoc_url="/redoc" if settings.DEBUG else None,
    lifespan=lifespan,
)

# Middleware (order matters - first added = outermost)
app.add_middleware(GZipMiddleware, minimum_size=1000)
app.add_middleware(
    CORSMiddleware,
    allow_origins=settings.CORS_ORIGINS,
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)
app.add_middleware(LoggingMiddleware)
app.add_middleware(RequestIDMiddleware)

# Exception handlers
register_exception_handlers(app)

# Routes
app.include_router(api_router, prefix="/api/v1")

# Metrics
Instrumentator().instrument(app).expose(app)


@app.get("/health")
async def health_check():
    return {"status": "ok"}
```

## Pydantic Models (Validation & Serialization)

```python
# models/domain.py
from datetime import datetime
from enum import Enum
from typing import Optional
from pydantic import BaseModel, Field, EmailStr, field_validator


class UserRole(str, Enum):
    ADMIN = "admin"
    USER = "user"
    VIEWER = "viewer"


class UserBase(BaseModel):
    email: EmailStr
    name: str = Field(..., min_length=2, max_length=100)
    role: UserRole = UserRole.USER


class UserCreate(UserBase):
    password: str = Field(
        ...,
        min_length=8,
        pattern=r"^(?=.*[a-z])(?=.*[A-Z])(?=.*\d).+$",
        description="Must contain uppercase, lowercase, and digit"
    )

    @field_validator("name")
    @classmethod
    def name_must_be_title_case(cls, v: str) -> str:
        return v.strip().title()


class UserUpdate(BaseModel):
    name: Optional[str] = Field(None, min_length=2, max_length=100)
    email: Optional[EmailStr] = None


class UserResponse(UserBase):
    id: int
    created_at: datetime
    updated_at: datetime

    model_config = {"from_attributes": True}


class UserListResponse(BaseModel):
    items: list[UserResponse]
    total: int
    page: int
    limit: int
    pages: int


class PaginationParams(BaseModel):
    page: int = Field(1, ge=1)
    limit: int = Field(20, ge=1, le=100)
    sort: str = Field("created_at", pattern="^(created_at|name|email)$")
    order: str = Field("desc", pattern="^(asc|desc)$")
```

## Dependency Injection

```python
# core/deps.py
from typing import AsyncGenerator
from fastapi import Depends, HTTPException, status
from fastapi.security import HTTPBearer, HTTPAuthorizationCredentials
from jose import jwt
from sqlalchemy.ext.asyncio import AsyncSession
import redis.asyncio as redis

from app.core.config import settings
from app.db.session import get_db
from app.models.database import User
from app.repositories.user import UserRepository

security = HTTPBearer()


async def get_current_user(
    credentials: HTTPAuthorizationCredentials = Depends(security),
    db: AsyncSession = Depends(get_db),
) -> User:
    token = credentials.credentials
    try:
        payload = jwt.decode(token, settings.JWT_SECRET, algorithms=["HS256"])
        user_id: int = payload.get("sub")
        if user_id is None:
            raise HTTPException(status_code=401, detail="Invalid token")
    except jwt.ExpiredSignatureError:
        raise HTTPException(status_code=401, detail="Token expired")
    except jwt.JWTError:
        raise HTTPException(status_code=401, detail="Invalid token")

    repo = UserRepository(db)
    user = await repo.get_by_id(user_id)
    if user is None:
        raise HTTPException(status_code=401, detail="User not found")
    return user


async def require_admin(user: User = Depends(get_current_user)) -> User:
    if user.role != "admin":
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Admin access required"
        )
    return user


def get_redis() -> redis.Redis:
    return redis.from_url(settings.REDIS_URL, decode_responses=True)


# Usage in endpoints
from fastapi import APIRouter, Depends

router = APIRouter()

@router.get("/users/me", response_model=UserResponse)
async def get_my_profile(user: User = Depends(get_current_user)):
    return user

@router.get("/admin/users", response_model=UserListResponse)
async def list_users_admin(
    admin: User = Depends(require_admin),
    db: AsyncSession = Depends(get_db),
):
    ...
```

## Middleware

```python
# core/middleware.py
import time
import uuid
from fastapi import Request, Response
from starlette.middleware.base import BaseHTTPMiddleware

from app.core.logging import logger


class RequestIDMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        request_id = request.headers.get("X-Request-ID", str(uuid.uuid4()))
        request.state.request_id = request_id

        response = await call_next(request)
        response.headers["X-Request-ID"] = request_id
        return response


class LoggingMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        start_time = time.time()

        response = await call_next(request)

        duration = time.time() - start_time
        logger.info(
            "request",
            method=request.method,
            path=request.url.path,
            status_code=response.status_code,
            duration_ms=round(duration * 1000, 2),
            request_id=getattr(request.state, "request_id", None),
        )

        return response
```

## Exception Handling

```python
# core/exceptions.py
from fastapi import Request, HTTPException
from fastapi.responses import JSONResponse


class AppException(Exception):
    def __init__(self, status_code: int, code: str, message: str, details: dict = None):
        self.status_code = status_code
        self.code = code
        self.message = message
        self.details = details


class NotFoundError(AppException):
    def __init__(self, resource: str, id: str = None):
        super().__init__(
            status_code=404,
            code="NOT_FOUND",
            message=f"{resource} not found" + (f": {id}" if id else ""),
        )


class ValidationError(AppException):
    def __init__(self, message: str, details: dict = None):
        super().__init__(400, "VALIDATION_ERROR", message, details)


class ConflictError(AppException):
    def __init__(self, message: str):
        super().__init__(409, "CONFLICT", message)


def register_exception_handlers(app):
    @app.exception_handler(AppException)
    async def app_exception_handler(request: Request, exc: AppException):
        return JSONResponse(
            status_code=exc.status_code,
            content={
                "error": {
                    "code": exc.code,
                    "message": exc.message,
                    **(exc.details and {"details": exc.details}),
                }
            },
        )

    @app.exception_handler(HTTPException)
    async def http_exception_handler(request: Request, exc: HTTPException):
        return JSONResponse(
            status_code=exc.status_code,
            content={
                "error": {
                    "code": "HTTP_ERROR",
                    "message": exc.detail,
                }
            },
        )
```

## SQLAlchemy Async ORM Patterns

```python
# db/session.py
from sqlalchemy.ext.asyncio import create_async_engine, async_sessionmaker, AsyncSession
from sqlalchemy.orm import DeclarativeBase

from app.core.config import settings

engine = create_async_engine(
    settings.DATABASE_URL,
    echo=settings.DEBUG,
    pool_size=20,
    max_overflow=10,
    pool_pre_ping=True,
)

AsyncSessionLocal = async_sessionmaker(
    engine, class_=AsyncSession, expire_on_commit=False
)


class Base(DeclarativeBase):
    pass


async def get_db() -> AsyncGenerator[AsyncSession, None]:
    async with AsyncSessionLocal() as session:
        try:
            yield session
            await session.commit()
        except Exception:
            await session.rollback()
            raise
        finally:
            await session.close()


async def close_engine():
    await engine.dispose()
```

```python
# models/database.py
from datetime import datetime
from sqlalchemy import String, DateTime, func, Integer
from sqlalchemy.orm import Mapped, mapped_column, relationship

from app.db.session import Base


class TimestampMixin:
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now()
    )
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), onupdate=func.now()
    )


class User(TimestampMixin, Base):
    __tablename__ = "users"

    id: Mapped[int] = mapped_column(Integer, primary_key=True, index=True)
    email: Mapped[str] = mapped_column(String(255), unique=True, index=True)
    name: Mapped[str] = mapped_column(String(100))
    role: Mapped[str] = mapped_column(String(20), default="user")
    hashed_password: Mapped[str] = mapped_column(String(255))

    # Relationships
    posts: Mapped[list["Post"]] = relationship(back_populates="author")
```

```python
# repositories/user.py
from typing import Optional
from sqlalchemy import select, func
from sqlalchemy.ext.asyncio import AsyncSession

from app.models.database import User
from app.models.domain import UserCreate


class UserRepository:
    def __init__(self, db: AsyncSession):
        self.db = db

    async def get_by_id(self, user_id: int) -> Optional[User]:
        result = await self.db.execute(select(User).where(User.id == user_id))
        return result.scalar_one_or_none()

    async def get_by_email(self, email: str) -> Optional[User]:
        result = await self.db.execute(select(User).where(User.email == email))
        return result.scalar_one_or_none()

    async def create(self, data: UserCreate, hashed_password: str) -> User:
        user = User(**data.model_dump(), hashed_password=hashed_password)
        self.db.add(user)
        await self.db.flush()
        await self.db.refresh(user)
        return user

    async def list_paginated(self, page: int, limit: int):
        count_query = select(func.count()).select_from(User)
        total = (await self.db.execute(count_query)).scalar()

        query = (
            select(User)
            .offset((page - 1) * limit)
            .limit(limit)
            .order_by(User.created_at.desc())
        )
        result = await self.db.execute(query)
        users = result.scalars().all()

        return users, total
```

## Background Tasks with Celery

```python
# tasks/celery_app.py
from celery import Celery
from app.core.config import settings

celery_app = Celery(
    "worker",
    broker=settings.REDIS_URL,
    backend=settings.REDIS_URL,
)

celery_app.conf.update(
    task_serializer="json",
    accept_content=["json"],
    result_serializer="json",
    timezone="UTC",
    enable_utc=True,
    task_track_started=True,
    task_acks_late=True,
    worker_prefetch_multiplier=1,
    task_routes={
        "app.tasks.email.*": {"queue": "email"},
        "app.tasks.reports.*": {"queue": "reports"},
    },
)

celery_app.autodiscover_tasks(["app.tasks"])
```

```python
# tasks/email.py
import logging
from jinja2 import Template
from app.tasks.celery_app import celery_app
from app.core.email import send_email

logger = logging.getLogger(__name__)


@celery_app.task(
    bind=True,
    max_retries=3,
    default_retry_delay=60,
    acks_late=True,
)
def send_welcome_email(self, user_id: int, email: str, name: str):
    try:
        template = Template(WELCOME_EMAIL_TEMPLATE)
        html = template.render(name=name)

        send_email(
            to=email,
            subject="Welcome!",
            html=html,
        )
        logger.info(f"Welcome email sent to {email}")
    except Exception as exc:
        logger.error(f"Failed to send email: {exc}")
        raise self.retry(exc=exc)


@celery_app.task
def send_password_reset_email(email: str, reset_token: str):
    url = f"{settings.FRONTEND_URL}/reset-password?token={reset_token}"
    send_email(
        to=email,
        subject="Password Reset",
        html=f"<p>Click <a href='{url}'>here</a> to reset your password.</p>",
    )


# Periodic tasks
from celery.schedules import crontab

celery_app.conf.beat_schedule = {
    "cleanup-expired-tokens": {
        "task": "app.tasks.cleanup.cleanup_expired_tokens",
        "schedule": crontab(hour=2, minute=0),  # Daily at 2 AM
    },
    "generate-daily-reports": {
        "task": "app.tasks.reports.generate_daily",
        "schedule": crontab(hour=6, minute=0),
    },
}
```

## Alembic Migrations

```bash
# Initialize
alembic init alembic

# Create migration
alembic revision --autogenerate -m "create_users_table"

# Apply migrations
alembic upgrade head

# Rollback
alembic downgrade -1

# Show current
alembic current
```

```python
# alembic/env.py (async)
import asyncio
from logging.config import fileConfig
from sqlalchemy.ext.asyncio import create_async_engine
from alembic import context

from app.db.session import Base
from app.core.config import settings

config = context.config
if config.config_file_name is not None:
    fileConfig(config.config_file_name)

target_metadata = Base.metadata


def run_migrations_offline():
    url = settings.DATABASE_URL
    context.configure(url=url, target_metadata=target_metadata, literal_binds=True)
    with context.begin_transaction():
        context.run_migrations()


def do_run_migrations(connection):
    context.configure(connection=connection, target_metadata=target_metadata)
    with context.begin_transaction():
        context.run_migrations()


async def run_async_migrations():
    engine = create_async_engine(settings.DATABASE_URL)
    async with engine.connect() as connection:
        await connection.run_sync(do_run_migrations)
    await engine.dispose()


def run_migrations_online():
    asyncio.run(run_async_migrations())


if context.is_offline_mode():
    run_migrations_offline()
else:
    run_migrations_online()
```

## WebSocket Support

```python
# api/v1/endpoints/ws.py
from fastapi import APIRouter, WebSocket, WebSocketDisconnect
from typing import list
import json

router = APIRouter()


class ConnectionManager:
    def __init__(self):
        self.active_connections: dict[str, list[WebSocket]] = {}

    async def connect(self, websocket: WebSocket, room: str):
        await websocket.accept()
        if room not in self.active_connections:
            self.active_connections[room] = []
        self.active_connections[room].append(websocket)

    def disconnect(self, websocket: WebSocket, room: str):
        self.active_connections[room].remove(websocket)

    async def broadcast(self, room: str, message: dict):
        if room in self.active_connections:
            for connection in self.active_connections[room]:
                await connection.send_json(message)


manager = ConnectionManager()


@router.websocket("/ws/{room}")
async def websocket_endpoint(websocket: WebSocket, room: str):
    await manager.connect(websocket, room)
    try:
        while True:
            data = await websocket.receive_text()
            message = json.loads(data)
            await manager.broadcast(room, message)
    except WebSocketDisconnect:
        manager.disconnect(websocket, room)
```

## Rate Limiting

```python
# core/rate_limit.py
from fastapi import Request, HTTPException
from redis.asyncio import Redis

from app.core.deps import get_redis


class RateLimiter:
    def __init__(self, times: int, seconds: int):
        self.times = times
        self.seconds = seconds

    async def __call__(self, request: Request):
        redis_client = get_redis()
        key = f"rate_limit:{request.url.path}:{request.client.host}"

        current = await redis_client.incr(key)
        if current == 1:
            await redis_client.expire(key, self.seconds)

        if current > self.times:
            raise HTTPException(
                status_code=429,
                detail="Too many requests"
            )

        return current


# Usage
@router.get("/expensive-operation")
@RateLimiter(times=5, seconds=60)
async def expensive_operation():
    ...
```

## Test Patterns

```python
# tests/conftest.py
import asyncio
import pytest
import pytest_asyncio
from httpx import AsyncClient, ASGITransport
from sqlalchemy.ext.asyncio import create_async_engine, async_sessionmaker

from app.main import app
from app.db.session import Base, get_db
from app.core.config import settings

# Test database
TEST_DB_URL = settings.DATABASE_URL.replace("/main", "/test")
engine = create_async_engine(TEST_DB_URL, echo=True)
TestSessionLocal = async_sessionmaker(engine, class_=AsyncSession)


@pytest.fixture(scope="session")
def event_loop():
    loop = asyncio.new_event_loop()
    yield loop
    loop.close()


@pytest_asyncio.fixture(autouse=True)
async def setup_db():
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)
    yield
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.drop_all)


@pytest_asyncio.fixture
async def db_session():
    async with TestSessionLocal() as session:
        yield session
        await session.rollback()


@pytest_asyncio.fixture
async def client(db_session):
    async def override_get_db():
        yield db_session

    app.dependency_overrides[get_db] = override_get_db

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as ac:
        yield ac

    app.dependency_overrides.clear()


@pytest_asyncio.fixture
async def auth_headers(client):
    response = await client.post("/api/v1/auth/register", json={
        "email": "test@example.com",
        "name": "Test User",
        "password": "Test1234!",
    })
    token = response.json()["access_token"]
    return {"Authorization": f"Bearer {token}"}
```

```python
# tests/test_api/test_users.py
import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
async def test_create_user(client: AsyncClient):
    response = await client.post("/api/v1/users", json={
        "email": "new@example.com",
        "name": "New User",
        "password": "Secure123!",
    })
    assert response.status_code == 201
    data = response.json()
    assert data["email"] == "new@example.com"
    assert "id" in data


@pytest.mark.asyncio
async def test_create_user_duplicate_email(client: AsyncClient):
    await client.post("/api/v1/users", json={
        "email": "dup@example.com",
        "name": "First",
        "password": "Secure123!",
    })
    response = await client.post("/api/v1/users", json={
        "email": "dup@example.com",
        "name": "Second",
        "password": "Secure123!",
    })
    assert response.status_code == 409


@pytest.mark.asyncio
async def test_get_profile(client: AsyncClient, auth_headers: dict):
    response = await client.get("/api/v1/users/me", headers=auth_headers)
    assert response.status_code == 200
    assert response.json()["email"] == "test@example.com"


@pytest.mark.asyncio
async def test_get_profile_unauthorized(client: AsyncClient):
    response = await client.get("/api/v1/users/me")
    assert response.status_code == 403
```

## OpenAPI Documentation

```python
# api/v1/router.py
from fastapi import APIRouter
from app.api.v1.endpoints import auth, users, health

api_router = APIRouter()

api_router.include_router(auth.router, prefix="/auth", tags=["Authentication"])
api_router.include_router(users.router, prefix="/users", tags=["Users"])
api_router.include_router(health.router, prefix="/health", tags=["Health"])

# Custom OpenAPI tags
def custom_openapi():
    from app.main import app
    if app.openapi_tags:
        return app.openapi()
    app.openapi_tags = [
        {"name": "Authentication", "description": "Login, register, tokens"},
        {"name": "Users", "description": "User management"},
        {"name": "Health", "description": "Health checks"},
    ]
    return app.openapi()
```

## Checklist

- [ ] Async/await used consistently
- [ ] Pydantic models for request/response validation
- [ ] Dependency injection for services
- [ ] SQLAlchemy async session with proper lifecycle
- [ ] Alembic migrations version controlled
- [ ] Celery workers configured with queues
- [ ] Rate limiting on sensitive endpoints
- [ ] WebSocket connection manager implemented
- [ ] Exception handlers registered globally
- [ ] OpenAPI docs available (dev only)
- [ ] Tests with async client and fixtures
- [ ] Health check endpoint
- [ ] Structured logging with request IDs
