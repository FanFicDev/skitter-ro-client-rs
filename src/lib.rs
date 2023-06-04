use async_compression::tokio::bufread::ZlibDecoder;
pub use reqwest;
pub use reqwest::Url;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use time::OffsetDateTime;
use tokio::io::AsyncReadExt;

#[derive(Clone)]
pub struct Client<'a> {
    pub client: reqwest::Client,
    pub base_url: Url,
    pub user: &'a str,
    pub pass: &'a str,
}

#[serde_with::serde_as]
#[derive(Serialize, PartialEq)]
pub struct Web {
    pub id: i64,
    #[serde(serialize_with = "serialize_rfc3339")]
    pub created: OffsetDateTime,
    pub url: String,
    pub status: i16,
    #[serde_as(as = "serde_with::base64::Base64")]
    pub response: Vec<u8>,
}

impl fmt::Debug for Web {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Web")
            .field("id", &self.id)
            .field("created", &self.created.to_string())
            .field("url", &self.url)
            .field("status", &self.status)
            .field("response.len", &self.response.len())
            .finish()
    }
}

fn serialize_rfc3339<S: Serializer>(v: &OffsetDateTime, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(
        &v.format(&time::format_description::well_known::Rfc3339)
            .unwrap(),
    )
}

fn deserialize_rfc3339<'de, D: Deserializer<'de>>(d: D) -> Result<OffsetDateTime, D::Error> {
    let s: &str = Deserialize::deserialize(d)?;
    Ok(OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).unwrap())
}

#[serde_with::serde_as]
#[derive(Deserialize)]
pub struct CompressedWeb {
    pub id: i64,
    #[serde(deserialize_with = "deserialize_rfc3339")]
    pub created: OffsetDateTime,
    pub url: String,
    pub status: i16,
    #[serde_as(as = "serde_with::base64::Base64")]
    pub response: Vec<u8>,
}

impl CompressedWeb {
    pub async fn decompress(self) -> Result<Web, String> {
        if self.response.len() < 4 {
            return Err(format!(
                "decompression error: missing header, length: {}",
                self.response.len()
            ));
        }

        let expected_size = {
            let mut header: [u8; 4] = [0, 0, 0, 0];
            (0..4).for_each(|i| {
                header[i] = self.response[i];
            });
            u32::from_be_bytes(header) as usize
        };

        let mut d = ZlibDecoder::new(&self.response[4..]);
        let mut buf = Vec::<u8>::new();
        if let Err(e) = d.read_to_end(&mut buf).await {
            return Err(format!("decompression error: could not read to end: {e}"));
        }

        if buf.len() != expected_size {
            return Err(format!(
                "decompression error: expected {expected_size} bytes, got {}",
                buf.len()
            ));
        }

        Ok(Web {
            id: self.id,
            created: self.created,
            url: self.url,
            status: self.status,
            response: buf,
        })
    }
}

impl fmt::Debug for CompressedWeb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompressedWeb")
            .field("id", &self.id)
            .field("created", &self.created.to_string())
            .field("url", &self.url)
            .field("status", &self.status)
            .field("response.len", &self.response.len())
            .finish()
    }
}

#[derive(Deserialize, Debug)]
pub struct WebStat {
    pub max_wid: i64,
}

#[derive(Deserialize)]
struct WebRangeResponse {
    pub entries: Vec<CompressedWeb>,
}

impl<'a> Client<'a> {
    pub fn new(client: reqwest::Client, base_url: Url, user: &'a str, pass: &'a str) -> Self {
        Self {
            client,
            base_url,
            user,
            pass,
        }
    }

