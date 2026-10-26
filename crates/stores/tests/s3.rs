//! S3Store against a small in-process S3 stand-in that implements the parts
//! dotsafe uses: GET/PUT/DELETE objects, conditional PUTs, ListObjectsV2 with
//! a delimiter and pagination. It checks requests are signed but doesn't
//! verify signatures.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use dotsafe_core::store::{Cond, Put, Store};
use dotsafe_core::{CachedStore, Environment, Error, KdfParams, Record, S3Location, Vault};
use dotsafe_stores::{S3Credentials, S3Store};
use tiny_http::{Header, Response, Server};
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };
const BUCKET: &str = "vaults";
const PAGE: usize = 2;

#[derive(Default)]
struct Bucket {
    objects: Mutex<BTreeMap<String, (Vec<u8>, String)>>,
    versions: AtomicU64,
    unsigned: AtomicU64,
}

fn header(req: &tiny_http::Request, name: &'static str) -> Option<String> {
    req.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str().to_owned())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Starts the stand-in and returns its endpoint.
fn serve(bucket: Arc<Bucket>) -> String {
    let server = Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let url = Url::parse(&format!("http://local{}", req.url())).unwrap();
            let query: BTreeMap<String, String> = url.query_pairs().into_owned().collect();
            if !query.contains_key("X-Amz-Signature") {
                bucket.unsigned.fetch_add(1, Ordering::SeqCst);
            }
            let path = percent_decode(url.path());
            let rest = path.strip_prefix(&format!("/{BUCKET}")).unwrap_or("");
            let key = rest.trim_start_matches('/').to_owned();
            let method = req.method().as_str().to_owned();
            let response = match (method.as_str(), key.is_empty()) {
                ("GET", true) => list(&bucket, &query),
                ("GET", false) => match bucket.objects.lock().unwrap().get(&key) {
                    Some((bytes, etag)) => Response::from_data(bytes.clone())
                        .with_header(Header::from_bytes("ETag", etag.as_str()).unwrap()),
                    None => Response::from_string("<Error><Code>NoSuchKey</Code></Error>").with_status_code(404),
                },
                ("PUT", false) => {
                    let conditions = (header(&req, "If-None-Match"), header(&req, "If-Match"));
                    let mut body = Vec::new();
                    req.as_reader().read_to_end(&mut body).unwrap();
                    let mut objects = bucket.objects.lock().unwrap();
                    let current = objects.get(&key).map(|(_, e)| e.clone());
                    let failed = match (conditions.0.as_deref(), conditions.1.as_deref()) {
                        (Some("*"), _) => current.is_some(),
                        (_, Some(expected)) => current.as_deref() != Some(expected),
                        _ => false,
                    };
                    if failed {
                        Response::from_string("<Error><Code>PreconditionFailed</Code></Error>").with_status_code(412)
                    } else {
                        let etag = format!("\"{}\"", bucket.versions.fetch_add(1, Ordering::SeqCst));
                        objects.insert(key, (body, etag.clone()));
                        Response::from_string("").with_header(Header::from_bytes("ETag", etag.as_str()).unwrap())
                    }
                }
                ("DELETE", false) => {
                    bucket.objects.lock().unwrap().remove(&key);
                    Response::from_string("").with_status_code(204)
                }
                _ => Response::from_string("").with_status_code(400),
            };
            let _ = req.respond(response);
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn percent_decode(s: &str) -> String {
    url::form_urlencoded::parse(s.replace('+', "%2B").as_bytes()).map(|(k, v)| format!("{k}{v}")).collect()
}

fn list(bucket: &Bucket, query: &BTreeMap<String, String>) -> Response<std::io::Cursor<Vec<u8>>> {
    let prefix = query.get("prefix").cloned().unwrap_or_default();
    let delimiter = query.get("delimiter").cloned();
    let after = query.get("continuation-token").cloned().unwrap_or_default();
    let objects = bucket.objects.lock().unwrap();
    let matching: Vec<(&String, &String)> = objects
        .iter()
        .filter(|(k, _)| k.starts_with(&prefix) && k.as_str() > after.as_str())
        .filter(|(k, _)| delimiter.as_deref().is_none_or(|d| !k[prefix.len()..].contains(d)))
        .map(|(k, (_, e))| (k, e))
        .collect();
    let page: Vec<_> = matching.iter().take(PAGE).collect();
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">"#,
    );
    xml += &format!("<Name>{BUCKET}</Name><Prefix>{}</Prefix><MaxKeys>{PAGE}</MaxKeys>", xml_escape(&prefix));
    for (key, etag) in &page {
        xml += &format!(
            "<Contents><Key>{}</Key><LastModified>2026-10-26T00:00:00.000Z</LastModified><ETag>{}</ETag><Size>1</Size><StorageClass>STANDARD</StorageClass></Contents>",
            xml_escape(key),
            xml_escape(etag)
        );
    }
    if matching.len() > PAGE {
        xml += &format!(
            "<IsTruncated>true</IsTruncated><NextContinuationToken>{}</NextContinuationToken>",
            xml_escape(page.last().unwrap().0)
        );
    } else {
        xml += "<IsTruncated>false</IsTruncated>";
    }
    xml += "</ListBucketResult>";
    Response::from_string(xml)
}

