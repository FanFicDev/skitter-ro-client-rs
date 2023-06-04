use skitter_ro_client::{Client, Url};

#[tokio::main]
async fn main() {
    let args = std::env::args().collect::<Vec<String>>();
    if args.len() != 3 {
        println!("usage: {} <user> <pass>", args[0]);
        return;
    }

    let user = &args[1];
    let pass = &args[2];

    let client = reqwest::Client::new();
    let client = Client::new(
        client,
        Url::parse("https://zst1uv23.fanfic.dev/").unwrap(),
        user,
        pass,
    );

    let res = client
        .fetch_range(148868250, 148868500, Some("%/s/%"))
        .await
        .expect("failed to fetch_range");

    for r in res.into_iter() {
        println!("{r:?}");
    }

    let stat = client.fetch_stat().await.expect("failed to fetch_stat");
    println!("stat: {stat:#?}");
}
