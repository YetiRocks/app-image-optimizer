use yeti_sdk::prelude::*;

// Upload an image.
//
// POST /app-image-optimizer/upload
//   Body (JSON): { "filename": "photo.jpg", "contentType": "image/jpeg",
//                  "data": "<base64-encoded>", "metadata": "{}" }
//
// PUT /app-image-optimizer/upload?id=img-123
//   Replace existing image and purge all cached variants.
//
// Returns: { "id": "img-...", "contentType": "...", "sizeBytes": N }
resource!(Upload {
    name = "upload",
    post(ctx) => {
        let body: Value = ctx.require_json_body()?.clone();

        let data = body["data"].as_str()
            .ok_or_else(|| YetiError::Validation("missing required field: data (base64)".into()))?;
        let content_type = body["contentType"].as_str()
            .ok_or_else(|| YetiError::Validation("missing required field: contentType".into()))?;

        // Validate content type
        if !is_image_type(content_type) {
            return bad_request(&format!("unsupported content type: {}", content_type));
        }

        // Size check: base64 is ~33% overhead, so 10MB base64 ≈ 7.5MB raw
        if data.len() > 13_333_333 {
            return bad_request("image exceeds 10MB limit");
        }

        let image_table = ctx.get_table("Image")?;
        let now = unix_timestamp()?.to_string();
        let size_bytes = data.len() * 3 / 4; // approximate decoded size

        let id = format!("img-{}-{}", now, &hash(data)[..8]);
        let filename = body["filename"].as_str().unwrap_or("untitled");
        let metadata = body["metadata"].as_str().unwrap_or("{}");

        let record = json!({
            "id": id,
            "filename": filename,
            "contentType": content_type,
            "sizeBytes": size_bytes,
            "createdAt": now,
            "metadata": metadata,
            "data": data,
        });

        image_table.put(&id, record).await?;

        reply().code(201).json(json!({
            "id": id,
            "filename": filename,
            "contentType": content_type,
            "sizeBytes": size_bytes
        }))
    },
    put(ctx) => {
        let id = match ctx.query("id") {
            Some(id) => id.to_string(),
            None => return bad_request("missing ?id= parameter"),
        };

        let body: Value = ctx.require_json_body()?.clone();
        let data = body["data"].as_str()
            .ok_or_else(|| YetiError::Validation("missing required field: data (base64)".into()))?;
        let content_type = body["contentType"].as_str()
            .ok_or_else(|| YetiError::Validation("missing required field: contentType".into()))?;

        if !is_image_type(content_type) {
            return bad_request(&format!("unsupported content type: {}", content_type));
        }

        let image_table = ctx.get_table("Image")?;
        let variant_table = ctx.get_table("ImageVariant")?;

        if !image_table.does_exist(&id).await? {
            return not_found(&format!("image {} not found", id));
        }

        let now = unix_timestamp()?.to_string();
        let size_bytes = data.len() * 3 / 4;

        let record = json!({
            "id": id,
            "filename": body["filename"].as_str().unwrap_or("untitled"),
            "contentType": content_type,
            "sizeBytes": size_bytes,
            "updatedAt": now,
            "metadata": body["metadata"].as_str().unwrap_or("{}"),
            "data": data,
        });
        image_table.put(&id, record).await?;

        // Purge cached variants
        let all_variants: Vec<Value> = variant_table.get_all().await?;
        let mut purged = 0u32;
        for v in &all_variants {
            if v["imageId"].as_str() == Some(&id) {
                if let Some(vid) = v["id"].as_str() {
                    let _ = variant_table.delete(vid).await;
                    purged += 1;
                }
            }
        }

        ok(json!({
            "id": id,
            "contentType": content_type,
            "sizeBytes": size_bytes,
            "variantsPurged": purged
        }))
    }
});

fn is_image_type(ct: &str) -> bool {
    matches!(ct,
        "image/jpeg" | "image/png" | "image/webp" | "image/avif" |
        "image/gif" | "image/svg+xml" | "image/bmp" | "image/tiff"
    )
}

fn hash(s: &str) -> String {
    let mut h: u64 = 5381;
    for b in s.as_bytes().iter().take(1024) {
        h = h.wrapping_mul(33).wrapping_add(*b as u64);
    }
    format!("{:016x}", h)
}
