use yeti_sdk::prelude::*;

// Serve image variants with cache-key routing.
//
// GET /app-image-optimizer/variant?id=img-123&width=400&format=webp&dpr=2
//   Returns cached variant or generates on first request.
//   Cache key: {imageId}_{width}_{dpr}_{format}
//
// DELETE /app-image-optimizer/variant?id=img-123
//   Purge all cached variants for an image.
//
// Note: Dynamic image processing (resize/format conversion) requires the
// future yeti-media extension. Currently serves original data with format
// metadata for client-side processing or CDN transformation.
resource!(Variant {
    name = "variant",
    get(ctx) => {
        let image_id = match ctx.query("id") {
            Some(id) => id.to_string(),
            None => return bad_request("missing ?id= parameter"),
        };

        let width = ctx.query("width").unwrap_or("orig").to_string();
        let format = ctx.query("format").unwrap_or("original").to_string();
        let dpr = ctx.query("dpr").unwrap_or("1").to_string();

        // Validate parameters
        if width != "orig" {
            if width.parse::<u32>().is_err() {
                return bad_request("width must be a positive integer or 'orig'");
            }
        }
        if !["webp", "jpeg", "png", "avif", "original"].contains(&format.as_str()) {
            return bad_request("format must be webp, jpeg, png, avif, or original");
        }
        if dpr.parse::<f32>().is_err() {
            return bad_request("dpr must be a number");
        }

        let cache_key = format!("{}_{}_{:.1}_{}", image_id, width, dpr.parse::<f32>().unwrap_or(1.0), format);
        let variant_table = ctx.get_table("ImageVariant")?;

        // Cache hit
        if let Some(variant) = variant_table.get(&cache_key).await? {
            let ct = variant["contentType"].as_str().unwrap_or("image/jpeg");
            let data = variant["data"].as_str().unwrap_or("");
            // Decode base64 and serve
            return reply()
                .header("x-cache", "HIT")
                .header("x-variant-key", &cache_key)
                .header("cache-control", "public, max-age=3600")
                .type_header(ct)
                .send(data.as_bytes().to_vec());
        }

        // Cache miss — load original
        let image_table = ctx.get_table("Image")?;
        let image = match image_table.get(&image_id).await? {
            Some(img) => img,
            None => return not_found(&format!("image {} not found", image_id)),
        };

        let original_ct = image["contentType"].as_str().unwrap_or("image/jpeg");
        let data = image["data"].as_str().unwrap_or("");

        // Map requested format to content type
        let target_ct = match format.as_str() {
            "webp" => "image/webp",
            "jpeg" => "image/jpeg",
            "png" => "image/png",
            "avif" => "image/avif",
            _ => original_ct,
        };

        // Store variant (currently serves original data — resize/conversion
        // will be handled by yeti-media extension when available)
        let now = unix_timestamp()?.to_string();
        let variant_record = json!({
            "id": cache_key,
            "imageId": image_id,
            "format": format,
            "width": width,
            "dpr": dpr,
            "contentType": target_ct,
            "sizeBytes": data.len() * 3 / 4,
            "createdAt": now,
            "data": data,
        });
        variant_table.put(&cache_key, variant_record).await?;

        reply()
            .header("x-cache", "MISS")
            .header("x-variant-key", &cache_key)
            .header("cache-control", "public, max-age=3600")
            .type_header(target_ct)
            .send(data.as_bytes().to_vec())
    },
    delete(ctx) => {
        let image_id = match ctx.query("id") {
            Some(id) => id.to_string(),
            None => return bad_request("missing ?id= parameter"),
        };

        let variant_table = ctx.get_table("ImageVariant")?;
        let all: Vec<Value> = variant_table.get_all().await?;
        let mut purged = 0u32;

        for v in &all {
            if v["imageId"].as_str() == Some(&image_id) {
                if let Some(vid) = v["id"].as_str() {
                    let _ = variant_table.delete(vid).await;
                    purged += 1;
                }
            }
        }

        ok(json!({
            "imageId": image_id,
            "variantsPurged": purged
        }))
    }
});
