use skitter_ro_client::CompressedWeb;
use sqlx::Row;
use sqlx::{migrate::MigrateDatabase, sqlite::SqlitePoolOptions};

#[tokio::main(flavor = "multi_thread", worker_threads = 1)]
async fn main() {
    let args = std::env::args().collect::<Vec<String>>();
    if args.len() != 3 {
        eprintln!("usage: {} <db_url> <id>", args[0]);
        return;
    }

    let db_url = &args[1];
    let id: i64 = args[2].parse().expect("failed to parse id");

    if !sqlx::Sqlite::database_exists(db_url)
        .await
        .expect("failed to check if db exists")
    {
        panic!("sqlite db does not exist: {db_url}");
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(db_url)
        .await
        .expect("failed to connect to sqlite db");

    let row = match sqlx::query("select * from web where id = ?")
        .bind(id)
        .fetch_optional(&pool)
        .await
        .expect("failed to query db")
    {
        Some(row) => row,
        None => panic!("failed to find id: {id}"),
    };

    let compressed_web = CompressedWeb {
        id: row.try_get("id").expect("failed to load id"),
        created: row.try_get("created").expect("failed to load created"),
        url: row.try_get("url").expect("failed to load url"),
        status: row.try_get("status").expect("failed to load status"),
        response: row.try_get("response").expect("failed to load response"),
    };
    eprintln!("{compressed_web:?}");

    let web = compressed_web
        .decompress()
        .await
        .expect("failed to decompress");
    eprintln!("          {web:?}");

    let text = std::str::from_utf8(&web.response).expect("failed to utf-8 decode response");
    println!("{text}");
}
