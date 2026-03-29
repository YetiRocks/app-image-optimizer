<p align="center">
  <img src="https://cdn.prod.website-files.com/68e09cef90d613c94c3671c0/697e805a9246c7e090054706_logo_horizontal_grey.png" alt="Yeti" width="200" />
</p>

---

# app-image-optimizer

[![Yeti](https://img.shields.io/badge/Yeti-Application-blue)](https://yetirocks.com)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

> **[Yeti](https://yetirocks.com)** - The Performance Platform for Agent-Driven Development.
> Schema-driven APIs, real-time streaming, and vector search. From prompt to production.

Image upload, storage, and variant serving with cache-key routing and format negotiation.

## Features

- **Upload and replace** images up to 10MB with automatic ID generation
- **Variant caching** with composite cache keys (`{imageId}_{width}_{dpr}_{format}`) and 1-hour TTL
- **Cache-key routing** -- cache hits served directly with `X-Cache: HIT` header
- **Automatic variant purge** when the source image is updated
- **Format negotiation** -- request WebP, JPEG, PNG, AVIF, or original format
- **DPR-aware** variant keys for retina/high-DPI displays
- **Content type validation** for 8 image formats (JPEG, PNG, WebP, AVIF, GIF, SVG, BMP, TIFF)

> **Note:** Dynamic image processing (resize/format conversion) requires the future yeti-media extension. Currently, variants store the original image data with the requested format metadata, suitable for CDN-side transformation or client-side processing.

## Installation

```bash
git clone https://github.com/yetirocks/app-image-optimizer.git
cp -r app-image-optimizer ~/yeti/applications/
```

## Project Structure

```
app-image-optimizer/
  config.yaml
  schemas/
    schema.graphql
  resources/
    upload.rs       # Image upload and replacement with variant purge
    variant.rs      # Variant serving with cache-key routing
```

## Configuration

```yaml
name: "Image Optimizer"
app_id: "app-image-optimizer"
version: "0.1.0"
description: "Image upload, storage, and variant serving with cache key routing and format negotiation"

schemas:
  - schemas/schema.graphql

resources:
  - resources/*.rs

auth:
  methods: [jwt, basic]
```

## Schema

**Image** -- Source images stored as base64-encoded data with content type, dimensions, and metadata (EXIF, alt text, attribution).

**ImageVariant** -- Cached variant records with 1-hour TTL. The primary key is a composite cache key encoding the image ID, width, DPR, and format. Public read access allows unauthenticated variant serving.

```graphql
type Image @table(database: "app-image-optimizer") @export {
    id: ID! @primaryKey
    filename: String
    contentType: String!
    width: Int
    height: Int
    sizeBytes: Int
    createdAt: String!
    updatedAt: String
    metadata: String              # JSON: EXIF data, alt text, attribution
    data: String!                 # base64-encoded image data
}

type ImageVariant @table(expiration: 3600, database: "app-image-optimizer")
    @export(public: [read]) {
    id: ID! @primaryKey           # cache key: {imageId}_{width}_{dpr}_{format}
    imageId: String! @indexed
    format: String! @indexed      # "webp", "jpeg", "png", "avif", "original"
    width: String!                # requested width or "orig"
    dpr: String!                  # device pixel ratio
    contentType: String!
    sizeBytes: Int
    createdAt: String!
    data: String!                 # base64-encoded variant data
}
```

## API Reference

### POST /app-image-optimizer/upload

Upload a new image. Body is JSON with base64-encoded image data.

```bash
curl -X POST https://localhost:9996/app-image-optimizer/upload \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "filename": "hero.jpg",
    "contentType": "image/jpeg",
    "data": "'$(base64 < hero.jpg)'"
  }'

# Response
# 201 { "id": "img-1711700000-a1b2c3d4", "filename": "hero.jpg",
#        "contentType": "image/jpeg", "sizeBytes": 245760 }
```

### PUT /app-image-optimizer/upload?id={imageId}

Replace an existing image. All cached variants for this image are automatically purged.

```bash
curl -X PUT "https://localhost:9996/app-image-optimizer/upload?id=img-1711700000-a1b2c3d4" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "filename": "hero-v2.jpg",
    "contentType": "image/jpeg",
    "data": "'$(base64 < hero-v2.jpg)'"
  }'

# Response
# 200 { "id": "img-...", "contentType": "image/jpeg", "sizeBytes": 312000, "variantsPurged": 3 }
```

### GET /app-image-optimizer/variant?id={imageId}&width={w}&format={fmt}&dpr={dpr}

Serve a variant. Returns a cache hit if available, otherwise creates the variant record.

| Parameter | Required | Default | Values |
|-----------|----------|---------|--------|
| `id` | Yes | -- | Image ID |
| `width` | No | `orig` | Integer or `orig` |
| `format` | No | `original` | `webp`, `jpeg`, `png`, `avif`, `original` |
| `dpr` | No | `1` | Device pixel ratio (e.g., `2`) |

```bash
# Request a 400px-wide WebP variant at 2x DPR
curl "https://localhost:9996/app-image-optimizer/variant?id=img-1711700000-a1b2c3d4&width=400&format=webp&dpr=2"

# Response headers:
#   X-Cache: MISS (or HIT on subsequent requests)
#   X-Variant-Key: img-1711700000-a1b2c3d4_400_2.0_webp
#   Cache-Control: public, max-age=3600
#   Content-Type: image/webp
```

### DELETE /app-image-optimizer/variant?id={imageId}

Purge all cached variants for an image.

```bash
curl -X DELETE "https://localhost:9996/app-image-optimizer/variant?id=img-1711700000-a1b2c3d4" \
  -H "Authorization: Bearer $TOKEN"

# Response
# 200 { "imageId": "img-...", "variantsPurged": 5 }
```

---

Built with [Yeti](https://yetirocks.com) | The Performance Platform for Agent-Driven Development
