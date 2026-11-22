//! Helpers for the `GraphServer` tests: a token source, the scripted Graph
//! server's replies, and a `GraphServer` pointed at it. Example.org
//! addresses only.

use std::path::Path;

use envio_net::graph::mock::{Action, HOST, Seen, Server};
use envio_net::graph::{Bearer, GraphClient};
use envio_store::GraphMap;
use envio_sync::graph::GraphServer;
use futures::future::LocalBoxFuture;
use secrecy::SecretString;

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
