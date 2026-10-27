//! WebDavStore against a small in-process WebDAV stand-in: GET, PUT with
//! If-Match / If-None-Match, DELETE, PROPFIND (Depth 1) and MKCOL, with Basic
//! auth. Like Nextcloud, a PUT into a folder that doesn't exist yet is a 409.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use dotsafe_core::store::{Cond, Put, Store};
use dotsafe_core::{CachedStore, Environment, KdfParams, Record, Vault, WebDavLocation};
use dotsafe_stores::WebDavStore;
use tiny_http::{Header, Response, Server};
use uuid::Uuid;

const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };
const ROOT: &str = "/dav/files/me/dotsafe";

#[derive(Default)]
struct Dav {
    files: Mutex<BTreeMap<String, (Vec<u8>, String)>>,
    folders: Mutex<BTreeSet<String>>,
    versions: AtomicU64,
}

fn header(req: &tiny_http::Request, name: &'static str) -> Option<String> {
    req.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str().to_owned())
}

fn serve(dav: Arc<Dav>) -> String {
    dav.folders.lock().unwrap().insert(ROOT.to_string());
    let server = Server::http("127.0.0.1:0").unwrap();
    let addr = server.server_addr().to_ip().unwrap();
    let expected = format!("Basic {}", base64::engine::general_purpose::STANDARD.encode("me:app-password"));
    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            if header(&req, "Authorization").as_deref() != Some(expected.as_str()) {
                let _ = req.respond(Response::empty(401));
                continue;
            }
            let path = req.url().split('?').next().unwrap().trim_end_matches('/').to_string();
            let parent = path.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default();
            let method = req.method().to_string();
            let mut body = Vec::new();
            req.as_reader().read_to_end(&mut body).unwrap();
            let mut files = dav.files.lock().unwrap();
            let mut folders = dav.folders.lock().unwrap();
            let response = match method.as_str() {
                "GET" => match files.get(&path) {
                    Some((bytes, etag)) => Response::from_data(bytes.clone())
                        .with_header(Header::from_bytes("ETag", etag.as_bytes()).unwrap()),
                    None => Response::from_data(Vec::new()).with_status_code(404),
                },
                "PUT" => {
                    let current = files.get(&path).map(|(_, e)| e.clone());
                    let if_match = header(&req, "If-Match");
                    let if_none = header(&req, "If-None-Match");
                    if !folders.contains(&parent) {
                        Response::from_data(Vec::new()).with_status_code(409)
                    } else if (if_none.as_deref() == Some("*") && current.is_some())
                        || if_match.as_ref().is_some_and(|m| Some(m) != current.as_ref())
                    {
                        Response::from_data(Vec::new()).with_status_code(412)
                    } else {
                        let etag = format!("\"v{}\"", dav.versions.fetch_add(1, Ordering::SeqCst));
                        files.insert(path.clone(), (body, etag.clone()));
                        Response::from_data(Vec::new())
                            .with_status_code(201)
                            .with_header(Header::from_bytes("ETag", etag.as_bytes()).unwrap())
                    }
                }
                "DELETE" => {
                    files.remove(&path);
                    Response::from_data(Vec::new()).with_status_code(204)
                }
                "MKCOL" => {
                    if folders.insert(path.clone()) { Response::from_data(Vec::new()).with_status_code(201) } else {
                        Response::from_data(Vec::new()).with_status_code(405)
                    }
                }
                "PROPFIND" => {
                    if !folders.contains(&path) {
                        Response::from_data(Vec::new()).with_status_code(404)
                    } else {
                        let mut xml = format!(
                            r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:"><d:response><d:href>{path}/</d:href></d:response>"#
                        );
                        for (file, (_, etag)) in files.iter().filter(|(f, _)| f.rsplit_once('/').map(|(p, _)| p) == Some(path.as_str())) {
                            let href = file.replace(' ', "%20");
                            xml.push_str(&format!("<d:response><d:href>{href}</d:href><d:propstat><d:prop><d:getetag>{etag}</d:getetag></d:prop></d:propstat></d:response>"));
                        }
                        for folder in folders.iter().filter(|f| f.rsplit_once('/').map(|(p, _)| p) == Some(path.as_str())) {
                            xml.push_str(&format!("<d:response><d:href>{folder}/</d:href></d:response>"));
                        }
                        xml.push_str("</d:multistatus>");
                        Response::from_data(xml.into_bytes()).with_status_code(207)
                    }
                }
                _ => Response::from_data(Vec::new()).with_status_code(405),
            };
            drop((files, folders));
            let _ = req.respond(response);
        }
    });
    format!("http://{addr}{ROOT}/")
}

fn location(url: &str) -> WebDavLocation {
    WebDavLocation { url: url.to_string(), username: "me".into() }
}

#[test]
fn objects_round_trip_with_conditional_writes() {
    let url = serve(Arc::default());
    let store = WebDavStore::new(&location(&url), "app-password").unwrap();
    assert!(store.list("").unwrap().is_empty());
    let Put::Written(v1) = store.put("vault.json", b"one", Cond::IfAbsent).unwrap() else { panic!() };
    assert_eq!(store.put("vault.json", b"again", Cond::IfAbsent).unwrap(), Put::Conflict);
    assert!(matches!(store.put("vault.json", b"two", Cond::IfMatch(v1.clone())).unwrap(), Put::Written(_)));
    assert_eq!(store.put("vault.json", b"stale", Cond::IfMatch(v1)).unwrap(), Put::Conflict);
    assert_eq!(store.get("vault.json").unwrap().unwrap().bytes, b"two");

    // The records folder is made on first write.
    let key = format!("records/{}.json", Uuid::new_v4());
    assert!(matches!(store.put(&key, b"r", Cond::IfAbsent).unwrap(), Put::Written(_)));
    assert_eq!(store.list("").unwrap(), ["vault.json"], "folders aren't listed as objects");
    assert_eq!(store.list("records").unwrap(), std::slice::from_ref(&key));
    store.delete(&key).unwrap();
    assert!(store.get(&key).unwrap().is_none());
}

#[test]
fn a_whole_vault_works_on_webdav_behind_the_cache() {
    let url = serve(Arc::default());
    let cache = tempfile::tempdir().unwrap();
    let remote = WebDavStore::new(&location(&url), "app-password").unwrap();
    let vault = Vault::create(CachedStore::open(remote, cache.path()).unwrap(), "pw", FAST).unwrap();
    let device = Uuid::new_v4();
    let mut env = Environment::new(Uuid::new_v4(), "production", vault.stamp(device));
    env.set("API_KEY", "sk_webdav", vault.stamp(device));
    vault.put(Record::Environment(env.clone())).unwrap();
    vault.sync().unwrap();

    // Another machine, with its own cache, sees it.
    let other_cache = tempfile::tempdir().unwrap();
    let remote = WebDavStore::new(&location(&url), "app-password").unwrap();
    let other = Vault::unlock(CachedStore::open(remote, other_cache.path()).unwrap(), "pw").unwrap();
    let Some(Record::Environment(seen)) = other.get(env.id).unwrap() else { panic!() };
    assert_eq!(seen.value("API_KEY"), Some("sk_webdav"));
}

#[test]
fn wrong_passwords_and_plain_http_are_refused() {
    let url = serve(Arc::default());
    let store = WebDavStore::new(&location(&url), "wrong").unwrap();
    let err = store.list("").unwrap_err().to_string();
    assert!(err.contains("refused the username or password"), "{err}");
    assert!(WebDavStore::new(&location("http://cloud.example.com/dav/"), "x").is_err(), "https only, except localhost");
}
