use std::{env, net::SocketAddr, sync::Arc, time::Duration as StdDuration};

use aws_config::BehaviorVersion;
use aws_sdk_s3::{Client as S3Client, presigning::PresigningConfig, primitives::ByteStream};
use aws_types::region::Region;
use axum::{
    Json, Router,
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use chrono::{DateTime, Duration, Utc};
use jsonschema::JSONSchema;
use reqwest::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool, postgres::PgPoolOptions};
use thiserror::Error;
use tower_http::cors::CorsLayer;
use tracing::{error, info};
use url::Url;
use uuid::Uuid;

const SESSION_COOKIE_NAME: &str = "theme_store_session";
const DEFAULT_THEME_FILE_NAME: &str = "theme.json";

#[derive(Clone)]
struct AppState {
    db: PgPool,
    auth: AuthConfig,
    storage: StorageConfig,
    http_client: Client,
    s3_client: S3Client,
    theme_schema: Arc<JSONSchema>,
}

#[derive(Clone)]
struct AuthConfig {
    github_client_id: String,
    github_client_secret: String,
    github_redirect_uri: String,
    session_cookie_secure: bool,
    session_cookie_domain: Option<String>,
    session_ttl_hours: i64,
    post_auth_redirect: Option<String>,
}

#[derive(Clone)]
struct StorageConfig {
    bucket: String,
    key_prefix: String,
    presign_ttl_seconds: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let _ = dotenvy::dotenv();

    let database_url = env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set for termy_api"))?;
    let bind_addr = env::var("THEME_STORE_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());

    let auth = AuthConfig {
        github_client_id: env::var("GITHUB_CLIENT_ID")
            .map_err(|_| anyhow::anyhow!("GITHUB_CLIENT_ID must be set"))?,
        github_client_secret: env::var("GITHUB_CLIENT_SECRET")
            .map_err(|_| anyhow::anyhow!("GITHUB_CLIENT_SECRET must be set"))?,
        github_redirect_uri: env::var("GITHUB_REDIRECT_URI")
            .map_err(|_| anyhow::anyhow!("GITHUB_REDIRECT_URI must be set"))?,
        session_cookie_secure: env::var("SESSION_COOKIE_SECURE")
            .ok()
            .map(|value| parse_bool_env(&value, "SESSION_COOKIE_SECURE"))
            .transpose()?
            .unwrap_or(false),
        session_cookie_domain: env::var("SESSION_COOKIE_DOMAIN")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        session_ttl_hours: env::var("SESSION_TTL_HOURS")
            .ok()
            .map(|value| parse_i64_env(&value, "SESSION_TTL_HOURS"))
            .transpose()?
            .unwrap_or(24 * 7),
        post_auth_redirect: env::var("POST_AUTH_REDIRECT").ok(),
    };

    let storage = StorageConfig {
        bucket: env::var("S3_BUCKET").map_err(|_| anyhow::anyhow!("S3_BUCKET must be set"))?,
        key_prefix: env::var("S3_KEY_PREFIX").unwrap_or_else(|_| "themes".to_string()),
        presign_ttl_seconds: env::var("S3_PRESIGN_TTL_SECONDS")
            .ok()
            .map(|value| parse_u64_env(&value, "S3_PRESIGN_TTL_SECONDS"))
            .transpose()?
            .unwrap_or(900),
    };

    let s3_region = env::var("S3_REGION").map_err(|_| anyhow::anyhow!("S3_REGION must be set"))?;
    let s3_endpoint = env::var("S3_ENDPOINT").ok();

    let mut aws_loader =
        aws_config::defaults(BehaviorVersion::latest()).region(Region::new(s3_region));
    if let Some(endpoint) = s3_endpoint.as_ref() {
        aws_loader = aws_loader.endpoint_url(endpoint);
    }
    let aws_config = aws_loader.load().await;

    let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&aws_config);
    if s3_endpoint.is_some() {
        s3_config_builder = s3_config_builder.force_path_style(true);
    }
    let s3_client = S3Client::from_conf(s3_config_builder.build());

    let theme_schema_json: Value = serde_json::from_str(include_str!("../../../theme.schema.json"))
        .map_err(|err| anyhow::anyhow!("failed to parse theme.schema.json at startup: {err}"))?;
    let theme_schema =
        Arc::new(JSONSchema::compile(&theme_schema_json).map_err(|err| {
            anyhow::anyhow!("failed to compile theme.schema.json at startup: {err}")
        })?);

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    sqlx::migrate!().run(&pool).await?;

