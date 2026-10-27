//! AWS Signature Version 4, for the IAM and STS query APIs. The primitives
//! (HMAC-SHA256, SHA-256) come from `dotsafe_core::crypto`; this module only
//! arranges the request the way AWS specifies.

use dotsafe_core::crypto::{hmac_sha256, sha256_hex};

/// A request to sign: a POST with a form body, as IAM and STS take.
pub struct Request<'a> {
    pub host: &'a str,
    pub region: &'a str,
    pub service: &'a str,
    pub body: &'a str,
    /// `YYYYMMDD'T'HHMMSS'Z'`, UTC.
    pub amz_date: &'a str,
}

pub const CONTENT_TYPE: &str = "application/x-www-form-urlencoded; charset=utf-8";

/// The `Authorization` header for `request`, signed with this access key.
pub fn authorization(request: &Request, access_key_id: &str, secret_access_key: &str) -> String {
    signed("POST", "", request, access_key_id, secret_access_key)
}

fn signed(method: &str, query: &str, r: &Request, access_key_id: &str, secret: &str) -> String {
    let date = &r.amz_date[..8];
    let signed_headers = "content-type;host;x-amz-date";
    let canonical = [
        method,
        "/",
        query,
        &format!("content-type:{CONTENT_TYPE}"),
        &format!("host:{}", r.host),
        &format!("x-amz-date:{}", r.amz_date),
        "",
        signed_headers,
        &sha256_hex(r.body.as_bytes()),
    ]
    .join("\n");
    let scope = format!("{date}/{}/{}/aws4_request", r.region, r.service);
    let to_sign = ["AWS4-HMAC-SHA256", r.amz_date, &scope, &sha256_hex(canonical.as_bytes())].join("\n");
    let key = signing_key(secret, date, r.region, r.service);
    let signature: String = hmac_sha256(&key, to_sign.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    format!("AWS4-HMAC-SHA256 Credential={access_key_id}/{scope}, SignedHeaders={signed_headers}, Signature={signature}")
}

fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> [u8; 32] {
    let k = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k = hmac_sha256(&k, region.as_bytes());
    let k = hmac_sha256(&k, service.as_bytes());
    hmac_sha256(&k, b"aws4_request")
}

/// A form body: `key=value` pairs, percent-encoded the way SigV4 expects
/// (unreserved characters kept, everything else `%XX`, space as `%20`).
pub fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect::<Vec<_>>().join("&")
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            b => format!("%{b:02X}"),
        })
        .collect()
}

/// Unix seconds as SigV4's `YYYYMMDD'T'HHMMSS'Z'`.
pub fn amz_date(unix_secs: u64) -> String {
    let (days, rem) = ((unix_secs / 86_400) as i64, unix_secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AWS's published example ("Create a signed AWS API request"): ListUsers
    /// on IAM, signed with the documentation's example secret key.
    #[test]
    fn matches_the_aws_documentation_example() {
        let request = Request {
            host: "iam.amazonaws.com",
            region: "us-east-1",
            service: "iam",
            body: "",
            amz_date: "20150830T123600Z",
        };
        assert_eq!(
            hex(&signing_key("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY", "20150830", "us-east-1", "iam")),
            "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9"
        );
        let header = signed(
            "GET",
            "Action=ListUsers&Version=2010-05-08",
            &request,
            "AKIDEXAMPLE",
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
        );
        assert_eq!(
            header,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request, \
             SignedHeaders=content-type;host;x-amz-date, \
             Signature=5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"
        );
    }

    #[test]
    fn forms_and_dates() {
        assert_eq!(form(&[("Action", "CreateAccessKey"), ("UserName", "ci bot+1")]), "Action=CreateAccessKey&UserName=ci%20bot%2B1");
        assert_eq!(amz_date(1_440_938_160), "20150830T123600Z");
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
