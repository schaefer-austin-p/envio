//! The Graph client's read calls: folder tree, delta pages, MIME and
//! categories, search and the `Message-ID` lookup.

use serde_json::Value;

use super::client::{GraphClient, Outcome, Spec};
use super::error::Op;
use super::model::{DeltaPage, GFolder, GMeta};
use super::parse;
use super::path::{enc, link_path};
use crate::{NetError, Result};

const SELECT: &str = "$select=receivedDateTime,isRead,flag,categories,isDraft";
/// Folder tree guards: Graph nests mail folders at most 10 deep.
const MAX_FOLDER_DEPTH: usize = 20;
const MAX_FOLDERS: usize = 5000;
/// Pages followed at most in one list call (a runaway link guard).
const MAX_PAGES: usize = 1000;

/// A parsed list page and its next link.
type PageOf<T> = Result<(Vec<T>, Option<String>)>;

impl GraphClient {
    /// Follows `nextLink`s from `first`, collecting each page.
    async fn pages<T>(&mut self, first: String, page: fn(&[u8]) -> PageOf<T>) -> Result<Vec<T>> {
        let (mut out, mut path) = (Vec::new(), first);
        for _ in 0..MAX_PAGES {
            let body = self
                .body(&Spec::get(Op::Read, path))
                .await?
                .ok_or_else(|| NetError::Refused("HTTP 404".into()))?;
            let (items, next) = page(&body)?;
            out.extend(items);
            match next {
                Some(link) => path = link_path(&link)?,
                None => return Ok(out),
            }
        }
        Err(NetError::Protocol("too many Graph pages".into()))
    }

    /// Every folder, hidden and nested ones included. A folder seen twice
    /// (by id) is kept once and not walked again; the walk stops at
    /// `MAX_FOLDER_DEPTH` levels and `MAX_FOLDERS` folders.
    pub async fn folders(&mut self) -> Result<Vec<GFolder>> {
        let q = "includeHiddenFolders=true&$top=100";
        let top = self
            .pages(format!("/v1.0/me/mailFolders?{q}"), parse::folders)
            .await?;
        let mut seen = std::collections::HashSet::new();
        let mut all: Vec<(GFolder, usize)> = Vec::new();
        for f in top {
            if seen.insert(f.id.clone()) {
                all.push((f, 1));
            }
        }
        let mut at = 0;
        while at < all.len() {
            let (id, depth, kids) = (all[at].0.id.clone(), all[at].1, all[at].0.child_count);
            at += 1;
            if kids == 0 || depth >= MAX_FOLDER_DEPTH {
                continue;
            }
            let path = format!("/v1.0/me/mailFolders/{}/childFolders?{q}", enc(&id));
            for f in self.pages(path, parse::folders).await? {
                if all.len() >= MAX_FOLDERS {
                    return Err(NetError::Protocol("too many Graph folders".into()));
                }
                if seen.insert(f.id.clone()) {
                    all.push((f, depth + 1));
                }
            }
        }
        Ok(all.into_iter().map(|(f, _)| f).collect())
    }

    /// One delta page: the first for `folder_id` (`link` is `None`), or the
    /// one `link` names.
    pub async fn delta(&mut self, folder_id: &str, link: Option<&str>) -> Result<DeltaPage> {
        let path = match link {
            Some(l) => link_path(l)?,
            None => format!(
                "/v1.0/me/mailFolders/{}/messages/delta?{SELECT}",
                enc(folder_id)
            ),
        };
        let mut spec = Spec::get(Op::Delta, path);
        spec.headers
            .push(("Prefer", "odata.maxpagesize=200".into()));
        match self.exec(&spec).await? {
            Outcome::Ok(r) => parse::delta(r.status, &r.body),
            Outcome::Expired => Ok(DeltaPage::Expired),
            Outcome::Gone => Err(NetError::Refused("HTTP 404, folder not found".into())),
        }
    }

    /// The message's MIME; `None` if it is gone.
    pub async fn message_mime(&mut self, id: &str) -> Result<Option<Vec<u8>>> {
        let path = format!("/v1.0/me/messages/{}/$value", enc(id));
        self.body(&Spec::get(Op::Read, path)).await
    }

    /// The message's categories; `None` if it is gone.
    pub async fn get_categories(&mut self, id: &str) -> Result<Option<Vec<String>>> {
        let path = format!("/v1.0/me/messages/{}?$select=categories", enc(id));
        let Some(body) = self.body(&Spec::get(Op::Read, path)).await? else {
            return Ok(None);
        };
        let v: Value = serde_json::from_slice(&body)
            .map_err(|_| NetError::Protocol("Graph reply is not JSON".into()))?;
        let list = v["categories"].as_array().map(|l| {
            l.iter()
                .filter_map(|c| c.as_str().map(str::to_owned))
                .collect()
        });
        Ok(Some(list.unwrap_or_default()))
    }

    /// `$search` over one folder, all pages.
    pub async fn search(&mut self, folder_id: &str, query: &str) -> Result<Vec<GMeta>> {
        let quoted = format!("\"{}\"", query.replace('\\', "\\\\").replace('"', "\\\""));
        let path = format!(
            "/v1.0/me/mailFolders/{}/messages?$search={}&$top=50&{SELECT}",
            enc(folder_id),
            enc(&quoted)
        );
        self.pages(path, parse::search).await
    }

    /// Messages in a folder with this `Message-ID` (with its angle
    /// brackets), to find a draft or a sent message after a lost reply.
    pub async fn find_message_id(&mut self, folder_id: &str, mid: &str) -> Result<Vec<GMeta>> {
        let filter = format!("internetMessageId eq '{}'", mid.replace('\'', "''"));
        let path = format!(
            "/v1.0/me/mailFolders/{}/messages?$filter={}&{SELECT}",
            enc(folder_id),
            enc(&filter)
        );
        self.pages(path, parse::search).await
    }
}
