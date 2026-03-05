# termy_theme_store

Theme store API for Termy, backed by PostgreSQL via SQLx.

## Run

```bash
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/termy_theme_store
export THEME_STORE_BIND=127.0.0.1:8080
export GITHUB_CLIENT_ID=...
export GITHUB_CLIENT_SECRET=...
export GITHUB_REDIRECT_URI=http://127.0.0.1:8080/auth/github/callback
export S3_BUCKET=termy-themes
export S3_REGION=eu-central-1
# optional for MinIO/localstack:
# export S3_ENDPOINT=http://127.0.0.1:9000
cargo run -p termy_theme_store
```

Migrations run automatically on startup.

## Routes

- `GET /health`
- `GET /auth/github/login`
- `GET /auth/github/callback`
- `GET /auth/me`
- `POST /auth/logout`
- `GET /themes`
- `POST /themes`
- `GET /themes/:slug`
- `PATCH /themes/:slug`
- `GET /themes/:slug/versions`
- `POST /themes/:slug/versions`

## Example requests

Log in with GitHub (browser redirect):

```bash
open "http://127.0.0.1:8080/auth/github/login"
```

Create theme:

```bash
curl -X POST http://127.0.0.1:8080/themes \
  -F 'name=Tokyo Night' \
  -F 'slug=tokyo-night' \
  -F 'description=Dark blue terminal palette' \
  -F 'version=1.0.0' \
  -F 'isPublic=true' \
  -F 'themeFile=@./tokyo-night.json;type=application/json'
```

Publish version:

```bash
curl -X POST http://127.0.0.1:8080/themes/tokyo-night/versions \
  -F 'version=1.1.0' \
  -F 'changelog=Update blue tones' \
  -F 'checksumSha256=...' \
  -F 'themeFile=@./tokyo-night-v1.1.0.json;type=application/json'
```
