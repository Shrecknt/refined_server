use nbt::Blob;
use sqlx::postgres::PgRow;

pub async fn items_in_chest(
    pool: &sqlx::PgPool,
    x: f64,
    y: f64,
    z: f64,
) -> Result<Vec<PgRow>, sqlx::Error> {
    sqlx::query("SELECT * FROM get_items_from_chest ($1::float, $2::float, $3::float);")
        .bind(x as f64)
        .bind(y as f64)
        .bind(z as f64)
        .fetch_all(pool)
        .await
}

pub async fn set_item_in_chest(
    pool: &sqlx::PgPool,
    x: f64,
    y: f64,
    z: f64,
    location_in_chest: i32,
    item_id: &str,
    item_count: i16,
    mut item_nbt: Option<Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error>> {
    if item_nbt.is_none() {
        let blob: Blob = Blob::new();
        let mut serialized_nbt: Vec<u8> = vec![];
        blob.to_writer(&mut serialized_nbt)?;
        item_nbt = Some(serialized_nbt);
    }
    sqlx::query("CALL insert_item_into_chest ($1::float, $2::float, $3::float, $4::int, $5::text, $6::smallint, $7::bytea);")
        .bind(x as f64)
        .bind(y as f64)
        .bind(z as f64)
        .bind(location_in_chest)
        .bind(item_id)
        .bind(item_count)
        .bind(item_nbt)
        .fetch_optional(pool)
        .await?;
    Ok(())
}

pub async fn create_chest(pool: &sqlx::PgPool, x: f64, y: f64, z: f64) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO chests (x, y, z) VALUES ($1::float, $2::float, $3::float) ON CONFLICT (x, y, z) DO NOTHING;")
        .bind(x as f64)
        .bind(y as f64)
        .bind(z as f64)
        .fetch_optional(pool).await?;
    Ok(())
}
