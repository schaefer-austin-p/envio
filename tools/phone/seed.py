#!/usr/bin/env python3
"""Fills a throwaway test mailbox with synthetic mail for the phone run.

Everything here is invented: the names, companies, amounts and words below
are the whole vocabulary, and the domains are example.org, example.net and
envio.test. A fixed random seed makes the mailbox the same on every run.
The certificate check is off on purpose: the throwaway Stalwart from
crates/envio-net/tests/stalwart/up.sh is self-signed. Do not point this at
a real account.
"""
import argparse
import imaplib
import random
import ssl
import struct
import zlib
from datetime import datetime, timedelta, timezone
from email.message import EmailMessage
from email.utils import format_datetime, formataddr

SEED = 8080

# Invented people. Domains are reserved or test-only.
PEOPLE = [
    ("Lena Ortiz", "lena.ortiz@example.org"),
    ("Sam Okafor", "sam.okafor@example.net"),
    ("Mira Halvorsen", "mira@envio.test"),
    ("Tomas Beck", "tomas.beck@example.org"),
    ("Priya Nandakumar", "priya@example.net"),
    ("Jonas Lindqvist", "jonas@envio.test"),
    ("Aiko Tanabe", "aiko.tanabe@example.org"),
    ("Dara Quinn", "dara@example.net"),
    ("Felix Marchetti", "felix@envio.test"),
    ("Noor Haddad", "noor.haddad@example.org"),
    ("Ines Varga", "ines@example.net"),
    ("Oskar Brandt", "oskar@envio.test"),
]

# Fixed word list for subjects and bodies.
WORDS = (
    "harbour lantern meadow quarterly budget invoice draft schedule garden "
    "kitchen ferry timetable workshop recipe pottery notes review outline "
    "reminder parcel bicycle library picnic roadmap sketch prototype "
    "calendar lunch museum festival moving boxes tickets proposal summary "
    "update agenda minutes folder printer winter harvest orchard chimney "
    "compass blanket violin kettle ladder window postcard"
).split()
OPENERS = ["Quick question about", "Notes on", "Re the", "Thoughts on", "Plan for",
           "Update on", "Following up on", "About the", "Draft of the", "Reminder about"]
COLOURS = ["#1b6ca8", "#a83232", "#2e7d32", "#7b3fa0", "#b36b00"]


def words(rng, n):
    return " ".join(rng.choice(WORDS) for _ in range(n))


def sentence(rng):
    s = words(rng, rng.randint(6, 14))
    return s[0].upper() + s[1:] + "."


def paragraphs(rng, n):
    return [" ".join(sentence(rng) for _ in range(rng.randint(2, 4))) for _ in range(n)]


def png_bytes(rng, size=48):
    colour = bytes(rng.randint(40, 215) for _ in range(3))
    raw = (b"\x00" + colour * size) * size

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def pdf_bytes(text):
    stream = f"BT /F1 14 Tf 72 720 Td ({text}) Tj ET".encode()
    objs = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R "
        b"/Resources << /Font << /F1 5 0 R >> >> >>",
        b"<< /Length %d >>\nstream\n" % len(stream) + stream + b"\nendstream",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    out = b"%PDF-1.4\n"
    offsets = []
    for i, o in enumerate(objs, 1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % i + o + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
    for off in offsets:
        out += b"%010d 00000 n \n" % off
    out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, xref)
    return out


def build(sender, to, subject, when, msgid, text, html=None, in_reply_to=None,
          references=None, attach=None):
    m = EmailMessage()
    m["From"] = formataddr(sender)
    m["To"] = to
    m["Subject"] = subject
    m["Date"] = format_datetime(when)
    m["Message-ID"] = msgid
    if in_reply_to:
        m["In-Reply-To"] = in_reply_to
        m["References"] = " ".join(references or [in_reply_to])
    m.set_content(text)
    if html:
        m.add_alternative(html, subtype="html")
    for name, ctype, data in attach or []:
        main, sub = ctype.split("/")
        m.add_attachment(data, maintype=main, subtype=sub, filename=name)
    return m, when


def html_body(rng, paras, coloured):
    out = []
    for p in paras:
        if coloured and rng.random() < 0.5:
            out.append(f'<p style="color:{rng.choice(COLOURS)}">{p}</p>')
        else:
            out.append(f"<p>{p}</p>")
    return "<html><body>" + "".join(out) + "</body></html>"


