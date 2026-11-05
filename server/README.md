# SillyTavern RS sync server

Optional server that keeps characters, chats, settings, persona avatars and (encrypted)
API keys in sync between your devices. The app stays local-first: it works offline and
syncs when the server is reachable. When the same item changed on two devices, the app
asks which copy to keep.

API keys are encrypted on the device with your passphrase (Argon2id + XChaCha20-Poly1305)
before upload; the server only stores ciphertext.

## Deploy on a VPS (Docker + automatic HTTPS)

Requirements: a Linux VPS with Docker, ports 80 and 443 free, and a domain or subdomain
with a DNS **A record** pointing at the VPS IP.

```bash
# Docker, if missing
curl -fsSL https://get.docker.com | sh

mkdir -p ~/sillytavern-rs && cd ~/sillytavern-rs
for f in docker-compose.yml Caddyfile .env.example; do
  curl -fsSLO "https://raw.githubusercontent.com/faizonly5953/sillytavern-on-rust/main/server/$f"
done
cp .env.example .env
nano .env                      # set DOMAIN=sync.yourdomain.com
docker compose up -d
docker compose logs sync       # shows "New access token ..." on first start
```

If you missed the token: `docker compose exec sync cat /data/token`.

In the app: **Settings → Cloud → Connect** (or the first-run screen), then enter
host `sync.yourdomain.com`, port `443`, HTTPS on, the access token, and a passphrase
for API keys. Use the same passphrase on every device.

### Automatic updates

Every production release publishes `ghcr.io/faizonly5953/sillytavern-rs-server:latest`.
The `updater` service checks for it every hour (`UPDATE_INTERVAL` in `.env`) and restarts
the server only when a new image arrived; data in the `sync-data` volume is kept.
Prereleases (`v1.2.0-beta.1`) are published under their own tag and never picked up.

Update right away: `docker compose pull sync && docker compose up -d sync`.
Turn auto-update off: `docker compose stop updater`.

## Without Docker

```bash
cargo build --release
ST_SYNC_DATA=./data ST_SYNC_ADDR=127.0.0.1:8787 ./target/release/sillytavern-rs-server
```

Put it behind any HTTPS reverse proxy (Caddy, nginx). Plain HTTP works but sends your
token and chats unencrypted; only use it inside a private network or VPN.

## Backup

Everything lives in the `sync-data` Docker volume (`/data/sync.db` and `/data/token`):

```bash
docker compose exec sync sh -c 'cat /data/sync.db' > sync-backup.db
```

## API (v1)

All routes except `health` need `Authorization: Bearer <token>`.

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/v1/health` | liveness |
| GET | `/api/v1/info` | `server_id`, `latest_seq` |
| GET | `/api/v1/changes?since=N` | metadata of items changed after sequence N |
| GET | `/api/v1/item?kind=&id=` | item bytes (`x-rev`, `x-hash`, `x-deleted` headers) |
| PUT | `/api/v1/item?kind=&id=&base_rev=&hash=` | write; 409 + current metadata if `base_rev` is stale |
| DELETE | `/api/v1/item?kind=&id=&base_rev=` | tombstone; same conflict rule |

Kinds: `character`, `chat`, `settings`, `persona_avatar`, `secrets`.
