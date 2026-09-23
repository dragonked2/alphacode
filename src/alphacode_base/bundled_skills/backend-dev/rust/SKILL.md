---
name: backend-dev/rust
description: Rust/Axum/Actix-web backend development — handler patterns, extractors, middleware, error handling, state management, WebSocket, authentication, validation, response streaming, testing, database integration.
---

# Rust Backend Development

## Axum Project Structure

```
src/
├── config/
│   ├── mod.rs            # Configuration structs
│   └── env.rs            # Environment loading
├── error/
│   ├── mod.rs            # Error types
│   └── response.rs       # IntoResponse impl
├── handlers/
│   ├── mod.rs
│   ├── auth.rs
│   ├── users.rs
│   └── health.rs
├── middleware/
│   ├── mod.rs
│   ├── auth.rs           # Auth middleware
│   ├── logging.rs        # Request logging
│   ├── rate_limit.rs     # Rate limiting
│   └── request_id.rs     # Correlation ID
├── models/
│   ├── mod.rs
│   ├── domain.rs         # Domain structs
│   └── database.rs       # DB models (sqlx)
├── repositories/
│   ├── mod.rs
│   └── user.rs
├── services/
│   ├── mod.rs
│   ├── auth.rs
│   └── user.rs
├── routes/
│   ├── mod.rs
│   ├── auth.rs
│   └── users.rs
├── state.rs              # AppState
├── main.rs
└── Cargo.toml
```

## Axum Application Setup

```rust
// main.rs
use axum::{Router, routing::get};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tower_http::{cors::CorsLayer, compression::CompressionLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod error;
mod handlers;
mod middleware;
mod models;
mod repositories;
mod routes;
mod services;
mod state;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config::Config::from_env()?;

    let db_pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!().run(&db_pool).await?;

    let redis = redis::Client::open(config.redis_url.clone())?;

    let state = state::AppState {
        db: db_pool,
        redis,
        config: config.clone(),
    };

    let app = Router::new()
        .merge(routes::api_routes())
        .layer(
            tower_http::ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CompressionLayer::new())
                .layer(CorsLayer::permissive())
        )
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("Server listening on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
```

## State Management

```rust
// state.rs
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: redis::Client,
    pub config: crate::config::Config,
}

// For shared state across handlers
pub type SharedState = Arc<AppState>;
```

```rust
// config/mod.rs
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    pub jwt_expires_in: i64,
    pub port: u16,
    pub cors_origins: Vec<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(config::File::with_name("config/default"))
            .add_source(config::Environment::with_prefix("APP").separator("__"))
            .build()?
            .try_deserialize()
    }
}
```

## Handler Patterns with Extractors

```rust
// handlers/users.rs
use axum::{
    extract::{Path, Query, State, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::models::domain::{UserCreate, UserUpdate, UserResponse, PaginationParams};
use crate::state::AppState;
use crate::services::user::UserService;

#[derive(Deserialize)]
pub struct ListUsersQuery {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub sort: Option<String>,
}

#[derive(Serialize)]
pub struct UserListResponse {
    pub items: Vec<UserResponse>,
    pub total: i64,
    pub page: u32,
    pub limit: u32,
}

pub async fn list_users(
    State(state): State<AppState>,
    Query(params): Query<ListUsersQuery>,
) -> Result<Json<UserListResponse>, AppError> {
    let page = params.page.unwrap_or(1);
    let limit = params.limit.unwrap_or(20).min(100);

    let service = UserService::new(&state);
    let (users, total) = service.list(page, limit).await?;

    Ok(Json(UserListResponse {
        items: users.into_iter().map(UserResponse::from).collect(),
        total,
        page,
        limit,
    }))
}

pub async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<UserResponse>, AppError> {
    let service = UserService::new(&state);
    let user = service.get_by_id(id).await?;
    Ok(Json(UserResponse::from(user)))
}

pub async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<UserCreate>,
) -> Result<(StatusCode, Json<UserResponse>), AppError> {
    let service = UserService::new(&state);
    let user = service.create(payload).await?;
    Ok((StatusCode::CREATED, Json(UserResponse::from(user))))
}

pub async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(payload): Json<UserUpdate>,
) -> Result<Json<UserResponse>, AppError> {
    let service = UserService::new(&state);
    let user = service.update(id, payload).await?;
    Ok(Json(UserResponse::from(user)))
}

pub async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<StatusCode, AppError> {
    let service = UserService::new(&state);
    service.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

## Extractors (Custom)

```rust
// middleware/auth.rs
use axum::{
    extract::{FromRequestParts, Request},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: i32,
    pub email: String,
    pub roles: Vec<String>,
    pub exp: usize,
}

