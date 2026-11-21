//! Records Microsoft Graph replies from a test account as redacted fixtures
//! for `envio-net`'s Graph parsers and client tests. A dev tool, not part of
//! the app. Two steps, so a person can sign in between them:
//!
//! ```text
//! ENVIO_MS_CLIENT_ID=<client id> cargo run -p envio-net --example graph_record -- url
//! ENVIO_MS_CLIENT_ID=<client id> cargo run -p envio-net --example graph_record -- record '<redirect url>'
//! ```
//!
//! `url` prints the sign-in URL and saves the PKCE verifier and state to a
//! file (mode 0600) under `$XDG_RUNTIME_DIR` (else the target directory).
//! After signing in, the browser fails to open `enviooauth://callback?...`;
//! pass that whole address to `record`, which checks the state, signs in,
//! deletes the file, records the replies and writes
//! `tests/fixtures/graph/*.json` and a README there. Nothing is written when
//! the final check finds anything that could identify the account.
//!
//! What it does to the mailbox, all of it minimal and undone before it ends:
//! - marks one existing Inbox message read (or unread, if it was read) and
//!   then back to how it was;
//! - creates one draft in Drafts, "envio fixture recorder (temporary)", to
//!   nobody real, then deletes that draft for good (`permanentDelete`).
//!
//! Everything else is a read. The access token stays in memory; no token,
//! body, address or id_token is ever printed, only step names, HTTP status
//! and Graph error codes.

#[path = "graph_record/api.rs"]
mod api;
#[path = "graph_record/check.rs"]
mod check;
#[path = "graph_record/redact.rs"]
mod redact;
#[path = "graph_record/steps.rs"]
mod steps;

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use envio_net::graph::http::Http;
use envio_net::oauth::{
    CONSUMER_TENANT, Pkce, Provider, authorize_url, exchange_code, parse_redirect,
};
use envio_net::tls::Roots;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

// `User.Read` is for GET /me, which only this recorder calls. If the Graph
// backend (T16) ever calls /me it needs User.Read too.
const SCOPES: &str = "Mail.ReadWrite Mail.Send User.Read offline_access openid email profile";

fn provider() -> Result<Provider, String> {
    let id = std::env::var("ENVIO_MS_CLIENT_ID").unwrap_or_default();
    if id.trim().is_empty() {
        return Err("set ENVIO_MS_CLIENT_ID".into());
    }
    let mut p = Provider::microsoft_template().with_client_id(id.trim());
    // T16's `Provider::microsoft_graph()` replaces this override.
    p.scopes = SCOPES;
    Ok(p)
}

fn verifier_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|d| d.is_dir())
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    dir.join("envio-graph-record.json")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/graph")
}

fn save_verifier(pkce: &Pkce) -> Result<(), String> {
    let path = verifier_path();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("cannot create the verifier file: {e}"))?;
    // An older file may have had looser rights.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    let text = json!({"verifier": pkce.verifier.expose_secret(), "state": pkce.state});
    file.write_all(text.to_string().as_bytes())
        .map_err(|e| e.to_string())
}