    let state = AppState {
        db: pool,
        auth,
        storage,
        http_client: Client::builder().build()?,
        s3_client,
        theme_schema,
    };
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    let socket_addr: SocketAddr = listener.local_addr()?;
    info!(%socket_addr, "theme store API listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn build_router(state: AppState) -> Router {
    let cors = build_cors_layer();
    Router::new()
        .route("/health", get(health))
        .route("/auth/github/login", get(auth_github_login))
        .route("/auth/github/callback", get(auth_github_callback))
        .route("/auth/me", get(auth_me))
        .route(
            "/auth/device-session",
            axum::routing::post(auth_device_session),
        )
        .route("/auth/logout", axum::routing::post(auth_logout))
        .route("/themes/me", get(list_my_themes))
        .route("/themes", get(list_themes).post(create_theme))
        .route("/themes/{slug}", get(get_theme).patch(update_theme))
        .route(
            "/themes/{slug}/versions",
            get(list_theme_versions).post(publish_theme_version),
        )
        .layer(cors)
        .with_state(state)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            error!(?err, "failed to install Ctrl+C signal handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => {
                error!(?err, "failed to install terminate signal handler");
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "termy_api=info".into()),
        )
        .compact()
        .init();
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct Theme {
    id: Uuid,
    name: String,
    slug: String,
    description: String,
    latest_version: Option<String>,
    file_key: Option<String>,
    #[sqlx(default)]
    file_url: Option<String>,
    github_username_claim: String,
    github_user_id_claim: Option<i64>,
    is_public: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct ThemeVersion {
    id: Uuid,
    theme_id: Uuid,
    version: String,
    file_key: String,
    #[sqlx(default)]
    file_url: Option<String>,
    changelog: String,
    checksum_sha256: Option<String>,
    created_by: Option<String>,
    published_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct AuthUser {
    id: Uuid,
    github_user_id: i64,
    github_login: String,
    avatar_url: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthDeviceSessionResponse {
    session_token: String,
    user: AuthUser,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateThemeRequest {
    name: Option<String>,
    description: Option<String>,
    is_public: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthGithubLoginQuery {
    redirect_to: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthGithubCallbackQuery {
    code: String,
    state: String,
}

#[derive(Debug, Deserialize)]
struct GithubTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct GithubUserResponse {
    id: i64,
    login: String,
    avatar_url: Option<String>,
    name: Option<String>,
}

#[derive(Debug)]
struct CreateThemeUpload {
    name: String,
    description: String,
    is_public: bool,
    version: String,
    changelog: Option<String>,
    checksum_sha256: Option<String>,
    github_username_claim: Option<String>,
    theme_json: Vec<u8>,
}

#[derive(Debug)]
struct PublishThemeVersionUpload {
    version: String,
    changelog: Option<String>,
    checksum_sha256: Option<String>,
    created_by: Option<String>,
    theme_json: Vec<u8>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ThemeWithVersionsResponse {
    theme: Theme,
    versions: Vec<ThemeVersion>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublishThemeVersionResponse {
    theme: Theme,
    version: ThemeVersion,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn auth_github_login(
    State(state): State<AppState>,
    Query(query): Query<AuthGithubLoginQuery>,
) -> Result<Redirect, ApiError> {
    sqlx::query("DELETE FROM oauth_state WHERE expires_at < NOW()")
        .execute(&state.db)
        .await?;

    let oauth_state = Uuid::new_v4().simple().to_string();
    let expires_at = Utc::now() + Duration::minutes(10);

    sqlx::query("INSERT INTO oauth_state (state, redirect_to, expires_at) VALUES ($1, $2, $3)")
        .bind(&oauth_state)
        .bind(query.redirect_to)
        .bind(expires_at)
        .execute(&state.db)
        .await?;

    let mut authorize_url =
        Url::parse("https://github.com/login/oauth/authorize").map_err(|_| {
            ApiError::ExternalAuth("failed to build GitHub authorization URL".to_string())
        })?;

    authorize_url
        .query_pairs_mut()
        .append_pair("client_id", &state.auth.github_client_id)
        .append_pair("redirect_uri", &state.auth.github_redirect_uri)
        .append_pair("scope", "read:user")
        .append_pair("state", &oauth_state);

    Ok(Redirect::to(authorize_url.as_str()))
}

async fn auth_github_callback(
    State(state): State<AppState>,
    Query(query): Query<AuthGithubCallbackQuery>,
) -> Result<Response, ApiError> {
    let redirect_to = sqlx::query_scalar::<_, Option<String>>(
        "DELETE FROM oauth_state WHERE state = $1 AND expires_at > NOW() RETURNING redirect_to",
    )
    .bind(&query.state)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::BadRequest("invalid or expired OAuth state".to_string()))?;

    let token_response = state
        .http_client
        .post("https://github.com/login/oauth/access_token")
        .header(header::ACCEPT, "application/json")
        .form(&[
            ("client_id", state.auth.github_client_id.as_str()),
            ("client_secret", state.auth.github_client_secret.as_str()),
            ("code", query.code.as_str()),
            ("redirect_uri", state.auth.github_redirect_uri.as_str()),
        ])
        .send()
        .await
        .map_err(|err| {
            ApiError::ExternalAuth(format!("failed to call GitHub token endpoint: {err}"))
        })?;

    if !token_response.status().is_success() {
        return Err(ApiError::ExternalAuth(format!(
            "GitHub token exchange failed with status {}",
            token_response.status()
        )));
    }

    let token_payload = token_response
        .json::<GithubTokenResponse>()
        .await
        .map_err(|err| {
            ApiError::ExternalAuth(format!("invalid token response from GitHub: {err}"))
        })?;

    let github_user_response = state
        .http_client
        .get("https://api.github.com/user")
        .header(header::ACCEPT, "application/vnd.github+json")
        .header(header::USER_AGENT, "termy-api")
        .bearer_auth(&token_payload.access_token)
        .send()
        .await
        .map_err(|err| ApiError::ExternalAuth(format!("failed to fetch GitHub user: {err}")))?;

    if !github_user_response.status().is_success() {
        return Err(ApiError::ExternalAuth(format!(
            "GitHub user fetch failed with status {}",
            github_user_response.status()
        )));
    }

    let github_user = github_user_response
        .json::<GithubUserResponse>()
        .await
        .map_err(|err| {
            ApiError::ExternalAuth(format!("invalid user response from GitHub: {err}"))
        })?;

    let auth_user = sqlx::query_as::<_, AuthUser>(
        r#"
        INSERT INTO user_account (github_user_id, github_login, avatar_url, name)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (github_user_id)
        DO UPDATE SET
            github_login = EXCLUDED.github_login,
            avatar_url = EXCLUDED.avatar_url,
            name = EXCLUDED.name,
            updated_at = NOW()
        RETURNING id, github_user_id, github_login, avatar_url, name
        "#,
    )
    .bind(github_user.id)
    .bind(github_user.login)
    .bind(github_user.avatar_url)
    .bind(github_user.name)
    .fetch_one(&state.db)
    .await?;

    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let token_hash = hash_token(&token);
    let expires_at = Utc::now() + Duration::hours(state.auth.session_ttl_hours);

    sqlx::query("INSERT INTO user_session (user_id, token_hash, expires_at) VALUES ($1, $2, $3)")
        .bind(auth_user.id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(&state.db)
        .await?;

    let target = redirect_to
        .or_else(|| state.auth.post_auth_redirect.clone())
        .unwrap_or_else(|| "/themes".to_string());
    let response = if is_termy_deeplink(&target) {
        let native_target = build_native_auth_redirect_target(&target, &token, &auth_user)
            .map_err(|_| {
                ApiError::ExternalAuth("failed to build native auth redirect".to_string())
            })?;
        Redirect::to(&native_target).into_response()
    } else {
        let cookie = build_session_cookie(
            &token,
            state.auth.session_ttl_hours,
            state.auth.session_cookie_secure,
            state.auth.session_cookie_domain.as_deref(),
        );
        let mut response = Redirect::to(&target).into_response();
        response.headers_mut().append(
            header::SET_COOKIE,
            HeaderValue::from_str(&cookie)
                .map_err(|_| ApiError::ExternalAuth("failed to set session cookie".to_string()))?,
        );
        response
    };

    Ok(response)
}

async fn auth_me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthUser>, ApiError> {
    let user = require_auth_user(&state, &headers).await?;
    Ok(Json(user))
}

async fn auth_device_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthDeviceSessionResponse>, ApiError> {
    let user = require_auth_user(&state, &headers).await?;
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let token_hash = hash_token(&token);
    let expires_at = Utc::now() + Duration::hours(state.auth.session_ttl_hours);

    sqlx::query("INSERT INTO user_session (user_id, token_hash, expires_at) VALUES ($1, $2, $3)")
        .bind(user.id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(&state.db)
        .await?;

    Ok(Json(AuthDeviceSessionResponse {
        session_token: token,
        user,
    }))
}

async fn auth_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(token) = extract_auth_token(&headers) {
        let token_hash = hash_token(&token);
        sqlx::query("DELETE FROM user_session WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&state.db)
            .await?;
    }

    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&clear_session_cookie(
            state.auth.session_cookie_secure,
            state.auth.session_cookie_domain.as_deref(),
        ))
        .map_err(|_| ApiError::ExternalAuth("failed to clear session cookie".to_string()))?,
    );

    Ok(response)
}

async fn list_themes(State(state): State<AppState>) -> Result<Json<Vec<Theme>>, ApiError> {
    let mut themes = sqlx::query_as::<_, Theme>(
        r#"
        SELECT
            id,
            name,
            slug,
            description,
            latest_version,
            file_key,
            github_username_claim,
            github_user_id_claim,
            is_public,
            created_at,
            updated_at
        FROM theme
        WHERE is_public = TRUE
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    for theme in &mut themes {
        attach_theme_file_url(&state, theme).await?;
    }

    Ok(Json(themes))
}

async fn list_my_themes(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Theme>>, ApiError> {
    let auth_user = require_auth_user(&state, &headers).await?;

    let mut themes = sqlx::query_as::<_, Theme>(
        r#"
        SELECT
            id,
            name,
            slug,
            description,
            latest_version,
            file_key,
            github_username_claim,
            github_user_id_claim,
            is_public,
            created_at,
            updated_at
        FROM theme
        WHERE
            github_user_id_claim = $1
            OR (
                github_user_id_claim IS NULL
                AND lower(github_username_claim) = lower($2)
            )
        ORDER BY created_at DESC
        "#,
    )
    .bind(auth_user.github_user_id)
    .bind(auth_user.github_login)
    .fetch_all(&state.db)
    .await?;

    for theme in &mut themes {
        attach_theme_file_url(&state, theme).await?;
    }

    Ok(Json(themes))
}

async fn get_theme(
    Path(slug): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Theme>, ApiError> {
    let mut theme = fetch_theme_by_slug(&state.db, &slug).await?;
    ensure_can_read_theme(&theme, &state, &headers).await?;
    attach_theme_file_url(&state, &mut theme).await?;
    Ok(Json(theme))
}

async fn create_theme(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<(StatusCode, Json<Theme>), ApiError> {
    let auth_user = require_auth_user(&state, &headers).await?;
    let payload = parse_create_theme_upload(multipart).await?;
    let normalized_version = parse_semver_version(&payload.version, "version")?;
    let generated_slug = generate_unique_slug(&state.db, &payload.name).await?;

    validate_theme_json(&state.theme_schema, &payload.theme_json)?;

    if let Some(claimed_login) = payload.github_username_claim
        && !claimed_login.eq_ignore_ascii_case(&auth_user.github_login)
    {
        return Err(ApiError::BadRequest(
            "githubUsernameClaim must match the authenticated GitHub user".to_string(),
        ));
    }

    let file_key = build_theme_file_key(
        &state.storage.key_prefix,
        &generated_slug,
        &normalized_version,
    );
    upload_theme_to_s3(&state, &file_key, &payload.theme_json).await?;

    let mut tx = state.db.begin().await?;

    let theme_result = sqlx::query_as::<_, Theme>(
        r#"
        INSERT INTO theme (
            name,
            slug,
            description,
            latest_version,
            file_key,
            github_username_claim,
            github_user_id_claim,
            is_public
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING
            id,
            name,
            slug,
            description,
            latest_version,
            file_key,
            github_username_claim,
            github_user_id_claim,
            is_public,
            created_at,
            updated_at
        "#,
    )
    .bind(payload.name)
    .bind(generated_slug)
    .bind(payload.description)
    .bind(normalized_version.clone())
    .bind(file_key.clone())
    .bind(auth_user.github_login)
    .bind(auth_user.github_user_id)
    .bind(payload.is_public)
    .fetch_one(&mut *tx)
    .await;

    let theme = match theme_result {
        Ok(theme) => theme,
        Err(err) if is_unique_violation(&err) => {
            return Err(ApiError::Conflict("theme slug already exists".to_string()));
        }
        Err(err) => return Err(ApiError::from(err)),
    };

    sqlx::query(
        r#"
        INSERT INTO theme_version (
            theme_id,
            version,
            file_key,
            changelog,
            checksum_sha256,
            created_by
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(theme.id)
    .bind(normalized_version)
    .bind(file_key)
    .bind(payload.changelog.unwrap_or_default())
    .bind(payload.checksum_sha256)
    .bind(Some(theme.github_username_claim.clone()))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let mut theme = theme;
    attach_theme_file_url(&state, &mut theme).await?;

    Ok((StatusCode::CREATED, Json(theme)))
}

async fn update_theme(
    Path(slug): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpdateThemeRequest>,
) -> Result<Json<Theme>, ApiError> {
    let auth_user = require_auth_user(&state, &headers).await?;
    let current_theme = fetch_theme_by_slug(&state.db, &slug).await?;
    ensure_theme_owner(&current_theme, &auth_user)?;

    let has_updates =
        request.name.is_some() || request.description.is_some() || request.is_public.is_some();

    if !has_updates {
        return Err(ApiError::BadRequest(
            "provide at least one field to update".to_string(),
        ));
    }

    let name = request
        .name
        .map(|value| parse_required_field(value, "name"))
        .transpose()?;

    let mut theme = sqlx::query_as::<_, Theme>(
        r#"
        UPDATE theme
        SET
            name = COALESCE($2, name),
            description = COALESCE($3, description),
            is_public = COALESCE($4, is_public),
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            name,
            slug,
            description,
            latest_version,
            file_key,
            github_username_claim,
            github_user_id_claim,
            is_public,
            created_at,
            updated_at
        "#,
    )
    .bind(current_theme.id)
    .bind(name)
    .bind(request.description)
    .bind(request.is_public)
    .fetch_one(&state.db)
    .await?;

    attach_theme_file_url(&state, &mut theme).await?;

    Ok(Json(theme))
}

async fn publish_theme_version(
    Path(slug): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<(StatusCode, Json<PublishThemeVersionResponse>), ApiError> {
    let auth_user = require_auth_user(&state, &headers).await?;
    let current_theme = fetch_theme_by_slug(&state.db, &slug).await?;
    ensure_theme_owner(&current_theme, &auth_user)?;

    let payload = parse_publish_theme_version_upload(multipart).await?;
    let normalized_version = parse_semver_version(&payload.version, "version")?;
    validate_theme_json(&state.theme_schema, &payload.theme_json)?;

    if let Some(current_latest) = &current_theme.latest_version
        && let Ok(current_latest_version) = Version::parse(current_latest)
    {
        let next = Version::parse(&normalized_version).map_err(|err| {
            ApiError::BadRequest(format!(
                "version must be a valid semantic version (e.g. 1.2.3): {err}"
            ))
        })?;
        if next <= current_latest_version {
            return Err(ApiError::BadRequest(format!(
                "version must be greater than current latest version ({current_latest})"
            )));
        }
    }

    let existing_version = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT id FROM theme_version WHERE theme_id = $1 AND version = $2",
    )
    .bind(current_theme.id)
    .bind(&normalized_version)
    .fetch_optional(&state.db)
    .await?
    .flatten();
    if existing_version.is_some() {
        return Err(ApiError::Conflict(
            "this version already exists for the theme".to_string(),
        ));
    }

    let file_key = build_theme_file_key(
        &state.storage.key_prefix,
        &current_theme.slug,
        &normalized_version,
    );
    upload_theme_to_s3(&state, &file_key, &payload.theme_json).await?;

    let mut transaction = state.db.begin().await?;

    let version_result = sqlx::query_as::<_, ThemeVersion>(
        r#"
        INSERT INTO theme_version (
            theme_id,
            version,
            file_key,
            changelog,
            checksum_sha256,
            created_by
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING
            id,
            theme_id,
            version,
            file_key,
            changelog,
            checksum_sha256,
            created_by,
            published_at,
            created_at
        "#,
    )
    .bind(current_theme.id)
    .bind(normalized_version.clone())
    .bind(file_key.clone())
    .bind(payload.changelog.unwrap_or_default())
    .bind(payload.checksum_sha256)
    .bind(payload.created_by.or(Some(auth_user.github_login)))
    .fetch_one(&mut *transaction)
    .await;

    let mut inserted_version = match version_result {
        Ok(inserted) => inserted,
        Err(err) if is_unique_violation(&err) => {
            return Err(ApiError::Conflict(
                "this version already exists for the theme".to_string(),
            ));
        }
        Err(err) => return Err(ApiError::from(err)),
    };

    let mut updated_theme = sqlx::query_as::<_, Theme>(
        r#"
        UPDATE theme
        SET
            latest_version = $1,
            file_key = $2,
            updated_at = NOW()
        WHERE id = $3
        RETURNING
            id,
            name,
            slug,
            description,
            latest_version,
            file_key,
            github_username_claim,
            github_user_id_claim,
            is_public,
            created_at,
            updated_at
        "#,
    )
    .bind(normalized_version)
    .bind(file_key)
    .bind(current_theme.id)
    .fetch_one(&mut *transaction)
    .await?;

    transaction.commit().await?;
    attach_theme_file_url(&state, &mut updated_theme).await?;
    attach_version_file_url(&state, &mut inserted_version).await?;

    Ok((
        StatusCode::CREATED,
        Json(PublishThemeVersionResponse {
            theme: updated_theme,
            version: inserted_version,
        }),
    ))
}

async fn list_theme_versions(
    Path(slug): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ThemeWithVersionsResponse>, ApiError> {
    let mut theme = fetch_theme_by_slug(&state.db, &slug).await?;
    ensure_can_read_theme(&theme, &state, &headers).await?;

    let mut versions = sqlx::query_as::<_, ThemeVersion>(
        r#"
        SELECT
            id,
            theme_id,
            version,
            file_key,
            changelog,
            checksum_sha256,
            created_by,
            published_at,
            created_at
        FROM theme_version
        WHERE theme_id = $1
        ORDER BY published_at DESC, created_at DESC
        "#,
    )
    .bind(theme.id)
    .fetch_all(&state.db)
    .await?;

    attach_theme_file_url(&state, &mut theme).await?;
    for version in &mut versions {
        attach_version_file_url(&state, version).await?;
    }

    Ok(Json(ThemeWithVersionsResponse { theme, versions }))
}

async fn fetch_theme_by_slug(pool: &PgPool, slug: &str) -> Result<Theme, ApiError> {
    let theme = sqlx::query_as::<_, Theme>(
        r#"
        SELECT
            id,
            name,
            slug,
            description,
            latest_version,
            file_key,
            github_username_claim,
            github_user_id_claim,
            is_public,
            created_at,
            updated_at
        FROM theme
        WHERE slug = $1
        "#,
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("theme not found".to_string()))?;

    Ok(theme)
}

async fn parse_create_theme_upload(
    mut multipart: Multipart,
) -> Result<CreateThemeUpload, ApiError> {
    let mut name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut is_public: Option<bool> = None;
    let mut version: Option<String> = None;
    let mut changelog: Option<String> = None;
    let mut checksum_sha256: Option<String> = None;
    let mut github_username_claim: Option<String> = None;
    let mut theme_json: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        let field_name = field.name().unwrap_or_default().to_string();

        match field_name.as_str() {
            "name" => {
                name = Some(parse_required_field(
                    field.text().await.map_err(multipart_error)?,
                    "name",
                )?);
            }
            "description" => {
                description = Some(field.text().await.map_err(multipart_error)?);
            }
            "isPublic" => {
                let raw = field.text().await.map_err(multipart_error)?;
                is_public = Some(parse_bool_field(&raw, "isPublic")?);
            }
            "version" => {
                version = Some(parse_required_field(
                    field.text().await.map_err(multipart_error)?,
                    "version",
                )?);
            }
            "changelog" => {
                changelog = Some(field.text().await.map_err(multipart_error)?);
            }
            "checksumSha256" => {
                checksum_sha256 = Some(field.text().await.map_err(multipart_error)?);
            }
            "githubUsernameClaim" => {
                github_username_claim = Some(field.text().await.map_err(multipart_error)?);
            }
            "themeJson" => {
                theme_json = Some(field.text().await.map_err(multipart_error)?.into_bytes());
            }
            "themeFile" | "file" => {
                theme_json = Some(field.bytes().await.map_err(multipart_error)?.to_vec());
            }
            _ => {}
        }
    }

    let name = name.ok_or_else(|| ApiError::BadRequest("name is required".to_string()))?;
    let version = version.unwrap_or_else(|| "1.0.0".to_string());
    let theme_json =
        theme_json.ok_or_else(|| ApiError::BadRequest("themeFile JSON is required".to_string()))?;

    Ok(CreateThemeUpload {
        name,
        description: description.unwrap_or_default(),
        is_public: is_public.unwrap_or(true),
        version,
        changelog,
        checksum_sha256,
        github_username_claim,
        theme_json,
    })
}

async fn parse_publish_theme_version_upload(
    mut multipart: Multipart,
) -> Result<PublishThemeVersionUpload, ApiError> {
    let mut version: Option<String> = None;
    let mut changelog: Option<String> = None;
    let mut checksum_sha256: Option<String> = None;
    let mut created_by: Option<String> = None;
    let mut theme_json: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        let field_name = field.name().unwrap_or_default().to_string();

        match field_name.as_str() {
            "version" => {
                version = Some(parse_required_field(
                    field.text().await.map_err(multipart_error)?,
                    "version",
                )?);
            }
            "changelog" => {
                changelog = Some(field.text().await.map_err(multipart_error)?);
            }
            "checksumSha256" => {
                checksum_sha256 = Some(field.text().await.map_err(multipart_error)?);
            }
            "createdBy" => {
                created_by = Some(field.text().await.map_err(multipart_error)?);
            }
            "themeJson" => {
                theme_json = Some(field.text().await.map_err(multipart_error)?.into_bytes());
            }
            "themeFile" | "file" => {
                theme_json = Some(field.bytes().await.map_err(multipart_error)?.to_vec());
            }
            _ => {}
        }
    }

    let version = version.ok_or_else(|| ApiError::BadRequest("version is required".to_string()))?;
    let theme_json =
        theme_json.ok_or_else(|| ApiError::BadRequest("themeFile JSON is required".to_string()))?;

    Ok(PublishThemeVersionUpload {
        version,
        changelog,
        checksum_sha256,
        created_by,
        theme_json,
    })
}

fn validate_theme_json(schema: &JSONSchema, json_bytes: &[u8]) -> Result<(), ApiError> {
    let payload = serde_json::from_slice::<Value>(json_bytes)
        .map_err(|err| ApiError::BadRequest(format!("themeFile must be valid JSON: {err}")))?;

    if let Err(errors) = schema.validate(&payload) {
        let details = errors
            .take(3)
            .map(|err| err.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ApiError::BadRequest(format!(
            "theme JSON does not match theme.schema.json: {details}"
        )));
    }

    Ok(())
}

async fn upload_theme_to_s3(state: &AppState, key: &str, payload: &[u8]) -> Result<(), ApiError> {
    state
        .s3_client
        .put_object()
        .bucket(&state.storage.bucket)
        .key(key)
        .content_type("application/json")
        .body(ByteStream::from(payload.to_vec()))
        .send()
        .await
        .map_err(|err| ApiError::Storage(format!("failed to upload theme JSON to S3: {err}")))?;

    Ok(())
}

async fn attach_theme_file_url(state: &AppState, theme: &mut Theme) -> Result<(), ApiError> {
    theme.file_url = match &theme.file_key {
        Some(key) => Some(presign_file_url(state, key).await?),
        None => None,
    };
    Ok(())
}

async fn attach_version_file_url(
    state: &AppState,
    version: &mut ThemeVersion,
) -> Result<(), ApiError> {
    version.file_url = Some(presign_file_url(state, &version.file_key).await?);
    Ok(())
}

async fn presign_file_url(state: &AppState, key: &str) -> Result<String, ApiError> {
    let expires_in =
        PresigningConfig::expires_in(StdDuration::from_secs(state.storage.presign_ttl_seconds))
            .map_err(|err| {
                ApiError::Storage(format!("failed to configure presign expiration: {err}"))
            })?;

    let request = state
        .s3_client
        .get_object()
        .bucket(&state.storage.bucket)
        .key(key)
        .presigned(expires_in)
        .await
        .map_err(|err| ApiError::Storage(format!("failed to presign S3 URL: {err}")))?;

    Ok(request.uri().to_string())
}

fn build_theme_file_key(prefix: &str, slug: &str, version: &str) -> String {
    let prefix = prefix.trim_matches('/');
    format!("{prefix}/{slug}/{version}/{DEFAULT_THEME_FILE_NAME}")
}

fn multipart_error(error: axum::extract::multipart::MultipartError) -> ApiError {
    ApiError::BadRequest(format!("invalid multipart payload: {error}"))
}

async fn require_auth_user(state: &AppState, headers: &HeaderMap) -> Result<AuthUser, ApiError> {
    let token = extract_auth_token(headers)
        .ok_or_else(|| ApiError::Unauthorized("authentication required".to_string()))?;
    let token_hash = hash_token(&token);

    let user = sqlx::query_as::<_, AuthUser>(
        r#"
        SELECT ua.id, ua.github_user_id, ua.github_login, ua.avatar_url, ua.name
        FROM user_session us
        JOIN user_account ua ON ua.id = us.user_id
        WHERE us.token_hash = $1 AND us.expires_at > NOW()
        "#,
    )
    .bind(token_hash)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::Unauthorized("invalid or expired session".to_string()))?;

    Ok(user)
}

async fn ensure_can_read_theme(
    theme: &Theme,
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    if theme.is_public {
        return Ok(());
    }

    let user = require_auth_user(state, headers).await?;
    ensure_theme_owner(theme, &user)
}

fn ensure_theme_owner(theme: &Theme, user: &AuthUser) -> Result<(), ApiError> {
    let owner_matches = if let Some(github_user_id_claim) = theme.github_user_id_claim {
        github_user_id_claim == user.github_user_id
    } else {
        theme
            .github_username_claim
            .eq_ignore_ascii_case(&user.github_login)
    };

    if owner_matches {
        Ok(())
    } else {
        Err(ApiError::Forbidden("you do not own this theme".to_string()))
    }
}

fn parse_required_field(value: String, field_name: &str) -> Result<String, ApiError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "{field_name} is required and cannot be empty"
        )));
    }
    Ok(trimmed.to_string())
}

fn parse_bool_field(value: &str, field_name: &str) -> Result<bool, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(ApiError::BadRequest(format!(
            "{field_name} must be one of: true/false/1/0/yes/no/on/off"
        ))),
    }
}

fn validate_slug(slug: &str) -> Result<(), ApiError> {
    let slug = slug.trim();
    let is_valid = !slug.is_empty()
        && slug.len() <= 64
        && slug
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        && !slug.starts_with('-')
        && !slug.ends_with('-');

    if !is_valid {
        return Err(ApiError::BadRequest(
            "slug must be lowercase alphanumeric plus hyphens and cannot start/end with '-'"
                .to_string(),
        ));
    }

    Ok(())
}

fn parse_semver_version(value: &str, field_name: &str) -> Result<String, ApiError> {
    let trimmed = value.trim();
    let parsed = Version::parse(trimmed).map_err(|err| {
        ApiError::BadRequest(format!(
            "{field_name} must be a valid semantic version (e.g. 1.2.3): {err}"
        ))
    })?;
    Ok(parsed.to_string())
}

fn slugify_name(value: &str) -> Option<String> {
    let mut slug = String::with_capacity(value.len());
    let mut previous_was_dash = false;

    for ch in value.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else {
            None
        };

        if let Some(character) = mapped {
            slug.push(character);
            previous_was_dash = false;
        } else if !slug.is_empty() && !previous_was_dash {
            slug.push('-');
            previous_was_dash = true;
        }
    }

    slug = slug.chars().take(64).collect::<String>();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() { None } else { Some(slug) }
}

async fn generate_unique_slug(pool: &PgPool, theme_name: &str) -> Result<String, ApiError> {
    let base = slugify_name(theme_name).ok_or_else(|| {
        ApiError::BadRequest(
            "name must contain at least one ASCII letter or number to derive a slug".to_string(),
        )
    })?;

    for suffix_number in 0..10_000_u32 {
        let candidate = if suffix_number == 0 {
            base.clone()
        } else {
            let suffix = format!("-{}", suffix_number + 1);
            let max_base_len = 64usize.saturating_sub(suffix.len());
            let trimmed_base = base
                .trim_end_matches('-')
                .chars()
                .take(max_base_len)
                .collect::<String>();
            format!("{trimmed_base}{suffix}")
        };

        validate_slug(&candidate)?;

        let exists = sqlx::query_scalar::<_, Option<Uuid>>("SELECT id FROM theme WHERE slug = $1")
            .bind(&candidate)
            .fetch_optional(pool)
            .await?
            .flatten()
            .is_some();

        if !exists {
            return Ok(candidate);
        }
    }

    Err(ApiError::Conflict(
        "could not derive a unique slug from theme name".to_string(),
    ))
}

fn parse_bool_env(value: &str, key: &str) -> anyhow::Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(anyhow::anyhow!(
            "{key} must be one of: true/false/1/0/yes/no/on/off"
        )),
    }
}

fn parse_i64_env(value: &str, key: &str) -> anyhow::Result<i64> {
    let parsed = value
        .parse::<i64>()
        .map_err(|_| anyhow::anyhow!("{key} must be a valid integer"))?;
    if parsed <= 0 {
        return Err(anyhow::anyhow!("{key} must be > 0"));
    }
    Ok(parsed)
}

fn parse_u64_env(value: &str, key: &str) -> anyhow::Result<u64> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| anyhow::anyhow!("{key} must be a valid unsigned integer"))?;
    if parsed == 0 {
        return Err(anyhow::anyhow!("{key} must be > 0"));
    }
    Ok(parsed)
}

fn extract_cookie(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
    for cookie_header in headers.get_all(header::COOKIE) {
        let cookie_value = match cookie_header.to_str() {
            Ok(value) => value,
            Err(_) => continue,
        };
        for part in cookie_value.split(';') {
            let mut parts = part.trim().splitn(2, '=');
            let Some(name) = parts.next() else {
                continue;
            };
            let Some(value) = parts.next() else {
                continue;
            };
            if name == cookie_name {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn extract_auth_token(headers: &HeaderMap) -> Option<String> {
    extract_bearer_token(headers).or_else(|| extract_cookie(headers, SESSION_COOKIE_NAME))
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let header_value = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    let token = header_value.strip_prefix("Bearer ")?.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn build_session_cookie(token: &str, ttl_hours: i64, secure: bool, domain: Option<&str>) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    let domain_flag = domain
        .map(|value| format!("; Domain={value}"))
        .unwrap_or_default();
    format!(
        "{name}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{domain}{secure}",
        name = SESSION_COOKIE_NAME,
        max_age = ttl_hours * 3600,
        domain = domain_flag,
        secure = secure_flag,
    )
}

fn clear_session_cookie(secure: bool, domain: Option<&str>) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    let domain_flag = domain
        .map(|value| format!("; Domain={value}"))
        .unwrap_or_default();
    format!(
        "{name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{domain}{secure}",
        name = SESSION_COOKIE_NAME,
        domain = domain_flag,
        secure = secure_flag,
    )
}

fn is_termy_deeplink(target: &str) -> bool {
    Url::parse(target)
        .map(|url| url.scheme() == "termy")
        .unwrap_or(false)
}

fn build_native_auth_redirect_target(
    target: &str,
    session_token: &str,
    user: &AuthUser,
) -> Result<String, url::ParseError> {
    let mut url = Url::parse(target)?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("session_token", session_token);
        pairs.append_pair("id", &user.id.to_string());
        pairs.append_pair("github_user_id", &user.github_user_id.to_string());
        pairs.append_pair("github_login", &user.github_login);
        if let Some(avatar_url) = &user.avatar_url {
            pairs.append_pair("avatar_url", avatar_url);
        }
        if let Some(name) = &user.name {
            pairs.append_pair("name", name);
        }
    }
    Ok(url.to_string())
}

fn build_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::mirror_request())
        .allow_credentials(true)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PATCH,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([header::ACCEPT, header::AUTHORIZATION, header::CONTENT_TYPE])
}

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    hex::encode(digest)
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .as_deref()
        == Some("23505")
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Debug, Error)]
enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    ExternalAuth(String),
    #[error("{0}")]
    Storage(String),
    #[error("database error")]
    Database(#[from] sqlx::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::ExternalAuth(_) => StatusCode::BAD_GATEWAY,
            Self::Storage(_) => StatusCode::BAD_GATEWAY,
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let body = match self {
            Self::Database(error) => {
                error!(?error, "database request failed");
                ErrorBody {
                    error: "internal server error".to_string(),
                }
            }
            other => ErrorBody {
                error: other.to_string(),
            },
        };

        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_semver_version, slugify_name, validate_slug};

    #[test]
    fn accepts_valid_slug() {
        assert!(validate_slug("tokyo-night").is_ok());
    }

    #[test]
    fn rejects_invalid_slug() {
        assert!(validate_slug("Tokyo-Night").is_err());
        assert!(validate_slug("-tokyo-night").is_err());
        assert!(validate_slug("tokyo-night-").is_err());
        assert!(validate_slug("tokyo_night").is_err());
    }

    #[test]
    fn slugifies_theme_names() {
        assert_eq!(slugify_name("Tokyo Night"), Some("tokyo-night".to_string()));
        assert_eq!(slugify_name("___Midnight___"), Some("midnight".to_string()));
        assert_eq!(slugify_name("!!!"), None);
    }

    #[test]
    fn validates_semver_versions() {
        assert_eq!(
            parse_semver_version("1.2.3", "version").expect("semver should parse"),
            "1.2.3"
        );
        assert!(parse_semver_version("v1.2.3", "version").is_err());
        assert!(parse_semver_version("1", "version").is_err());
    }
}
