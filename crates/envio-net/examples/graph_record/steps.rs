//! The recorded steps, in order. Each pushes the replies it wants kept and
//! notes what the README should say about it.

use serde_json::{Value, json};

use super::api::{Api, DRAFT_SUBJECT, Q, Undo, Walk, enc, path_of, trunc, walk};
use super::check::Own;

const SELECT: &str = "$select=receivedDateTime,isRead,flag,categories,isDraft";

pub struct Rec {
    pub api: Api,
    pub undo: Undo,
    pub files: Vec<(String, Value)>,
    pub notes: Vec<String>,
    pub own: Own,
    inbox: String,
    drafts: String,
    sent: String,
}

impl Rec {
    pub fn new(api: Api, own: Own) -> Self {
        Self {
            api,
            own,
            undo: Undo::default(),
            files: Vec::new(),
            notes: Vec::new(),
            inbox: String::new(),
            drafts: String::new(),
            sent: String::new(),
        }
    }

    pub async fn run(&mut self) -> Result<(), String> {
        self.account().await?;
        self.folders().await?;
        let w = self.inbox_delta().await?;
        self.flag_change(&w).await?;
        let draft_id = self.draft_removal().await?;
        self.batch(&w, &draft_id).await?;
        self.search().await?;
        self.find_ids().await?;
        self.errors(&w, &draft_id).await
    }

    fn keep(&mut self, name: &str, v: Value) {
        self.files.push((name.to_owned(), v));
    }

    async fn folder_id(&mut self, name: &str) -> Result<String, String> {
        let v = self
            .api
            .get("folder", &format!("/v1.0/me/mailFolders/{name}?$select=id"))
            .await?;
        v["id"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "folder: no id".into())
    }

    async fn account(&mut self) -> Result<(), String> {
        let path = "/v1.0/me?$select=displayName,mail,userPrincipalName";
        let me = self.api.get("account", path).await?;
        self.own.display_name = me["displayName"].as_str().map(str::to_owned);
        for k in ["mail", "userPrincipalName"] {
            self.own.address.extend(me[k].as_str().map(str::to_owned));
        }
        Ok(())
    }

    async fn folders(&mut self) -> Result<(), String> {
        let path = "/v1.0/me/mailFolders?includeHiddenFolders=true&$top=100";
        let top = self.api.get("folders", path).await?;
        let parent = top["value"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|f| f["childFolderCount"].as_u64().unwrap_or(0) > 0)
            .and_then(|f| f["id"].as_str().map(str::to_owned));
        self.keep("folders.json", top);
        if let Some(id) = parent {
            let path = format!("/v1.0/me/mailFolders/{}/childFolders", enc(&id));
            let kids = self.api.get("childFolders", &path).await?;
            self.keep("folders_children.json", kids);
        } else {
            self.notes
                .push("`folders_children.json` is missing: no folder has subfolders.".into());
        }
        self.inbox = self.folder_id("inbox").await?;
        self.drafts = self.folder_id("drafts").await?;
        self.sent = self.folder_id("sentitems").await?;
        Ok(())
    }

    async fn inbox_delta(&mut self) -> Result<Walk, String> {
        let path = format!(
            "/v1.0/me/mailFolders/{}/messages/delta?{SELECT}",
            enc(&self.inbox)
        );
        let w = walk(&mut self.api, "inbox delta", &path).await?;
        self.keep("delta_first.json", trunc(w.first.clone(), None));
        match &w.second {
            Some(s) => self.keep("delta_next.json", trunc(s.clone(), None)),
            None => self
                .notes
                .push("`delta_next.json` is missing: the Inbox fits in two pages.".into()),
        }
        self.keep("delta_final.json", trunc(w.last.clone(), None));
        Ok(w)
    }

    /// A flag change, recorded and undone.
    async fn flag_change(&mut self, w: &Walk) -> Result<(), String> {
        let Some((id, was_read)) = w.ids.first().cloned() else {
            return Err("the Inbox has no message to record".into());
        };
        let path = format!("/v1.0/me/messages/{}", enc(&id));
        let flip = Q::new("mark read", "PATCH", &path).body(json!({"isRead": !was_read}));
        self.api.ok(flip).await?;
        self.undo.unread_flip = Some((id.clone(), was_read));
        let after = walk(&mut self.api, "delta after read", &path_of(&w.link)?).await?;
        self.keep("delta_after_read.json", trunc(after.first, Some(&id)));
        self.undo.run(&mut self.api).await;
        Ok(())
    }

    /// A removal: a draft this recorder creates and deletes. Returns its id.
    async fn draft_removal(&mut self) -> Result<String, String> {
        let folder = enc(&self.drafts);
        let path = format!("/v1.0/me/mailFolders/{folder}/messages/delta?{SELECT}");
        let base = walk(&mut self.api, "drafts delta", &path).await?;
        let draft = json!({"subject": DRAFT_SUBJECT,
            "body": {"contentType": "text", "content": "temporary"},
            "toRecipients": [{"emailAddress": {"address": "nobody@example.org"}}]});
        let path = format!("/v1.0/me/mailFolders/{folder}/messages");
        let made = self
            .api
            .ok(Q::new("create draft", "POST", &path).body(draft))
            .await?;
        let id = made["id"].as_str().ok_or("create draft: no id")?.to_owned();
        self.undo.draft = Some(id.clone());
        let added = walk(
            &mut self.api,
            "drafts delta after create",
            &path_of(&base.link)?,
        )
        .await?;
        let del = format!("/v1.0/me/messages/{}/permanentDelete", enc(&id));
        self.api.ok(Q::new("delete draft", "POST", &del)).await?;
        self.undo.draft = None;
        let gone = walk(
            &mut self.api,
            "drafts delta after delete",
            &path_of(&added.link)?,
        )
        .await?;
        self.keep("delta_removed.json", trunc(gone.first, Some(&id)));
        Ok(id)
    }