def bulk(args, rng, now):
    to = formataddr((args.user.capitalize(), f"{args.user}@envio.test"))
    # Dates are spread over `days`; appended oldest first so UIDs rise with
    # the date, and the newest mail is the first row of the list.
    stamps = sorted(now - timedelta(seconds=rng.uniform(0, args.days * 86400))
                    for _ in range(args.count))
    attach_at = set(rng.sample(range(args.count), min(args.attachments, args.count)))
    threads = []  # (subject, [message ids])
    msgs = []
    for i, when in enumerate(stamps):
        msgid = f"<seed-{i:05d}@envio.test>"
        paras = paragraphs(rng, rng.randint(1, 4))
        text = "\n\n".join(paras) + "\n"
        coloured = rng.random() < 0.5
        html = html_body(rng, paras, coloured) if rng.random() < args.html_share else None
        sender = rng.choice(PEOPLE)
        reply_to = refs = None
        if threads and rng.random() < 0.3:
            subject, ids = rng.choice(threads[-30:])
            reply_to, refs = ids[-1], list(ids)
            ids.append(msgid)
            subject = subject if subject.startswith("Re: ") else "Re: " + subject
        else:
            subject = f"{rng.choice(OPENERS)} {words(rng, rng.randint(1, 3))}"
            threads.append((subject, [msgid]))
        attach = None
        if i in attach_at:
            attach = [(f"{rng.choice(WORDS)}-notes.pdf", "application/pdf",
                       pdf_bytes("Synthetic test file")),
                      (f"{rng.choice(WORDS)}.png", "image/png", png_bytes(rng))]
        msgs.append(build(sender, to, subject, when, msgid, text, html, reply_to, refs, attach))
    return msgs


def listing(args, now):
    """About 15 curated messages for the store screenshots."""
    to = formataddr((args.user.capitalize(), f"{args.user}@envio.test"))
    rng = random.Random(SEED)
    lena = ("Lena Ortiz", "lena.ortiz@example.org")
    sam = ("Sam Okafor", "sam.okafor@example.net")
    out = []

    def add(hours, sender, subject, text, html=None, reply=None, refs=None, attach=None):
        mid = f"<listing-{len(out):02d}@envio.test>"
        out.append(build(sender, to, subject, now - timedelta(hours=hours), mid, text, html,
                         reply, refs, attach))
        return mid

    add(300, lena, "Itinerary: Lisbon, 14-18 March",
        "Hi,\n\nHere is the plan for the Lisbon trip.\n\nThu 14: arrive 10:40, tram to the hostel\n"
        "Fri 15: walking tour, dinner at 20:00\nSat 16: day trip by train\n"
        "Sun 17: market, free afternoon\nMon 18: flight home 16:15\n\nSafe travels,\nLena\n",
        '<html><body><h2 style="color:#1b6ca8">Lisbon, 14-18 March</h2><table>'
        "<tr><td>Thu 14</td><td>Arrive 10:40, tram to the hostel</td></tr>"
        "<tr><td>Fri 15</td><td>Walking tour, dinner at 20:00</td></tr>"
        "<tr><td>Sat 16</td><td>Day trip by train</td></tr>"
        "<tr><td>Sun 17</td><td>Market, free afternoon</td></tr>"
        "<tr><td>Mon 18</td><td>Flight home 16:15</td></tr></table>"
        "<p>Safe travels,<br>Lena</p></body></html>")
    first = add(120, sam, "Project Harbor: kickoff notes",
                "Team,\n\nNotes from the kickoff:\n- scope agreed\n- first demo in two weeks\n"
                "- Dara owns the budget\n\nSam\n")
    second = add(96, ("Dara Quinn", "dara@example.net"), "Re: Project Harbor: kickoff notes",
                 "Thanks Sam. The budget sheet is in the shared folder; first numbers by Friday.\n\nDara\n",
                 reply=first, refs=[first])
    add(70, sam, "Re: Project Harbor: kickoff notes",
        "Friday works. I will book the demo room for the week after.\n\nSam\n",
        reply=second, refs=[first, second])
    add(50, ("Orchard Weekly", "news@example.net"),
        "Orchard Weekly: five tips for winter pruning",
        "Orchard Weekly\n\nFive tips for winter pruning. Read the full issue online.\n\n"
        "Unsubscribe: https://example.net/unsubscribe\n",
        '<html><body style="background:#f4f4f4"><div style="max-width:560px;margin:auto;'
        'background:#fff;padding:16px"><h1>Orchard Weekly</h1><p>Five tips for winter pruning.</p>'
        '<p><a href="https://example.net/issue/42">Read the full issue</a></p>'
        '<p style="font-size:11px;color:#777">You get this because you signed up at example.net. '
        '<a href="https://example.net/unsubscribe">Unsubscribe</a></p></div></body></html>')
    add(40, ("Kettle & Co", "orders@example.org"), "Your receipt from Kettle & Co, order 20417",
        "Receipt, order 20417\n\nStovetop kettle  1 x 34.00\nShipping  4.50\nTotal 38.50 EUR\n",
        '<html><body><div style="max-width:480px;margin:auto;font-family:sans-serif">'
        "<h2>Kettle &amp; Co</h2><p>Receipt, order 20417</p>"
        '<table width="100%"><tr><td>Stovetop kettle x 1</td><td align="right">34.00</td></tr>'
        '<tr><td>Shipping</td><td align="right">4.50</td></tr>'
        '<tr><td><b>Total</b></td><td align="right"><b>38.50 EUR</b></td></tr></table>'
        "</div></body></html>")
    add(30, ("Mira Halvorsen", "mira@envio.test"), "Signed lease, scan attached",
        "Hi,\n\nThe signed copy is attached.\n\nMira\n",
        attach=[("lease-scan.pdf", "application/pdf", pdf_bytes("Synthetic lease scan"))])
    add(26, ("Tomas Beck", "tomas.beck@example.org"), "Lunch on Thursday?",
        "Are you free Thursday around 12:30? The new place near the ferry has a good soup.\n\nTomas\n")
    add(20, ("Priya Nandakumar", "priya@example.net"), "Photos from the picnic",
        "Here is one photo from the picnic. More to come.\n\nPriya\n",
        attach=[("picnic.png", "image/png", png_bytes(rng, 96))])
    add(14, ("Jonas Lindqvist", "jonas@envio.test"), "Workshop agenda, draft 2",
        "Draft agenda for the workshop:\n1. Welcome\n2. Prototype demos\n3. Roadmap\n4. Wrap-up\n\nJonas\n",
        '<html><body><h3>Workshop agenda, draft 2</h3><ol><li>Welcome</li>'
        '<li style="color:#a83232">Prototype demos</li><li>Roadmap</li><li>Wrap-up</li></ol>'
        "</body></html>")
    add(9, ("Aiko Tanabe", "aiko.tanabe@example.org"), "Pottery class moved to Saturday",
        "The class now starts at 10:00 on Saturday. Bring an apron.\n\nAiko\n")
    add(6, ("Felix Marchetti", "felix@envio.test"), "Parcel pickup",
        "Your parcel is at the library desk until Friday. Bring an ID.\n\nFelix\n")
    add(4, ("Noor Haddad", "noor.haddad@example.org"), "Bicycle for sale",
        "Still interested in the green bicycle? I can drop it off on Sunday.\n\nNoor\n")
    add(2, lena, "Re: Itinerary: Lisbon, 14-18 March",
        "One change: the Sunday market is closed, so we move it to Saturday morning.\n\nLena\n",
        reply="<listing-00@envio.test>", refs=["<listing-00@envio.test>"])
    add(1, ("Ines Varga", "ines@example.net"), "Minutes from Monday",
        "Minutes:\n\n- budget approved\n- next review in March\n\nInes\n")
    return out


