use skitter_ro_client::{Client, Url};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::ConnectOptions;
use sqlx::{sqlite::SqlitePool, sqlite::SqlitePoolOptions};
use std::cmp::{max, min};
use std::str::FromStr;
use tap::prelude::*;
use tokio::time::Duration;
use tracing_log::LogTracer;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::{prelude::*, EnvFilter};

struct Tracer;

impl Tracer {
    pub fn new() -> Self {
        LogTracer::init().expect("failed to set logger");

        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(env_filter).with(
                tracing_subscriber::fmt::Layer::new()
                    .with_file(true)
                    .with_line_number(true)
                    .with_span_events(FmtSpan::CLOSE),
            ),
        )
        .expect("failed to set tracing subscriber");

        Self {}
    }
}

impl Drop for Tracer {
    fn drop(&mut self) {}
}

#[tracing::instrument]
async fn get_pool(db_url: &str) -> SqlitePool {
    let conn_opt = SqliteConnectOptions::from_str(db_url)
        .expect("failed to build conn_opt")
        .create_if_missing(true)
        .tap_mut(|c| {
            c.log_statements(tracing_log::log::LevelFilter::Debug);
        });
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(conn_opt)
        .await
        .expect("failed to connect to sqlite db");

    sqlx::query(&std::fs::read_to_string("./sql/001_init.sql").expect("failed to read schema"))
        .execute(&pool)
        .await
        .expect("failed to create table");

    pool
}

#[tokio::main(flavor = "multi_thread", worker_threads = 1)]
async fn main() {
    let _tracer = Tracer::new();

    let args = std::env::args().collect::<Vec<String>>();
    if args.len() != 4 {
        eprintln!("usage: {} <user> <pass> <url_like>", args[0]);
        return;
    }

    let user = &args[1];
    let pass = &args[2];
    let url_like = &args[3];

    let client = reqwest::Client::new();
    let client = Client::new(
        client,
        Url::parse("https://zst1uv23.fanfic.dev/").unwrap(),
        user,
        pass,
    );

    let db_url = "./web.db";
    let pool = get_pool(db_url).await;

    let mut interval = tokio::time::interval(Duration::from_secs(60));
    interval.tick().await; // First tick completes near instantly.

    loop {
        pull(url_like, &client, &pool).await;
        interval.tick().await;
    }
}

#[tracing::instrument(skip(client, pool))]
async fn pull(url_like: &str, client: &Client<'_>, pool: &SqlitePool) {
    let max_wid = client
        .fetch_stat()
        .await
        .expect("failed to fetch_stat")
        .max_wid;
    tracing::info!(max_wid, "fetched max_wid");
    let max_wid = max_wid + 1; // Cover max_wid with half-open range.

    let stored_max_wid = max(
        149470000 - 1,
        sqlx::query_scalar("select max(id) from web")
            .fetch_one(pool)
            .await
            .unwrap_or(0),
    );

    const BLOCK_SIZE: usize = 1000;
    for next_wid in (stored_max_wid + 1..max_wid).step_by(BLOCK_SIZE) {
        let target_max_wid = min(next_wid + 1000, max_wid);
        pull_block(next_wid, target_max_wid, url_like, client, pool).await;
    }
}

#[tracing::instrument(skip(client, pool))]
async fn pull_block(
    min_wid: i64,
    max_wid: i64,
    url_like: &str,
    client: &Client<'_>,
    pool: &SqlitePool,
) {
    let res = client
        .fetch_range_compressed(min_wid, max_wid, Some(url_like))
        .await
        .expect("failed to fetch_range");
    tracing::info!(
        block_span = max_wid - min_wid,
        count = res.len(),
        "fetched block"
    );

    for r in res.into_iter() {
        sqlx::query("insert into web(id, created, url, status, response) values(?, ?, ?, ?, ?)")
            .bind(r.id)
            .bind(r.created)
            .bind(r.url)
            .bind(r.status)
            .bind(r.response)
            .execute(pool)
            .await
            .expect("failed to insert");
    }
}