/// Days since 1970-01-01 to a UTC calendar date.
fn utc_date(secs: u64) -> String {
    let z = i64::try_from(secs / 86_400).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn readme(names: &[String], notes: &[String], date: &str) -> String {
    let keep = api::KEEP;
    let subject = api::DRAFT_SUBJECT;
    let mut out = format!(
        "# Graph fixtures\n\nRedacted replies from Microsoft Graph, recorded on {date} by\n`cargo run -p envio-net --example graph_record` on a throwaway Outlook.com\naccount. Addresses, names, subjects, ids, links and tokens are fakes\n(stable across files), bodies and previews are dropped, `internetMessageHeaders`\nkeep only From, To, Cc, Subject, Date, Message-ID, In-Reply-To, References,\nMIME-Version and Content-Type. Collections keep at most {keep} items. Error files\nare named by HTTP status.\n\nWhat the recorder did to the mailbox, all undone: marked one Inbox message read\n(or unread) and back; created one draft, \"{subject}\", and deleted it for good.\nEverything else was a read.\n\n## Files\n\n"
    );
    for n in names {
        out.push_str(&format!("- `{n}`\n"));
    }
    out.push_str("\n## Results\n\n");
    for note in notes {
        out.push_str(&format!("- {note}\n"));
    }
    out
}

/// The verifier and state that `url` saved. The file is deleted at once:
/// the code is single use, so it is of no further use.
fn take_saved() -> Result<(String, String), String> {
    let path = verifier_path();
    let text = std::fs::read_to_string(&path).map_err(|_| "no saved sign-in: run `url` first")?;
    let _ = std::fs::remove_file(&path);
    let saved: Value =
        serde_json::from_str(&text).map_err(|_| "the saved sign-in is damaged".to_owned())?;
    let get = |k: &str| saved[k].as_str().unwrap_or_default().to_owned();
    Ok((get("verifier"), get("state")))
}

/// Signs in with the redirect and returns the API client and the account's
/// own names. Refuses anything but a personal Microsoft account.
async fn sign_in(redirect: &str) -> Result<(api::Api, check::Own), String> {
    let p = provider()?;
    let (verifier, state) = take_saved()?;
    let parsed = parse_redirect(redirect, &state, &p.redirect_uri)
        .map_err(|e| format!("the redirect was not accepted: {e:?}"))?;
    let roots = Roots::system_or_bundled();
    let tokens = exchange_code(&p, &roots, &parsed.code, &SecretString::from(verifier))
        .await
        .map_err(|e| format!("sign-in failed: {e:?}"))?;
    let id = tokens.identity.ok_or("sign-in gave no identity")?;
    if id.tid.as_deref() != Some(CONSUMER_TENANT) {
        return Err("the account is not a personal Microsoft account (tid)".into());
    }
    let mut own = check::Own::default();
    own.address
        .extend(id.email.into_iter().chain(id.preferred_username));
    let api = api::Api {
        http: Http::new("graph.microsoft.com", 443, roots),
        token: tokens.access,
    };
    Ok((api, own))
}

/// Redacts, checks and writes the files. Nothing is written if any file
/// fails the check.
fn write_out(rec: steps::Rec) -> Result<(), String> {
    let mut redactor = redact::Redactor::default();
    let mut out = Vec::new();
    for (name, mut v) in rec.files {
        redactor.value(&mut v);
        let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
        out.push((name, text + "\n"));
    }
    // Only the tenant, and only because `sign_in` checked it is the consumer
    // one; added after redaction, which would turn the GUID into a fake.
    let claims = json!({"tid": CONSUMER_TENANT});
    out.push(("id_token_claims.json".into(), format!("{claims:#}\n")));
    out.sort();
    let names: Vec<String> = out.iter().map(|(n, _)| n.clone()).collect();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    let date = utc_date(now.map_or(0, |d| d.as_secs()));
    out.push(("README.md".into(), readme(&names, &rec.notes, &date)));
    for (name, text) in &out {
        check::refuse(text, &rec.own, CONSUMER_TENANT)
            .map_err(|r| format!("refusing to write: {name} still holds {}", r.describe()))?;
    }
    let dir = fixtures_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for (name, text) in &out {
        std::fs::write(dir.join(name), text).map_err(|e| e.to_string())?;
    }
    eprintln!("wrote {} files to tests/fixtures/graph", out.len());
    Ok(())
}

async fn record(redirect: &str) -> Result<(), String> {
    let (api, own) = sign_in(redirect).await?;
    let mut rec = steps::Rec::new(api, own);
    let result = rec.run().await;
    rec.undo.run(&mut rec.api).await;
    result?;
    write_out(rec)
}

fn print_url() -> Result<(), String> {
    let p = provider()?;
    let pkce = Pkce::new();
    save_verifier(&pkce)?;
    let url = authorize_url(&p, &pkce, "");
    println!("{}", url.strip_suffix("&login_hint=").unwrap_or(&url));
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["url"] => print_url(),
        ["record", redirect] => record(redirect).await,
        _ => Err("usage: graph_record url | graph_record record '<redirect url>'".into()),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
