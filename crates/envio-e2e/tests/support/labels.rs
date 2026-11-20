//! Server-side keyword helpers for the draft tests.

use envio_net::imap::Flag;

use super::core_run::password;
use super::drafts::USER;
use super::stalwart::Stalwart;

/// The UID of the live message in `drafts` whose header has `needle`.
pub async fn uid_of(server: &Stalwart, drafts: &str, needle: &str) -> u32 {
    let creds = envio_net::Credentials::new(USER, &password(USER));
    let mut session = server.imap(&creds).await;
    let state = session.select(drafts).await.unwrap();
    let all: Vec<u32> = (1..state.uid_next).collect();
    let found = session
        .fetch_summaries(&all)
        .await
        .unwrap()
        .iter()
        .find(|m| !m.flags.deleted && String::from_utf8_lossy(&m.header).contains(needle))
        .map(|m| m.uid);
    session.logout().await.unwrap();
    found.unwrap_or_else(|| panic!("no draft with {needle} in {drafts}"))
}

/// Adds or removes the keyword `$Label1` on the server's copy.
pub async fn label(server: &Stalwart, drafts: &str, uid: u32, add: bool) {
    let creds = envio_net::Credentials::new(USER, &password(USER));
    let mut session = server.imap(&creds).await;
    session.select(drafts).await.unwrap();
    let flags = [Flag::Keyword("$Label1".to_owned())];
    session.store_flags(&[uid], add, &flags).await.unwrap();
    session.logout().await.unwrap();
}