    // stat returns information about the current state of the upstream db.
    #[tracing::instrument(skip(self), err)]
    pub async fn fetch_stat(&self) -> Result<WebStat, String> {
        let res = self
            .client
            .get(
                self.base_url
                    .join("v0/web/stat")
                    .map_err(|e| format!("failed to build full url: {e}"))?,
            )
            .basic_auth(self.user, Some(self.pass))
            .header(
                "User-Agent",
                format!("skitter-ro-client-rs/0.0.1 +{}", self.user),
            )
            .send()
            .await
            .map_err(|e| format!("failed to send request: {e}"))?;

        let status = res.status();
        if status != reqwest::StatusCode::OK {
            return match res.bytes().await {
                Ok(body) => Err(format!(
                    "failed to fetch web stat unexpected status: {status}: {body:?}",
                )),
                Err(e) => Err(format!(
                    "failed to fetch web stat unexpected status and failed to read body: {status}: {e}",
                )),
            };
        }

        let body = res
            .text()
            .await
            .map_err(|e| format!("failed to fetch response body: {e}"))?;
        serde_json::from_str::<WebStat>(&body)
            .map_err(|e| format!("failed to deserialize response body: {e}"))
    }

    // fetch_range_compressed returns a range of cached web responses from based on their id.
    #[tracing::instrument(skip(self), err)]
    pub async fn fetch_range_compressed(
        &self,
        min_wid: i64,
        max_wid: i64,
        url_like: Option<&str>,
    ) -> Result<Vec<CompressedWeb>, String> {
        if max_wid - min_wid > 1000 {
            return Err("max_wid - min_wid must be less than 1000".to_string());
        }

        let params = [
            ("min_wid", Some(min_wid.to_string())),
            ("max_wid", Some(max_wid.to_string())),
            ("url_like", url_like.map(|u| u.to_string())),
        ];
        let res = self
            .client
            .get(
                self.base_url
                    .join("v0/web/range")
                    .map_err(|e| format!("failed to build full url: {e}"))?,
            )
            .query(&params)
            .basic_auth(self.user, Some(self.pass))
            .header(
                "User-Agent",
                format!("skitter-ro-client-rs/0.0.1 +{}", self.user),
            )
            .send()
            .await
            .map_err(|e| format!("failed to send request: {e}"))?;

        let status = res.status();
        if status != reqwest::StatusCode::OK {
            return match res.bytes().await {
                Ok(body) => Err(format!(
                    "failed to fetch web range unexpected status: {status}: {body:?}",
                )),
                Err(e) => Err(format!(
                    "failed to fetch web range unexpected status and failed to read body: {status}: {e}",
                )),
            };
        }

        let body = res
            .text()
            .await
            .map_err(|e| format!("failed to fetch response body: {e}"))?;
        Ok(serde_json::from_str::<WebRangeResponse>(&body)
            .map_err(|e| format!("failed to deserialize response body: {e}"))?
            .entries)
    }

    // fetch_range returns a range of cached web responses from based on their id.
    #[tracing::instrument(skip(self), err)]
    pub async fn fetch_range(
        &self,
        min_wid: i64,
        max_wid: i64,
        url_like: Option<&str>,
    ) -> Result<Vec<Web>, String> {
        let entries = self
            .fetch_range_compressed(min_wid, max_wid, url_like)
            .await?;

        let mut res = vec![];
        let mut errs = vec![];
        for w in entries.into_iter() {
            match w.decompress().await {
                Ok(w) => res.push(w),
                Err(e) => errs.push(e),
            }
        }

        if res.is_empty() && !errs.is_empty() {
            return Err(format!(
                "failed to decompress response: {}",
                errs.join("; ")
            ));
        }
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "api_user";
    const PASS: &str = "api_pass";

    // A properly encoded `response` for testing can be produced through zlib and a base64 encoder:
    // ```python
    //     import zlib
    //     import base64
    //     text = b'example body'
    //     header = len(text).to_bytes(4, byteorder='big', signed=False)
    //     body = zlib.compress(text)
    //     response = base64.b64encode(header + body)
    //     print(response)
    // ```

    fn parse_rfc3339(v: &str) -> OffsetDateTime {
        OffsetDateTime::parse(v, &time::format_description::well_known::Rfc3339).unwrap()
    }

    fn basic_auth(user: &str, pass: &str) -> String {
        use base64::Engine;

        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"))
        )
    }