def main():
    ap = argparse.ArgumentParser(description="Synthetic mail for the phone run (test servers only).")
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, required=True, help="IMAPS port")
    ap.add_argument("--user", required=True)
    ap.add_argument("--password", required=True)
    ap.add_argument("--count", type=int, default=3000)
    ap.add_argument("--days", type=int, default=400)
    ap.add_argument("--html-share", type=float, default=0.4)
    ap.add_argument("--attachments", type=int, default=20)
    ap.add_argument("--listing", action="store_true",
                    help="append the curated listing set instead of bulk mail")
    ap.add_argument("--allow-remote", action="store_true",
                    help="allow a host other than loopback (never a real account)")
    args = ap.parse_args()
    if args.host not in ("127.0.0.1", "::1", "localhost") and not args.allow_remote:
        ap.error("refusing a non-loopback host without --allow-remote: "
                 "this appends synthetic mail and must only hit a test server")

    now =datetime.now(timezone.utc).replace(microsecond=0)
    msgs = listing(args, now) if args.listing else bulk(args, random.Random(SEED), now)

    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    imap = imaplib.IMAP4_SSL(args.host, args.port, ssl_context=ctx)
    imap.login(args.user, args.password)
    for m, when in msgs:
        typ, _ = imap.append("INBOX", "", imaplib.Time2Internaldate(when.timestamp()), m.as_bytes())
        if typ != "OK":
            raise SystemExit(f"APPEND failed: {typ}")
    imap.select("INBOX")
    _, data = imap.uid("SEARCH", None, "ALL")
    print(f"appended {len(msgs)}; INBOX now holds {len(data[0].split())}")
    imap.logout()


if __name__ == "__main__":
    main()