// Extractor for getting current user
pub struct AuthUser(pub Claims);

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut http::request::Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized("Missing authorization header".into()))?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(AppError::Unauthorized("Invalid authorization format".into()))?;

        let state = parts.extensions.get::<AppState>()
            .ok_or(AppError::Internal("Missing app state".into()))?;

        let claims = decode::<Claims>(
            token,
            &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map(|data| data.claims)
        .map_err(|_| AppError::Unauthorized("Invalid token".into()))?;

        Ok(AuthUser(claims))
    }
}

// Middleware version
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let auth_header = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized("Missing authorization header".into()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AppError::Unauthorized("Invalid authorization format".into()))?;

    let claims = decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| AppError::Unauthorized("Invalid token".into()))?;

    req.extensions_mut().insert(claims);

    Ok(next.run(req).await)
}

// Authorization middleware
pub fn require_role(role: String) -> impl FnMut(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, AppError>> + Send>> {
    move |req: Request, next: Next| {
        let role = role.clone();
        Box::pin(async move {
            let claims = req
                .extensions()
                .get::<Claims>()
                .ok_or(AppError::Unauthorized("Not authenticated".into()))?;

            if !claims.roles.contains(&role) {
                return Err(AppError::Forbidden("Insufficient permissions".into()));
            }

            Ok(next.run(req).await)
        })
    }
}
```

## Error Handling

```rust
// error/mod.rs
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    Conflict(String),
    Validation(Vec<ValidationError>),
    Internal(String),
    Database(sqlx::Error),
}

#[derive(Debug, serde::Serialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Not found: {}", msg),
            Self::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            Self::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            Self::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            Self::Conflict(msg) => write!(f, "Conflict: {}", msg),
            Self::Validation(_) => write!(f, "Validation error"),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
            Self::Database(e) => write!(f, "Database error: {}", e),
        }
    }
}

impl std::error::Error for AppError {}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self::NotFound("Resource not found".into()),
            _ => Self::Database(err),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg, None),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg, None),
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg, None),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, "FORBIDDEN", msg, None),
            Self::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", msg, None),
            Self::Validation(errors) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "VALIDATION_ERROR",
                "Validation failed".into(),
                Some(errors),
            ),
            Self::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                msg,
                None,
            ),
            Self::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                "An internal error occurred".into(),
                None,
            ),
        };

        let body = json!({
            "error": {
                "code": code,
                "message": message,
                "details": details,
            }
        });

        (status, Json(body)).into_response()
    }
}
```

## Request Validation with Validator

```rust
// models/domain.rs
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

#[derive(Debug, Deserialize, Validate)]
pub struct UserCreate {
    #[validate(email(message = "Invalid email format"))]
    pub email: String,

    #[validate(length(min = 2, max = 100, message = "Name must be 2-100 characters"))]
    pub name: String,

    #[validate(
        length(min = 8, message = "Password must be at least 8 characters"),
        custom(function = "validate_password_strength")
    )]
    pub password: String,
}

