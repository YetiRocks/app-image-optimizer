<p align="center">
  <img src="https://cdn.prod.website-files.com/68e09cef90d613c94c3671c0/697e805a9246c7e090054706_logo_horizontal_grey.png" alt="Yeti" width="200" />
</p>

---

# app-image-optimizer

[![Yeti](https://img.shields.io/badge/Yeti-Application-blue)](https://yetirocks.com)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

> **[Yeti](https://yetirocks.com)** - The Performance Platform for Agent-Driven Development.
> Schema-driven APIs, real-time streaming, and vector search. From prompt to production.

**Image upload, storage, and variant serving with cache-key routing.** One application, zero infrastructure.

Serving images at scale means resizing, format conversion, caching, and purging -- usually spread across separate services (an object store, a CDN, an image processing pipeline, a cache layer). app-image-optimizer collapses all of that into a single yeti application: upload once, request any variant by width, format, and DPR, and let composite cache keys handle the rest.

---

## Why Image Optimizer

Every image pipeline starts simple and grows into a distributed system. You need an upload endpoint, a storage layer, a resize service, a format converter, a cache, a CDN invalidation hook, and a way to tie them all together. Each piece has its own deployment, its own config, its own failure mode.

Image Optimizer collapses that into a single yeti application:

- **Upload once, serve many** -- store a source image and request any combination of width, format, and DPR. Each combination gets its own cache entry with a composite key.
- **Composite cache keys** -- variant keys encode `{imageId}_{width}_{dpr}_{format}`, so every unique combination is a single lookup. Cache hits return instantly with `X-Cache: HIT`.
- **Automatic variant purge** -- replacing a source image purges all cached variants. No stale data, no manual invalidation.
- **Format negotiation** -- request WebP, JPEG, PNG, AVIF, or the original format. The variant record stores the target content type for downstream CDN or client-side transformation.
- **DPR-aware** -- variant keys include device pixel ratio, so retina and standard displays get separate cache entries without URL hacks.
- **TTL expiration** -- ImageVariant records auto-expire after 1 hour via schema-level `expiration: 3600`. No cron jobs, no sweep tasks.
- **Public read access** -- variants are served without authentication via `@export(public: [read])`. Upload and replace require auth.
- **Single binary deployment** -- compiles into a native Rust plugin. No Node.js, no npm, no Docker compose. Loads with yeti in seconds.

---

## Quick Start

### 1. Install

```bash
cd ~/yeti/applications
git clone https://github.com/yetirocks/app-image-optimizer.git
```

Restart yeti. The application compiles automatically on first load (~2 minutes) and is cached for subsequent starts (~10 seconds).

### 2. Upload an image

```bash
curl -X POST https://localhost/app-image-optimizer/api/upload \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "filename": "hero.jpg",
    "contentType": "image/jpeg",
    "data": "'$(base64 < hero.jpg)'"
  }'
```

Response:
```json
{
  "id": "img-1743292800-a1b2c3d4e5f67890",
  "filename": "hero.jpg",
  "contentType": "image/jpeg",
  "sizeBytes": 245760
}
```

### 3. Fetch a variant

```bash
curl "https://localhost/app-image-optimizer/api/variant?id=img-1743292800-a1b2c3d4e5f67890&width=400&format=webp&dpr=2"
```

Response headers (first request):
```
HTTP/2 200
x-cache: MISS
x-variant-key: img-1743292800-a1b2c3d4e5f67890_400_2.0_webp
cache-control: public, max-age=3600
content-type: image/webp
```

### 4. Check cache status

```bash
# Same request again — now served from cache
curl -I "https://localhost/app-image-optimizer/api/variant?id=img-1743292800-a1b2c3d4e5f67890&width=400&format=webp&dpr=2"
```

Response headers (subsequent requests):
```
HTTP/2 200
x-cache: HIT
x-variant-key: img-1743292800-a1b2c3d4e5f67890_400_2.0_webp
cache-control: public, max-age=3600
content-type: image/webp
```

---

## Architecture

```
Client (browser, CDN, agent)
    |
    +-- POST /upload -----------> Upload resource
    |                                |
    |                                v
    |                           Image table (RocksDB)
    |                                |
    +-- GET /variant?... -------> Variant resource
    |                                |
    |                           cache key = {id}_{width}_{dpr}_{format}
    |                                |
    |                          +-----+-----+
    |                          |           |
    |                        HIT         MISS
    |                          |           |
    |                          |     load Image → create variant
    |                          |           |
    |                          v           v
    |                     ImageVariant table (TTL 1hr)
    |                          |
    |                          v
    |                     response + X-Cache header
    |
    +-- PUT /upload?id= -------> Replace image → purge all variants
    |
    +-- DELETE /variant?id= ----> Purge variants for image
```

**Upload path:** Client request -> content type validation (8 formats) -> size check (10MB limit) -> generate ID (`img-{timestamp}-{hash}`) -> store in Image table -> return metadata.

**Variant path:** Client request -> build composite cache key -> lookup in ImageVariant table -> HIT: return cached data with `X-Cache: HIT` -> MISS: load source Image, map format to content type, store variant record, return with `X-Cache: MISS`.

**Replace path:** Client request -> verify image exists -> overwrite Image record -> scan ImageVariant table for matching `imageId` -> delete all matches -> return purge count.

---

## Features

### Image Upload (POST /app-image-optimizer/api/upload)

Upload a new image with base64-encoded data:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `data` | String | Yes | Base64-encoded image data (max ~10MB) |
| `contentType` | String | Yes | MIME type (must be a supported image format) |
| `filename` | String | No | Original filename (defaults to "untitled") |
| `metadata` | String (JSON) | No | Arbitrary metadata: EXIF, alt text, attribution |

Returns `201` with the generated ID, filename, content type, and approximate decoded size in bytes.

**ID generation:** `img-{unix_timestamp}-{first 8 chars of djb2 hash}`. The hash is computed over the first 1024 bytes of the base64 data for fast collision avoidance.

**Size limit:** Base64 data must not exceed 13,333,333 characters (~10MB decoded). Requests exceeding this return `400`.

### Image Replace (PUT /app-image-optimizer/api/upload?id={imageId})

Replace an existing image. The source record is overwritten and all cached variants for that image are automatically purged:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | Query param | Yes | Image ID to replace |
| `data` | String | Yes | New base64-encoded image data |
| `contentType` | String | Yes | New MIME type |
| `filename` | String | No | Updated filename |
| `metadata` | String (JSON) | No | Updated metadata |

Returns `200` with the image ID, content type, size, and `variantsPurged` count. Returns `404` if the image does not exist.

**Variant purge:** Scans all ImageVariant records and deletes those with a matching `imageId`. This ensures no stale variants are served after a source image update.

### Variant Serving (GET /app-image-optimizer/api/variant)

Serve an image variant by composite cache key. Cache hits are returned directly; cache misses create a new variant record:

| Parameter | Required | Default | Values |
|-----------|----------|---------|--------|
| `id` | Yes | -- | Image ID |
| `width` | No | `orig` | Positive integer or `orig` |
| `format` | No | `original` | `webp`, `jpeg`, `png`, `avif`, `original` |
| `dpr` | No | `1` | Device pixel ratio (numeric, e.g., `1`, `1.5`, `2`) |

**Cache key format:** `{imageId}_{width}_{dpr:.1}_{format}`

Examples:
- `img-123_orig_1.0_original` -- original size, original format, 1x DPR
- `img-123_400_2.0_webp` -- 400px wide, WebP, 2x DPR
- `img-123_800_1.5_avif` -- 800px wide, AVIF, 1.5x DPR

**Response headers:**

| Header | Value | Description |
|--------|-------|-------------|
| `X-Cache` | `HIT` or `MISS` | Whether the variant was served from cache |
| `X-Variant-Key` | Cache key string | The composite key used for this variant |
| `Cache-Control` | `public, max-age=3600` | 1-hour browser/CDN cache directive |
| `Content-Type` | Target MIME type | Mapped from requested format |

**Format mapping:**

| Requested format | Content-Type header |
|------------------|---------------------|
| `webp` | `image/webp` |
| `jpeg` | `image/jpeg` |
| `png` | `image/png` |
| `avif` | `image/avif` |
| `original` | Source image's content type |

### Variant Purge (DELETE /app-image-optimizer/api/variant?id={imageId})

Purge all cached variants for a specific image:

```bash
curl -X DELETE "https://localhost/app-image-optimizer/api/variant?id=img-1743292800-a1b2c3d4e5f67890" \
  -H "Authorization: Bearer $TOKEN"
```

Response:
```json
{
  "imageId": "img-1743292800-a1b2c3d4e5f67890",
  "variantsPurged": 5
}
```

### Supported Formats

| Format | MIME Type | Upload | Variant Request |
|--------|-----------|--------|-----------------|
| JPEG | `image/jpeg` | Yes | Yes |
| PNG | `image/png` | Yes | Yes |
| WebP | `image/webp` | Yes | Yes |
| AVIF | `image/avif` | Yes | Yes |
| GIF | `image/gif` | Yes | No |
| SVG | `image/svg+xml` | Yes | No |
| BMP | `image/bmp` | Yes | No |
| TIFF | `image/tiff` | Yes | No |

Upload accepts all 8 formats. Variant requests support the 4 web-optimized formats (`webp`, `jpeg`, `png`, `avif`) plus `original` which preserves the source format.

### REST CRUD (auto-generated)

Full CRUD on all tables is auto-generated from the schema:

| Endpoint | Methods | Description |
|----------|---------|-------------|
| `/app-image-optimizer/api/Image` | GET, POST | List/create images |
| `/app-image-optimizer/api/Image/{id}` | GET, PUT, DELETE | Read/update/delete an image |
| `/app-image-optimizer/api/ImageVariant` | GET, POST | List/create variants |
| `/app-image-optimizer/api/ImageVariant/{id}` | GET, PUT, DELETE | Read/update/delete a variant |

### Real-Time Streaming (auto-generated)

Real-time updates are built into the platform via `@export`:

```bash
# SSE -- server-sent events
GET /app-image-optimizer/api/Image?stream=sse
GET /app-image-optimizer/api/ImageVariant?stream=sse

# MQTT -- subscribe to changes
mosquitto_sub -t "app-image-optimizer/Image" -h localhost -p 8883
mosquitto_sub -t "app-image-optimizer/ImageVariant" -h localhost -p 8883
```

### MCP Tools (auto-generated)

MCP tools for table operations are auto-generated from `@export` schemas. Any MCP-compatible agent (Claude Code, Cursor, Windsurf) can discover and use them via the standard MCP protocol at `POST /app-image-optimizer/api/mcp`.

---

## Data Model

### Image Table

| Field | Type | Indexed | Description |
|-------|------|---------|-------------|
| `id` | ID! | Primary key | Auto-generated: `img-{timestamp}-{hash}` |
| `filename` | String | -- | Original filename |
| `contentType` | String! | -- | MIME type (validated on upload) |
| `width` | Int | -- | Image width in pixels |
| `height` | Int | -- | Image height in pixels |
| `sizeBytes` | Int | -- | Approximate decoded size in bytes |
| `createdAt` | String! | -- | Unix timestamp at creation |
| `updatedAt` | String | -- | Unix timestamp at last replacement |
| `metadata` | String | -- | JSON: EXIF data, alt text, attribution |
| `data` | String! | -- | Base64-encoded image data |

### ImageVariant Table

| Field | Type | Indexed | Description |
|-------|------|---------|-------------|
| `id` | ID! | Primary key | Composite cache key: `{imageId}_{width}_{dpr}_{format}` |
| `imageId` | String! | Yes | Reference to source Image |
| `format` | String! | Yes | Requested format: `webp`, `jpeg`, `png`, `avif`, `original` |
| `width` | String! | -- | Requested width or `orig` |
| `dpr` | String! | -- | Device pixel ratio (e.g., `1.0`, `2.0`) |
| `contentType` | String! | -- | Resolved MIME type for this variant |
| `sizeBytes` | Int | -- | Approximate decoded size in bytes |
| `createdAt` | String! | -- | Unix timestamp at creation |
| `data` | String! | -- | Base64-encoded variant data |

**TTL:** ImageVariant records expire automatically after 3600 seconds (1 hour) via schema-level `@table(expiration: 3600)`. No manual cleanup required.

**Public access:** ImageVariant has `@export(public: [read])`, allowing unauthenticated GET requests for variant serving. This enables CDN edge nodes and anonymous browsers to fetch variants without credentials.

---

## Configuration

```yaml
name: "Image Optimizer"
app_id: "app-image-optimizer"
version: "0.1.0"
description: "Image upload, storage, and variant serving with cache key routing and format negotiation"

schemas:
  path: schemas/schema.graphql

resources:
  path: resources/*.rs
  route: /api

auth:
  methods: [jwt, basic]
```

### Key settings

| Field | Value | Notes |
|-------|-------|-------|
| `app_id` | `app-image-optimizer` | URL prefix for all endpoints |
| `schemas` | `schemas/schema.graphql` | Defines Image and ImageVariant tables |
| `resources` | `resources/*.rs` | Upload and Variant custom resources |
| `auth.methods` | `[jwt, basic]` | Authentication methods for write operations |

---

## Project Structure

```
app-image-optimizer/
  config.yaml              # App configuration
  schemas/
    schema.graphql         # Image and ImageVariant tables
  resources/
    upload.rs              # Image upload (POST) and replacement (PUT) with variant purge
    variant.rs             # Variant serving (GET) with cache-key routing and purge (DELETE)
```

---

## Authentication

Image Optimizer uses yeti's built-in auth system. In development mode, all endpoints are accessible without authentication. In production:

- **JWT** and **Basic Auth** supported (configured in config.yaml)
- **ImageVariant** table allows public `read` access via `@export(public: [read])` -- variants can be served without authentication
- **Image** table requires authentication for all operations (upload, replace, delete, list)
- **Upload** (POST) and **Replace** (PUT) require authentication
- **Variant purge** (DELETE) requires authentication
- **Variant serving** (GET) is public -- no token needed

This split allows CDN edge servers and browsers to fetch optimized variants directly while keeping write operations protected.

---

## Future: yeti-media Extension

> **Note:** Dynamic image processing (resize and format conversion) requires the future `yeti-media` extension. Currently, variant records store the original image data with the requested format metadata. This is suitable for two deployment patterns:
>
> 1. **CDN-side transformation** -- serve variants to a CDN (Cloudflare, Fastly, CloudFront) that performs resize/conversion at the edge based on the `Content-Type` and requested dimensions.
> 2. **Client-side processing** -- use the variant metadata (width, format, DPR) in a frontend image component that handles scaling and format selection.
>
> When `yeti-media` ships, it will hook into the variant creation path to perform actual resize and format conversion before caching. Existing cache keys and API contracts will remain unchanged.

---

## Comparison

| | app-image-optimizer | Typical Image Pipeline |
|---|---|---|
| **Deployment** | Loads with yeti, zero config | S3 + Lambda + CloudFront + cache invalidation |
| **Upload** | Single JSON endpoint | Multipart upload to object store |
| **Variants** | Composite cache key, auto-generated | Separate resize service or CDN config |
| **Cache** | Built-in TTL expiration (1hr) | Redis/Memcached + manual TTL management |
| **Purge** | Automatic on image replace | Manual CDN invalidation API calls |
| **Format support** | 8 upload formats, 4 variant formats | Per-service format configuration |
| **Auth** | Built-in JWT/Basic, public reads | Custom auth per service |
| **Real-time** | Native SSE + MQTT from schema | Custom webhooks or polling |
| **Binary** | Compiles to native Rust plugin | Node.js runtime + sharp/imagemagick |
| **DPR support** | First-class cache key dimension | URL parameter or client hints |

---

Built with [Yeti](https://yetirocks.com) | The Performance Platform for Agent-Driven Development
