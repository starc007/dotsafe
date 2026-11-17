//! The share bucket against a small S3 stand-in: object writes with their
//! headers, the `?lifecycle` subresource, and public reads.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::thread;

use dotsafe_core::share::{ExpiryRule, ShareBucket};
use dotsafe_core::{Error, ShareBucketLocation};
use dotsafe_stores::{Fetcher, S3Credentials, S3ShareBucket};
use tiny_http::{Header, Response, Server};
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

const BUCKET: &str = "shares";

/// An object's bytes and the Cache-Control it was written with.
type Stored = (Vec<u8>, Option<String>);

#[derive(Default)]
struct Fake {
    objects: Mutex<BTreeMap<String, Stored>>,
    lifecycle: Mutex<Option<String>>,
    checksums: Mutex<Vec<String>>,
}

fn header(req: &tiny_http::Request, name: &'static str) -> Option<String> {
    req.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str().to_owned())
}

/// Serves the S3 API under `/shares/…`, public reads under `/public/…`, and
/// a redirect at `/moved`.
fn serve(fake: Arc<Fake>) -> String {
    let server = Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let url = Url::parse(&format!("http://local{}", req.url())).unwrap();
            let lifecycle = url.query_pairs().any(|(k, _)| k == "lifecycle");
            let path = url.path().to_string();
            let method = req.method().as_str().to_owned();
            let mut body = Vec::new();
            req.as_reader().read_to_end(&mut body).unwrap();
            let response = if path == "/moved" {
                Response::from_string("").with_status_code(302).with_header(Header::from_bytes("Location", "https://example.com/").unwrap())
            } else if path == "/big" {
                Response::from_data(vec![b'x'; 5 * 1024 * 1024])
            } else if let Some(key) = path.strip_prefix("/public/") {
                match fake.objects.lock().unwrap().get(key) {
                    Some((bytes, _)) => Response::from_data(bytes.clone()),
                    None => Response::from_string("<Error><Code>AccessDenied</Code></Error>").with_status_code(403),
                }
            } else if lifecycle {
                let mut config = fake.lifecycle.lock().unwrap();
                match method.as_str() {
                    "GET" => match config.as_ref() {
                        Some(xml) => Response::from_string(xml.clone()),
                        None => Response::from_string("<Error><Code>NoSuchLifecycleConfiguration</Code></Error>").with_status_code(404),
                    },
                    "PUT" => {
                        fake.checksums.lock().unwrap().push(header(&req, "x-amz-checksum-sha256").unwrap_or_default());
                        *config = Some(String::from_utf8(body).unwrap());
                        Response::from_string("")
                    }
                    "DELETE" => {
                        *config = None;
                        Response::from_string("").with_status_code(204)
                    }
                    _ => Response::from_string("").with_status_code(405),
                }
            } else {
                let key = path.trim_start_matches(&format!("/{BUCKET}/")).to_string();
                match method.as_str() {
                    "PUT" => {
                        let cache = header(&req, "Cache-Control");
                        fake.objects.lock().unwrap().insert(key, (body, cache));
                        Response::from_string("").with_header(Header::from_bytes("ETag", "\"1\"").unwrap())
                    }
                    "DELETE" => {
                        fake.objects.lock().unwrap().remove(&key);
                        Response::from_string("").with_status_code(204)
                    }
                    _ => Response::from_string("").with_status_code(405),
                }
            };
            let _ = req.respond(response);
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn bucket(endpoint: &str) -> S3ShareBucket {
    let location = ShareBucketLocation {
        endpoint: endpoint.into(),
        region: "auto".into(),
        bucket: BUCKET.into(),
        public_url: "https://shares.example.com/".into(),
    };
    let credentials = S3Credentials { access_key_id: "AKIA".into(), secret_access_key: Zeroizing::new("secret".into()) };
    S3ShareBucket::new(&location, &credentials).unwrap()
}

#[test]
fn snapshots_are_written_uncached_and_deleted() {
    let fake = Arc::new(Fake::default());
    let endpoint = serve(fake.clone());
    let bucket = bucket(&endpoint);
    assert_eq!(bucket.public_url(), "https://shares.example.com");

    bucket.put("s/abc", b"sealed").unwrap();
    let stored = fake.objects.lock().unwrap().get("s/abc").cloned().unwrap();
    assert_eq!(stored, (b"sealed".to_vec(), Some("no-store".into())));

    // Read back the way a machine does, with no key.
    let fetched = Fetcher::allowing_http_for_tests().get(&format!("{endpoint}/public/s/abc")).unwrap();
    assert_eq!(fetched.as_deref(), Some(&b"sealed"[..]));

    bucket.delete("s/abc").unwrap();
    bucket.delete("s/abc").unwrap();
    assert_eq!(Fetcher::allowing_http_for_tests().get(&format!("{endpoint}/public/s/abc")).unwrap(), None);
}

#[test]
fn expiry_rules_replace_ours_and_keep_the_users() {
    let fake = Arc::new(Fake::default());
    let endpoint = serve(fake.clone());
    let bucket = bucket(&endpoint);
    *fake.lifecycle.lock().unwrap() = Some(
        "<LifecycleConfiguration><Rule><ID>old-logs</ID><Filter><Prefix>logs/</Prefix></Filter><Status>Enabled</Status><Expiration><Days>30</Days></Expiration></Rule><Rule><ID>dotsafe-share-stale</ID><Filter><Prefix>s/00</Prefix></Filter><Status>Enabled</Status><Expiration><Days>1</Days></Expiration></Rule></LifecycleConfiguration>".into(),
    );

    let share = Uuid::new_v4();
    let in_a_month = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
        + 30 * 24 * 60 * 60 * 1000;
    bucket.set_expiry_rules(&[ExpiryRule { share, prefix: "s/ff".into(), expires: in_a_month }]).unwrap();
    let xml = fake.lifecycle.lock().unwrap().clone().unwrap();
    assert!(xml.contains("<ID>old-logs</ID>"), "the user's rule stays: {xml}");
    assert!(!xml.contains("dotsafe-share-stale"), "rules for shares that are gone go");
    assert!(xml.contains(&format!("<ID>dotsafe-share-{share}</ID>")) && xml.contains("<Prefix>s/ff</Prefix>"));
    assert!(xml.contains("T00:00:00.000Z</Date>"));
    assert_eq!(fake.checksums.lock().unwrap().last().map(|c| c.len()), Some(44), "a base64 SHA-256 of the body");

    // With nothing of ours and nothing of the user's, the configuration goes.
    *fake.lifecycle.lock().unwrap() = None;
    bucket.set_expiry_rules(&[]).unwrap();
    assert!(fake.lifecycle.lock().unwrap().is_none());
}

#[test]
fn fetching_is_https_only_and_follows_no_redirects() {
    let fake = Arc::new(Fake::default());
    let endpoint = serve(fake);
    let err = Fetcher::new().get(&format!("{endpoint}/public/s/abc")).unwrap_err();
    assert!(err.to_string().contains("only fetched over HTTPS"), "{err}");

    let err = Fetcher::allowing_http_for_tests().get(&format!("{endpoint}/moved")).unwrap_err();
    assert!(err.to_string().contains("doesn't follow redirects"), "{err}");

    assert!(Fetcher::allowing_http_for_tests().get(&format!("{endpoint}/big")).is_err(), "oversized bodies are refused");

    let err = Fetcher::new().get("https://127.0.0.1:1/s/abc").unwrap_err();
    assert!(matches!(err, Error::Unreachable(_)), "{err}");
}