fn location(endpoint: &str, prefix: &str) -> S3Location {
    S3Location { endpoint: endpoint.into(), region: "auto".into(), bucket: BUCKET.into(), prefix: prefix.into() }
}

fn credentials() -> S3Credentials {
    S3Credentials { access_key_id: "test-key".into(), secret_access_key: Zeroizing::new("test-secret".into()) }
}

fn store(endpoint: &str, prefix: &str) -> S3Store {
    S3Store::new(&location(endpoint, prefix), &credentials()).unwrap()
}

#[test]
fn objects_round_trip_with_conditional_writes() {
    let bucket = Arc::new(Bucket::default());
    let s3 = store(&serve(bucket.clone()), "team/dotsafe");

    assert!(s3.get("vault.json").unwrap().is_none());
    let Put::Written(v1) = s3.put("vault.json", b"one", Cond::IfAbsent).unwrap() else { panic!() };
    assert_eq!(s3.put("vault.json", b"two", Cond::IfAbsent).unwrap(), Put::Conflict);
    assert_eq!(s3.put("vault.json", b"two", Cond::IfMatch("\"stale\"".into())).unwrap(), Put::Conflict);
    assert!(matches!(s3.put("vault.json", b"two", Cond::IfMatch(v1)).unwrap(), Put::Written(_)));
    assert_eq!(s3.get("vault.json").unwrap().unwrap().bytes, b"two");

    // Keys live under the prefix in the bucket.
    assert!(bucket.objects.lock().unwrap().contains_key("team/dotsafe/vault.json"));

    s3.delete("vault.json").unwrap();
    assert!(s3.get("vault.json").unwrap().is_none());
    assert_eq!(bucket.unsigned.load(Ordering::SeqCst), 0, "every request is signed");
}

#[test]
fn listing_pages_and_stays_inside_the_directory() {
    let bucket = Arc::new(Bucket::default());
    let s3 = store(&serve(bucket.clone()), "p");
    for name in ["a", "b", "c", "d", "e"] {
        s3.put(&format!("records/{name}.json"), b"x", Cond::Any).unwrap();
    }
    s3.put("vault.json", b"x", Cond::Any).unwrap();
    s3.put("records/.hidden", b"x", Cond::Any).unwrap();
    bucket.objects.lock().unwrap().insert("other/vault.json".into(), (b"x".to_vec(), "\"o\"".into()));

    let records = s3.list("records").unwrap();
    assert_eq!(records, ["records/a.json", "records/b.json", "records/c.json", "records/d.json", "records/e.json"]);
    assert_eq!(s3.list("").unwrap(), ["vault.json"]);
    let metas = s3.list_meta("records").unwrap();
    assert!(metas.iter().all(|m| m.etag.starts_with('"')));
}

#[test]
fn unreachable_endpoints_are_reported_as_offline() {
    // Nothing listens on this port.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let s3 = store(&endpoint, "");
    assert!(matches!(s3.get("vault.json"), Err(Error::Unreachable(_))));
    assert!(matches!(s3.put("vault.json", b"x", Cond::Any), Err(Error::Unreachable(_))));
}

#[test]
fn bad_endpoints_are_rejected_up_front() {
    assert!(matches!(S3Store::new(&location("not a url", ""), &credentials()), Err(Error::Storage(_))));
}

#[test]
fn a_vault_in_a_bucket_syncs_between_devices() {
    let bucket = Arc::new(Bucket::default());
    let endpoint = serve(bucket);
    let (cache_a, cache_b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());

    let a = Vault::create(CachedStore::open(store(&endpoint, "v"), cache_a.path()).unwrap(), "pw", FAST).unwrap();
    let device = Uuid::new_v4();
    let mut env = Environment::new(Uuid::new_v4(), "production", a.stamp(device));
    env.set("STRIPE_KEY", "sk_live_x", a.stamp(device));
    a.put(Record::Environment(env.clone())).unwrap();

    let b = Vault::unlock(CachedStore::open(store(&endpoint, "v"), cache_b.path()).unwrap(), "pw").unwrap();
    let Some(Record::Environment(seen)) = b.get(env.id).unwrap() else { panic!() };
    assert_eq!(seen.value("STRIPE_KEY"), Some("sk_live_x"));

    // B changes it; A picks it up on sync.
    let mut edited = seen;
    edited.set("STRIPE_KEY", "sk_live_rotated", b.stamp(Uuid::new_v4()));
    b.put(Record::Environment(edited)).unwrap();
    a.sync().unwrap();
    let Some(Record::Environment(after)) = a.get(env.id).unwrap() else { panic!() };
    assert_eq!(after.value("STRIPE_KEY"), Some("sk_live_rotated"));
}