fn validate_password_strength(password: &str) -> Result<(), ValidationError> {
    let has_uppercase = password.chars().any(|c| c.is_uppercase());
    let has_lowercase = password.chars().any(|c| c.is_lowercase());
    let has_digit = password.chars().any(|c| c.is_numeric());

    if !has_uppercase || !has_lowercase || !has_digit {
        return Err(ValidationError::new(
            "Password must contain uppercase, lowercase, and digit",
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize, Validate)]
pub struct UserUpdate {
    #[validate(length(min = 2, max = 100))]
    pub name: Option<String>,

    #[validate(email)]
    pub email: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct PaginationParams {
    #[validate(range(min = 1))]
    pub page: Option<u32>,

    #[validate(range(min = 1, max = 100))]
    pub limit: Option<u32>,

    #[validate(regex(path = "SORT_REGEX", message = "Invalid sort field"))]
    pub sort: Option<String>,
}

lazy_static::lazy_static! {
    static ref SORT_REGEX: regex::Regex = regex::Regex::new(r"^(created_at|name|email)$").unwrap();
}
```

```rust
// handlers/users.rs (with validation)
use validator::Validate;

pub async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<UserCreate>,
) -> Result<(StatusCode, Json<UserResponse>), AppError> {
    payload.validate().map_err(|e| {
        let errors: Vec<ValidationError> = e.field_errors().into_iter().map(|(field, errs)| {
            ValidationError {
                field,
                message: errs.into_iter().next().map(|e| e.message.unwrap_or_default()).unwrap_or_default(),
            }
        }).collect();
        AppError::Validation(errors)
    })?;

    let service = UserService::new(&state);
    let user = service.create(payload).await?;
    Ok((StatusCode::CREATED, Json(UserResponse::from(user))))
}
```

## Rate Limiting Middleware

```rust
// middleware/rate_limit.rs
use axum::{extract::ConnectInfo, http::StatusCode, response::Response, middleware::Next};
use std::net::SocketAddr;
use tokio::sync::RwLock;
use std::collections::HashMap;

use crate::error::AppError;

struct RateLimitEntry {
    count: u32,
    window_start: std::time::Instant,
}

pub struct RateLimiter {
    limits: RwLock<HashMap<String, RateLimitEntry>>,
    max_requests: u32,
    window_secs: u64,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            limits: RwLock::new(HashMap::new()),
            max_requests,
            window_secs,
        }
    }

    pub async fn check(&self, key: &str) -> Result<u32, AppError> {
        let mut limits = self.limits.write().await;
        let now = std::time::Instant::now();

        let entry = limits.entry(key.to_string()).or_insert(RateLimitEntry {
            count: 0,
            window_start: now,
        });

        if now.duration_since(entry.window_start).as_secs() > self.window_secs {
            entry.count = 0;
            entry.window_start = now;
        }

        entry.count += 1;

        if entry.count > self.max_requests {
            return Err(AppError::BadRequest("Rate limit exceeded".into()));
        }

        Ok(self.max_requests - entry.count)
    }
}

pub async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut req: axum::http::Request,
    next: Next,
) -> Result<Response, AppError> {
    let limiter = req.extensions().get::<RateLimiter>().cloned()
        .ok_or(AppError::Internal("Missing rate limiter".into()))?;

    let key = format!("{}:{}", addr.ip(), req.uri().path());
    let remaining = limiter.check(&key).await?;

    let mut response = next.run(req).await;
    response.headers_mut().insert("X-RateLimit-Remaining", remaining.to_string().parse().unwrap());

    Ok(response)
}
```

## WebSocket Support

```rust
// handlers/ws.rs
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct WsMessage {
    event: String,
    data: serde_json::Value,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: crate::state::AppState) {
    let (mut sender, mut receiver) = socket.split();

    let (tx, mut rx) = broadcast::channel::<String>(100);

    // Forward messages to client
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Receive messages from client
    let tx_clone = tx.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            let msg: WsMessage = serde_json::from_str(&text).unwrap();
            let response = serde_json::json!({
                "event": msg.event,
                "data": msg.data,
            });
            let _ = tx_clone.send(response.to_string());
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}
```

## Database Integration (sqlx)

```rust
// models/database.rs
use chrono::{DateTime, Utc};
use sqlx::FromRow;

