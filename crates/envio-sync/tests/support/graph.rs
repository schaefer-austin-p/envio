//! Helpers for the `GraphServer` tests: a token source, the scripted Graph
//! server's replies, and a `GraphServer` pointed at it. Example.org
//! addresses only.

use std::path::Path;

use envio_net::graph::mock::{Action, HOST, Seen, Server};
use envio_net::graph::{Bearer, GraphClient};
use envio_store::GraphMap;
use envio_sync::driver::sync_folders;
use envio_sync::graph::GraphServer;
use envio_sync::server::MailServer;
use futures::future::LocalBoxFuture;
use secrecy::SecretString;

use super::local::{ACCOUNT, Store};

pub struct Tok;

impl Bearer for Tok {
    fn bearer(&self, _force: bool) -> LocalBoxFuture<'_, envio_net::Result<SecretString>> {
        Box::pin(async { Ok(SecretString::from("tok")) })
    }
}

pub fn graph_server(server: &Server, db_path: &Path) -> GraphServer {
    let client = GraphClient::with_host(HOST, server.port(), server.roots(), Box::new(Tok));
    GraphServer::new(client, GraphMap::open(db_path).unwrap())
}

/// The folder walk of a small mailbox: Inbox (with a child), Drafts, Sent, a
/// hidden folder, a contacts folder and one whose name has `/` and `.`.
/// Two replies: the top level, then Inbox's children.
pub fn folder_walk(inbox_id: &str) -> Vec<Action> {
    let f = |id: &str, name: &str, known: Option<&str>, kids: u32, extra: &str| {
        let known = known.map_or(String::new(), |k| format!(r#""wellKnownName":"{k}","#));
        format!(
            r#"{{"id":"{id}","displayName":"{name}","parentFolderId":"ROOT",{known}"childFolderCount":{kids}{extra}}}"#
        )
    };
    let top = [
        f(inbox_id, "Inbox", Some("inbox"), 1, ""),
        f("F-DRAFTS", "Drafts", Some("drafts"), 0, ""),
        f("F-SENT", "Sent Items", Some("sentitems"), 0, ""),
        f("F-HID", "Hidden", None, 0, r#","isHidden":true"#),
        f(
            "F-CON",
            "Contacts",
            None,
            0,
            r#","folderClass":"IPF.Contact""#,
        ),
        f("F-ODD", "A/B.c", None, 0, r#","folderClass":"IPF.Note""#),
    ]
    .join(",");
    let child = format!(
        r#"{{"id":"F-CHILD","displayName":"Child","parentFolderId":"{inbox_id}","childFolderCount":0}}"#
    );
    vec![
        Action::json(200, &format!(r#"{{"value":[{top}]}}"#)),
        Action::json(200, &format!(r#"{{"value":[{child}]}}"#)),
    ]
}

/// One delta item with every property.
pub fn msg(id: &str, received: &str, read: bool) -> String {
    format!(
        r#"{{"id":"{id}","isDraft":false,"isRead":{read},"receivedDateTime":"{received}","flag":{{"flagStatus":"notFlagged"}},"categories":[]}}"#
    )
}

/// A delta page that says more follows (`next`) or is the last (`done`).
pub fn page(items: &[String], next: bool, token: &str) -> Action {
    let key = if next { "nextLink" } else { "deltaLink" };
    let link = format!(
        "https://graph.microsoft.com/v1.0/me/mailFolders('F')/messages/delta?$token={token}"
    );
    Action::json(
        200,
        &format!(
            r#"{{"@odata.{key}":"{link}","value":[{}]}}"#,
            items.join(",")
        ),
    )
}

pub fn requests_with(server: &Server, needle: &str) -> Vec<Seen> {
    server
        .requests()
        .into_iter()
        .filter(|r| r.path.contains(needle))
        .collect()
}

/// A plain `{"value": [...]}` reply (search and filter results).
pub fn list(items: &[String]) -> Action {
    Action::json(200, &format!(r#"{{"value":[{}]}}"#, items.join(",")))
}

/// One 200 sub-reply of a `$batch` summary read; `n` is the sub-request id.
#[allow(clippy::too_many_arguments)]
pub fn sub_ok(n: usize, id: &str, received: &str, size: u32, subject: &str, mid: &str) -> String {
    let h = |name: &str, value: &str| format!(r#"{{"name":"{name}","value":"{value}"}}"#);
    let headers = [
        h("From", "Alice <alice@example.org>"),
        h("To", "Bob <bob@example.org>"),
        h("Subject", subject),
        h("Date", "Sat, 21 Nov 2026 00:00:00 +0000"),
        h("Message-ID", mid),
    ]
    .join(",");
    format!(
        r#"{{"id":"{n}","status":200,"body":{{"id":"{id}","receivedDateTime":"{received}","internetMessageHeaders":[{headers}],"singleValueExtendedProperties":[{{"id":"Integer 0xe08","value":"{size}"}}]}}}}"#
    )
}

/// A 404 sub-reply: the message is gone.
pub fn sub_gone(n: usize) -> String {
    format!(r#"{{"id":"{n}","status":404,"body":{{"error":{{"code":"ErrorItemNotFound"}}}}}}"#)
}

pub fn batch(subs: &[String]) -> Action {
    Action::json(200, &format!(r#"{{"responses":[{}]}}"#, subs.join(",")))
}

pub fn categories(names: &[&str]) -> Action {
    let list: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
    Action::json(200, &format!(r#"{{"categories":[{}]}}"#, list.join(",")))
}

/// A server on a synced Inbox: folders listed, `items` drained and
/// committed, the Inbox selected.
pub struct Synced {
    pub server: Server,
    pub store: Store,
    pub gs: GraphServer,
    pub map: GraphMap,
    pub inbox: envio_store::FolderId,
}

pub async fn synced(items: &[String]) -> Synced {
    let mut script = folder_walk("F-INBOX");
    script.push(page(items, false, "d1"));
    let server = Server::start(script).await;
    let mut store = Store::on_file();
    let mut gs = graph_server(&server, &store.db_path);
    sync_folders(&mut gs, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let inbox = store.folder("INBOX").id;
    gs.refresh("INBOX").await.unwrap();
    store.db.step(|db| gs.commit_folder(db, inbox)).unwrap();
    gs.select("INBOX").await.unwrap();
    let map = GraphMap::open(&store.db_path).unwrap();
    Synced {
        server,
        store,
        gs,
        map,
        inbox,
    }
}