    /// `$batch` with two live messages and the deleted draft.
    async fn batch(&mut self, w: &Walk, draft_id: &str) -> Result<(), String> {
        let mut ids: Vec<&str> = w.ids.iter().take(2).map(|(i, _)| i.as_str()).collect();
        ids.push(draft_id);
        let requests: Vec<Value> = ids.iter().enumerate().map(|(n, id)| json!({
            "id": (n + 1).to_string(), "method": "GET",
            "url": format!("/me/messages/{}?$select=internetMessageHeaders,receivedDateTime&$expand=singleValueExtendedProperties($filter=id eq 'Integer 0x0E08')", enc(id)),
            "headers": {"Prefer": "IdType=\"ImmutableId\""},
        })).collect();
        let q = Q::new("batch", "POST", "/v1.0/$batch").body(json!({"requests": requests}));
        let (status, batch) = self.api.call(q).await?;
        let sub: Vec<u64> = batch["responses"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["status"].as_u64())
            .collect();
        let verdict = match (status, sub.contains(&404)) {
            (200, true) => "worked, including a sub-404 for the deleted draft",
            (200, false) => {
                "worked (200 with a reply per request), but no sub-404 was provoked: \
                 the draft still answered right after `permanentDelete`"
            }
            _ => "did not answer 200",
        };
        self.notes.push(format!(
            "`$batch` {verdict}: HTTP {status}, sub-statuses {sub:?}."
        ));
        self.keep("batch.json", batch);
        Ok(())
    }

    async fn sample(&mut self, folder: &str, fields: &str) -> Result<Value, String> {
        let path = format!(
            "/v1.0/me/mailFolders/{}/messages?$top=1&$select={fields}",
            enc(folder)
        );
        self.api.get("sample", &path).await
    }

    /// `$search`, two pages, on a word from an Inbox subject.
    async fn search(&mut self) -> Result<(), String> {
        let inbox = self.inbox.clone();
        let one = self.sample(&inbox, "subject").await?;
        let subject = one["value"][0]["subject"].as_str().unwrap_or_default();
        let word = subject
            .split(|c: char| !c.is_alphanumeric())
            .find(|w| w.chars().count() >= 4)
            .unwrap_or("mail");
        let path = format!(
            "/v1.0/me/mailFolders/{}/messages?$search=%22{}%22&$top=2&{SELECT}",
            enc(&inbox),
            enc(word)
        );
        let page1 = self.api.get("search", &path).await?;
        if let Some(next) = page1["@odata.nextLink"].as_str() {
            let page2 = self.api.get("search next page", &path_of(next)?).await?;
            self.keep("search_page2.json", trunc(page2, None));
        } else {
            self.notes
                .push("`search_page2.json` is missing: the search had one page.".into());
        }
        self.keep("search_page1.json", trunc(page1, None));
        Ok(())
    }

    /// `find_message_id` on the Inbox and on Sent.
    async fn find_ids(&mut self) -> Result<(), String> {
        for (name, folder) in [("inbox", self.inbox.clone()), ("sent", self.sent.clone())] {
            let one = self.sample(&folder, "internetMessageId").await?;
            let Some(mid) = one["value"][0]["internetMessageId"].as_str() else {
                self.notes.push(format!(
                    "`find_message_id_{name}.json` is missing: that folder has no message."
                ));
                continue;
            };
            let filter = enc(&format!(
                "internetMessageId eq '{}'",
                mid.replace('\'', "''")
            ));
            let path = format!(
                "/v1.0/me/mailFolders/{}/messages?$filter={filter}&{SELECT}",
                enc(&folder)
            );
            let found = self.api.get("find_message_id", &path).await?;
            let hits = found["value"].as_array().map_or(0, Vec::len);
            self.notes.push(format!(
                "`find_message_id` on {name}: {hits} match(es) for the id with its angle brackets."
            ));
            self.keep(&format!("find_message_id_{name}.json"), found);
        }
        Ok(())
    }

    /// Error bodies: 404 (the deleted draft), 400 and an expired delta link.
    async fn errors(&mut self, w: &Walk, draft_id: &str) -> Result<(), String> {
        let gone = format!("/v1.0/me/messages/{}", enc(draft_id));
        let (s404, e404) = self.api.call(Q::new("404", "GET", &gone)).await?;
        let bad = "/v1.0/me/messages?$filter=bogusProperty%20eq%201";
        let (s400, e400) = self.api.call(Q::new("400", "GET", bad)).await?;
        for (status, body) in [(s404, e404), (s400, e400)] {
            if status >= 400 {
                self.keep(&format!("error_{status}.json"), body);
            } else {
                self.notes.push(format!(
                    "an error probe gave HTTP {status}, so no error file was kept for it."
                ));
            }
        }
        let head = w.link.split("$deltatoken=").next().unwrap_or_default();
        let stale = path_of(&format!("{head}$deltatoken=invalid"))?;
        let (sx, ex) = self.api.call(Q::new("stale delta", "GET", &stale)).await?;
        let code = ex["error"]["code"].as_str().unwrap_or_default();
        if sx == 410 || code.contains("syncStateNotFound") {
            self.keep(&format!("delta_expired_{sx}.json"), ex);
        } else {
            self.notes.push(format!(
                "`delta_expired_*.json` is missing: a mangled delta token gave HTTP {sx}, not an expiry."
            ));
        }
        Ok(())
    }
}