#[derive(Debug, FromRow)]
pub struct User {
    pub id: i32,
    pub email: String,
    pub name: String,
    pub role: String,
    pub hashed_password: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

```rust
// repositories/user.rs
use sqlx::PgPool;

use crate::error::AppError;
use crate::models::database::User;
use crate::models::domain::UserCreate;

pub struct UserRepository<'a> {
    db: &'a PgPool,
}

impl<'a> UserRepository<'a> {
    pub fn new(db: &'a PgPool) -> Self {
        Self { db }
    }

    pub async fn find_by_id(&self, id: i32) -> Result<User, AppError> {
        let user = sqlx::query_as::<_, User>(
            "SELECT * FROM users WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(self.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {} not found", id)))?;

        Ok(user)
    }

    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, AppError> {
        let user = sqlx::query_as::<_, User>(
            "SELECT * FROM users WHERE email = $1"
        )
        .bind(email)
        .fetch_optional(self.db)
        .await?;

        Ok(user)
    }

    pub async fn create(&self, data: UserCreate, hashed_password: &str) -> Result<User, AppError> {
        let user = sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (email, name, role, hashed_password)
            VALUES ($1, $2, 'user', $3)
            RETURNING *
            "#,
        )
        .bind(&data.email)
        .bind(&data.name)
        .bind(hashed_password)
        .fetch_one(self.db)
        .await?;

        Ok(user)
    }

    pub async fn list_paginated(
        &self,
        page: u32,
        limit: u32,
    ) -> Result<(Vec<User>, i64), AppError> {
        let offset = (page - 1) * limit;

        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(self.db)
            .await?;

        let users = sqlx::query_as::<_, User>(
            "SELECT * FROM users ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(self.db)
        .await?;

        Ok((users, total.0))
    }

    pub async fn update(
        &self,
        id: i32,
        email: Option<&str>,
        name: Option<&str>,
    ) -> Result<User, AppError> {
        let user = sqlx::query_as::<_, User>(
            r#"
            UPDATE users
            SET email = COALESCE($2, email),
                name = COALESCE($3, name),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(email)
        .bind(name)
        .fetch_optional(self.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {} not found", id)))?;

        Ok(user)
    }

    pub async fn delete(&self, id: i32) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(self.db)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("User {} not found", id)));
        }

        Ok(())
    }
}
```

## Router Configuration

```rust
// routes/mod.rs
use axum::{
    middleware,
    routing::{get, post, put, delete},
    Router,
};
use tower_http::services::{ServeDir, ServeFile};

use crate::handlers::{auth, health, users};
use crate::middleware::{auth_middleware, request_id, logging};
use crate::state::AppState;

pub fn api_routes() -> Router<AppState> {
    let auth_routes = Router::new()
        .route("/register", post(auth::register))
        .route("/login", post(auth::login))
        .route("/refresh", post(auth::refresh));

    let user_routes = Router::new()
        .route("/", get(users::list_users).post(users::create_user))
        .route(
            "/{id}",
            get(users::get_user)
                .put(users::update_user)
                .delete(users::delete_user),
        )
        .route_layer(middleware::from_fn(auth_middleware));

    let admin_routes = Router::new()
        .route("/admin/users", get(users::list_users))
        .route_layer(middleware::from_fn(auth_middleware))
        .route_layer(middleware::from_fn(crate::middleware::auth::require_role("admin".into())));

    Router::new()
        .route("/health", get(health::health_check))
        .nest("/api/v1/auth", auth_routes)
        .nest("/api/v1/users", user_routes)
        .merge(admin_routes)
        .layer(middleware::from_fn(request_id::request_id_middleware))
        .layer(middleware::from_fn(logging::logging_middleware))
}
```

## Testing

```rust
#[cfg(test)]
mod tests {
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use sqlx::PgPool;

    use crate::app;

    #[sqlx::test]
    async fn test_list_users(pool: PgPool) {
        let app = app::create_app(pool).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/users")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[sqlx::test]
    async fn test_create_user(pool: PgPool) {
        let app = app::create_app(pool).await;

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/users")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::json!({
                            "email": "test@example.com",
                            "name": "Test User",
                            "password": "Secure123!"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
    }
}
```

## Cargo.toml

```toml
[package]
name = "my-backend"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = { version = "0.7", features = ["ws", "macros"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.7", features = ["runtime-tokio", "postgres", "chrono", "uuid"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "compression", "trace"] }
jsonwebtoken = "9"
redis = { version = "0.24", features = ["tokio-comp"] }
validator = { version = "0.16", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
thiserror = "1"
config = "0.13"
lazy_static = "1"

[dev-dependencies]
tower = { version = "0.4", features = ["util"] }
http-body-util = "0.1"
```

## Checklist

- [ ] Axum router configured with proper nesting
- [ ] Extractors for auth, query params, path params
- [ ] Custom error type implementing IntoResponse
- [ ] Request validation with validator crate
- [ ] Rate limiting middleware
- [ ] Auth middleware with JWT validation
- [ ] Role-based authorization
- [ ] sqlx repository pattern
- [ ] WebSocket handler
- [ ] Structured tracing/logging
- [ ] Request ID middleware
- [ ] Graceful shutdown
- [ ] Unit and integration tests
