//! Opening a big message without downloading it whole: BODYSTRUCTURE says
//! what the parts are, a pure planner picks the text and the small inline
//! images, one FETCH gets them, and they are put back into a smaller message
//! whose other parts are stubs (see [`assemble`]). An attachment is fetched
//! later, on its own, by [`ImapSession::fetch_section`].

mod assemble;
mod plan;
mod structure;

use std::collections::HashMap;

use async_imap::imap_proto::{MessageSection, SectionPath};
use futures::TryStreamExt;

use super::ImapSession;
use super::client::map_error;
use crate::connect;
use crate::error::Result;

use assemble::{Fetched, assemble};
use plan::plan;
use structure::{Kind, Node, from_proto};

pub use plan::TEXT_CAP;

/// Most parts asked for in one FETCH, so the command line stays short.
const PARTS_PER_FETCH: usize = 40;
/// What a FETCH reply adds beyond the parts: headers and framing.
const FRAMING_BYTES: usize = 64 * 1024;

fn section_path(section: &str, tail: Option<MessageSection>) -> Option<SectionPath> {
    let numbers = section
        .split('.')
        .map(str::parse)
        .collect::<std::result::Result<Vec<u32>, _>>()
        .ok()?;
    Some(SectionPath::Part(numbers, tail))
}

impl ImapSession {
    /// The message with only its text and small inline images, the rest
    /// stubs. None if it's gone. The folder is selected; \Seen is not set.
    pub async fn fetch_partial(&mut self, uid: u32) -> Result<Option<Vec<u8>>> {
        let Some(root) = self.fetch_structure(uid).await? else {
            return Ok(None);
        };
        let plan = plan(&root);
        let mut fetched = Fetched::new();
        let header = self.fetch_header(uid).await?;
        let Some(header) = header else {
            return Ok(None);
        };
        let multipart = matches!(root.kind, Kind::Multipart { .. });
        let mut wanted: Vec<(String, u32, bool)> = Vec::new();
        for (section, limit) in &plan.fetch {
            wanted.push((section.clone(), *limit, true));
        }
        for section in &plan.stubs {
            // The stub keeps its headers, not its body.
            wanted.push((section.clone(), 0, false));
        }
        // The only part of a single-part message has the message's headers.
        if multipart {
            for chunk in wanted.chunks(PARTS_PER_FETCH) {
                self.fetch_parts(uid, chunk, &mut fetched).await?;
            }
        } else {
            for chunk in wanted.chunks(PARTS_PER_FETCH) {
                let bodies: Vec<_> = chunk.iter().filter(|w| w.2).cloned().collect();
                self.fetch_bodies_only(uid, &bodies, &mut fetched).await?;
            }
        }
        Ok(Some(assemble(&root, &header, &fetched)))
    }

    /// One part as a message of its own: its headers, then its (still
    /// transfer-encoded) body. None if the message or the part is gone.
    /// `size` is the part's size, for the time limit.
    pub async fn fetch_section(
        &mut self,
        uid: u32,
        section: &str,
        size: usize,
    ) -> Result<Option<Vec<u8>>> {
        let (Some(mime), Some(body)) = (
            section_path(section, Some(MessageSection::Mime)),
            section_path(section, None),
        ) else {
            return Ok(None);
        };
        let items = format!("(UID BODY.PEEK[{section}.MIME] BODY.PEEK[{section}])");
        connect::within_for(connect::transfer_timeout(size), async {
            let fetches: Vec<_> = self
                .session
                .uid_fetch(uid.to_string(), &items)
                .await
                .map_err(map_error)?
                .try_collect()
                .await
                .map_err(map_error)?;
            let Some(fetch) = fetches.iter().find(|f| f.uid == Some(uid)) else {
                return Ok(None);
            };
            let (Some(mime), Some(body)) = (fetch.section(&mime), fetch.section(&body)) else {
                return Ok(None);
            };
            let mut out = mime.to_vec();
            out.extend_from_slice(body);
            Ok(Some(out))
        })
        .await
    }