    fn user_agent(user: &str) -> String {
        format!("skitter-ro-client-rs/0.0.1 +{user}")
    }

    #[tokio::test]
    async fn fetch_range_success_empty() {
        let server = httpmock::MockServer::start();
        let web_range_mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/v0/web/range")
                .header("Authorization", basic_auth(USER, PASS))
                .header("User-Agent", user_agent(USER))
                .query_param("min_wid", "100")
                .query_param("max_wid", "200")
                .query_param("url_like", "%foo%");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(r#"{"entries":[]}"#);
        });

        let base_url = Url::parse(&server.base_url()).unwrap();
        let client = reqwest::Client::new();
        let client = super::Client::new(client, base_url, USER, PASS);

        let res = client.fetch_range(100, 200, Some("%foo%")).await;

        web_range_mock.assert();

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.len(), 0);
    }

    #[tokio::test]
    async fn fetch_range_success() {
        let server = httpmock::MockServer::start();
        let web_range_mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/v0/web/range")
                .header("Authorization", basic_auth(USER, PASS))
                .header("User-Agent", user_agent(USER))
                .query_param("min_wid", "100")
                .query_param("max_wid", "1100");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(r#"{"entries":[
                    {"id":100,"created":"2023-06-01T23:24:25.065Z","url":"https://example.com/s/1/1","status":200,"response":"AAAADHicS61IzC3ISVVIyk+pBAAfFwS7"},
                    {"id":101,"created":"2023-06-02T23:24:25.065Z","url":"https://example.com/s/1/2","status":200,"response":"AAAADHicS61IzC3ISVVIyk+pBAAfFwS7"},
                    {"id":102,"created":"2023-06-03T23:24:25.065Z","url":"https://example.com/s/2/1","status":200,"response":"AAAADHicS61IzC3ISVVIyk+pBAAfFwS7"}
                ]}"#);
        });

        let base_url = Url::parse(&server.base_url()).unwrap();
        let client = reqwest::Client::new();
        let client = super::Client::new(client, base_url, USER, PASS);

        let res = client.fetch_range(100, 1100, None).await;

        web_range_mock.assert();
        if res.is_err() {
            eprintln!("{res:?}");
        }

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.len(), 3);
        assert_eq!(
            res[0],
            Web {
                id: 100,
                created: parse_rfc3339("2023-06-01T23:24:25.065Z"),
                url: "https://example.com/s/1/1".to_string(),
                status: 200,
                response: b"example body".to_vec(),
            }
        );
        assert_eq!(
            res[1],
            Web {
                id: 101,
                created: parse_rfc3339("2023-06-02T23:24:25.065Z"),
                url: "https://example.com/s/1/2".to_string(),
                status: 200,
                response: b"example body".to_vec(),
            }
        );
        assert_eq!(
            res[2],
            Web {
                id: 102,
                created: parse_rfc3339("2023-06-03T23:24:25.065Z"),
                url: "https://example.com/s/2/1".to_string(),
                status: 200,
                response: b"example body".to_vec(),
            }
        );
    }

    #[tokio::test]
    async fn fetch_range_error_overlimit() {
        let client = reqwest::Client::new();
        let client = super::Client::new(
            client,
            Url::parse("https://example.com").unwrap(),
            USER,
            PASS,
        );

        let res = client.fetch_range(100, 1200, None).await;

        assert!(res.is_err());
        assert_eq!(
            res,
            Err("max_wid - min_wid must be less than 1000".to_string())
        );
    }

    #[tokio::test]
    async fn fetch_range_error_failed_to_send_request() {
        let client = reqwest::Client::new();
        let client = super::Client::new(
            client,
            Url::parse("https://example.invalid").unwrap(),
            USER,
            PASS,
        );

        let res = client.fetch_range(100, 200, None).await;

        assert!(res.is_err());
        if let Err(e) = res {
            assert!(e.starts_with("failed to send request: "));
        }
    }

    #[tokio::test]
    async fn fetch_range_error_failed_to_fetch() {
        let server = httpmock::MockServer::start();
        let web_range_mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/v0/web/range")
                .header("Authorization", basic_auth(USER, PASS))
                .header("User-Agent", user_agent(USER))
                .query_param("min_wid", "100")
                .query_param("max_wid", "200");
            then.status(500)
                .header("Content-Type", "application/json")
                .body(r#"{"err":1,"msg":"internal server error"}"#);
        });

        let base_url = Url::parse(&server.base_url()).unwrap();
        let client = reqwest::Client::new();
        let client = super::Client::new(client, base_url, USER, PASS);

        let res = client.fetch_range(100, 200, None).await;

        web_range_mock.assert();

        assert!(res.is_err());
        assert_eq!(res, Err(r#"failed to fetch web range unexpected status: 500 Internal Server Error: b"{\"err\":1,\"msg\":\"internal server error\"}""#.to_string()));
    }

    #[tokio::test]
    async fn fetch_range_error_failed_to_deserialize() {
        let server = httpmock::MockServer::start();
        let web_range_mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/v0/web/range")
                .header("Authorization", basic_auth(USER, PASS))
                .header("User-Agent", user_agent(USER))
                .query_param("min_wid", "100")
                .query_param("max_wid", "200");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(r#"{"entries":"#);
        });

        let base_url = Url::parse(&server.base_url()).unwrap();
        let client = reqwest::Client::new();
        let client = super::Client::new(client, base_url, USER, PASS);

        let res = client.fetch_range(100, 200, None).await;

        web_range_mock.assert();

        assert!(res.is_err());
        if let Err(e) = res {
            assert!(e.starts_with("failed to deserialize response body: "));
        }
    }

    async fn decompress_test(name: &str, needle: &str, response: &str) {
        let server = httpmock::MockServer::start();
        let web_range_mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/v0/web/range")
                .header("Authorization", basic_auth(USER, PASS))
                .header("User-Agent", user_agent(USER))
                .query_param("min_wid", "100")
                .query_param("max_wid", "200");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(r#"{"entries":[
                    {"id":100,"created":"2023-06-01T23:24:25.065Z","url":"https://example.com/s/1/1","status":200,"response":"RESPONSE_BODY"}
                ]}"#.replace("RESPONSE_BODY", response));
        });

        let base_url = Url::parse(&server.base_url()).unwrap();
        let client = reqwest::Client::new();
        let client = super::Client::new(client, base_url, USER, PASS);

        let res = client.fetch_range(100, 200, None).await;

        web_range_mock.assert();

        assert!(res.is_err(), "decompress_test({name})");
        if let Err(e) = res {
            assert_eq!(e, needle, "decompress_test({name})");
        }
    }

    #[tokio::test]
    async fn fetch_range_error_failed_to_decompress() {
        decompress_test(
            "missing_header",
            "failed to decompress response: decompression error: missing header, length: 2",
            "XDA=",
        )
        .await;

        decompress_test(
            "bad_data",
            "failed to decompress response: decompression error: could not read to end: deflate decompression error",
            "AAAADHicS61IzC3ISVVIyk+pBAAfFwS6",
        )
        .await;

        decompress_test(
            "mismatched_len",
            "failed to decompress response: decompression error: expected 100 bytes, got 12",
            "AAAAZHicS61IzC3ISVVIyk+pBAAfFwS7",
        )
        .await;
    }

    #[tokio::test]
    async fn fetch_stat_success() {
        let server = httpmock::MockServer::start();
        let web_range_mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/v0/web/stat")
                .header("Authorization", basic_auth(USER, PASS))
                .header("User-Agent", user_agent(USER));
            then.status(200)
                .header("Content-Type", "application/json")
                .body(r#"{"max_wid":1024}"#);
        });

        let base_url = Url::parse(&server.base_url()).unwrap();
        let client = reqwest::Client::new();
        let client = super::Client::new(client, base_url, USER, PASS);

        let res = client.fetch_stat().await;

        web_range_mock.assert();

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.max_wid, 1024);
    }
}
