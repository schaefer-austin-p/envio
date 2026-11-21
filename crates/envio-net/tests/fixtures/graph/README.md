# Graph fixtures

Redacted replies from Microsoft Graph, recorded on 2026-11-21 by
`cargo run -p envio-net --example graph_record` on a throwaway Outlook.com
account. Addresses, names, subjects, ids, links and tokens are fakes
(stable across files), bodies and previews are dropped, `internetMessageHeaders`
keep only From, To, Cc, Subject, Date, Message-ID, In-Reply-To, References,
MIME-Version and Content-Type. Collections keep at most 5 items. Error files
are named by HTTP status.

What the recorder did to the mailbox, all undone: marked one Inbox message read
(or unread) and back; created one draft, "envio fixture recorder (temporary)", and deleted it for good.
Everything else was a read.

## Files

- `batch.json`
- `delta_after_read.json`
- `delta_final.json`
- `delta_first.json`
- `delta_removed.json`
- `error_400.json`
- `find_message_id_inbox.json`
- `folders.json`
- `folders_children.json`
- `id_token_claims.json`
- `search_page1.json`
- `search_page2.json`

## Results

- `delta_next.json` is missing: the Inbox fits in two pages.
- `$batch` worked (200 with a reply per request, matched by `id`, immutable ids and the size property came back), but no sub-404 was provoked: the draft still answered 200 right after `permanentDelete`. The 404 probe therefore gave 200 and its body is not kept.
- Decision: T5 `summaries` uses `$batch`; the sub-404 path is tested from a hand-written case, not a fixture.
- `delta_removed.json` shows the deleted draft as an `@removed` item with reason `deleted`.
- No `error_404.json`: a deleted message did not give a 404 in time; the 404 body in tests is hand-written from the documented shape.
- `find_message_id` on inbox: 1 match(es) for the id with its angle brackets.
- `find_message_id_sent.json` is missing: that folder has no message.
- `delta_expired_*.json` is missing: a mangled delta token gave HTTP 400, not an expiry.