    async fn fetch_structure(&mut self, uid: u32) -> Result<Option<Node>> {
        connect::within(async {
            let fetches: Vec<_> = self
                .session
                .uid_fetch(uid.to_string(), "(UID BODYSTRUCTURE)")
                .await
                .map_err(map_error)?
                .try_collect()
                .await
                .map_err(map_error)?;
            Ok(fetches
                .iter()
                .find(|f| f.uid == Some(uid))
                .and_then(|f| f.bodystructure())
                .map(from_proto))
        })
        .await
    }

    async fn fetch_header(&mut self, uid: u32) -> Result<Option<Vec<u8>>> {
        connect::within_for(connect::transfer_timeout(FRAMING_BYTES), async {
            let fetches: Vec<_> = self
                .session
                .uid_fetch(uid.to_string(), "(UID BODY.PEEK[HEADER])")
                .await
                .map_err(map_error)?
                .try_collect()
                .await
                .map_err(map_error)?;
            Ok(fetches
                .iter()
                .find(|f| f.uid == Some(uid))
                .and_then(|f| f.header())
                .map(<[u8]>::to_vec))
        })
        .await
    }

    /// The headers of each part in `parts` and the body of those marked
    /// wanted (cut to the limit), into `out`.
    async fn fetch_parts(
        &mut self,
        uid: u32,
        parts: &[(String, u32, bool)],
        out: &mut Fetched,
    ) -> Result<()> {
        let mut items = String::from("(UID");
        let mut bytes = FRAMING_BYTES;
        for (section, limit, want) in parts {
            items.push_str(&format!(" BODY.PEEK[{section}.MIME]"));
            if *want && *limit > 0 {
                items.push_str(&format!(" BODY.PEEK[{section}]<0.{limit}>"));
                bytes += *limit as usize;
            }
        }
        items.push(')');
        self.fetch_items(uid, &items, bytes, parts, out).await
    }

    /// The bodies of the wanted parts only (the headers of a single-part
    /// message are the message's own).
    async fn fetch_bodies_only(
        &mut self,
        uid: u32,
        parts: &[(String, u32, bool)],
        out: &mut Fetched,
    ) -> Result<()> {
        let mut items = String::from("(UID");
        let mut bytes = FRAMING_BYTES;
        for (section, limit, _) in parts.iter().filter(|p| p.1 > 0) {
            items.push_str(&format!(" BODY.PEEK[{section}]<0.{limit}>"));
            bytes += *limit as usize;
        }
        items.push(')');
        if items == "(UID)" {
            // Only empty parts: nothing to ask the server.
            out.extend(parts.iter().map(|(s, _, _)| (s.clone(), Vec::new())));
            return Ok(());
        }
        self.fetch_items(uid, &items, bytes, parts, out).await
    }

    async fn fetch_items(
        &mut self,
        uid: u32,
        items: &str,
        bytes: usize,
        parts: &[(String, u32, bool)],
        out: &mut Fetched,
    ) -> Result<()> {
        let got: HashMap<String, Vec<u8>> =
            connect::within_for(connect::transfer_timeout(bytes), async {
                let fetches: Vec<_> = self
                    .session
                    .uid_fetch(uid.to_string(), items)
                    .await
                    .map_err(map_error)?
                    .try_collect()
                    .await
                    .map_err(map_error)?;
                let mut got = HashMap::new();
                let Some(fetch) = fetches.iter().find(|f| f.uid == Some(uid)) else {
                    return Ok(got);
                };
                for (section, limit, want) in parts {
                    if *want && *limit == 0 {
                        got.insert(section.clone(), Vec::new());
                    }
                    if let Some(path) = section_path(section, Some(MessageSection::Mime))
                        && let Some(data) = fetch.section(&path)
                    {
                        got.insert(format!("{section}.MIME"), data.to_vec());
                    }
                    if *want
                        && let Some(path) = section_path(section, None)
                        && let Some(data) = fetch.section(&path)
                    {
                        got.insert(section.clone(), data.to_vec());
                    }
                }
                Ok(got)
            })
            .await?;
        out.extend(got);
        Ok(())
    }
}
