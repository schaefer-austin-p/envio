#!/usr/bin/env python3
"""Regenerates the signed .eml fixtures and keys.txt beside this file.

Run by hand, never by the build:

    python3 -m venv /tmp/dkvenv && /tmp/dkvenv/bin/pip install dkimpy pynacl
    /tmp/dkvenv/bin/python make.py

dkimpy (1.1.x) is a signer written independently of this crate, so the
tests check envio's verifier against another implementation's signatures.
The RSA and Ed25519 keys are throwaway test keys committed beside this file
(`*.pem`, `ed25519.key`); they sign nothing real. A missing key is generated
with `openssl`. Domains are example.com, example.net, mail.example.com and
the shared-suffix pair evil/victim.duckdns.org only.

`keys.txt` holds `<name><TAB><TXT record>` lines for the DNS names the tests
ask for. Signatures carry a `t=` timestamp, so regenerating changes every
fixture; the tests do not depend on the time stamped in.

Optionally cross-check once offline with the `mail-auth` crate (not a
dependency): each *.eml should verify with the record from keys.txt.
"""
import base64
import os
import subprocess
import dkim

HERE = os.path.dirname(os.path.abspath(__file__))
HEADERS = ["from", "to", "subject", "date", "message-id", "content-type"]


def path(name):
    return os.path.join(HERE, name)


def ensure_rsa(name, bits):
    if not os.path.exists(path(name)):
        subprocess.run(["openssl", "genrsa", "-out", path(name), str(bits)], check=True)
    return open(path(name), "rb").read()


def rsa_spki_b64(name):
    der = subprocess.run(
        ["openssl", "rsa", "-in", path(name), "-pubout", "-outform", "DER"],
        check=True,
        capture_output=True,
    ).stdout
    return base64.b64encode(der).decode()


def ensure_ed25519():
    import nacl.encoding
    import nacl.signing

    if not os.path.exists(path("ed25519.key")):
        k = nacl.signing.SigningKey.generate()
        open(path("ed25519.key"), "wb").write(k.encode(nacl.encoding.Base64Encoder))
    seed = open(path("ed25519.key"), "rb").read().strip()
    k = nacl.signing.SigningKey(seed, encoder=nacl.encoding.Base64Encoder)
    pub = k.verify_key.encode(nacl.encoding.Base64Encoder).decode()
    return seed, pub


def message(
    frm="Bob <bob@example.com>",
    subject="Hello",
    body=b"Hello there.\r\nSecond line.\r\n",
    extra=b"",
    ctype=b"text/plain; charset=utf-8",
):
    head = (
        b"From: " + frm.encode() + b"\r\n"
        b"To: Alice <alice@example.org>\r\n"
        b"Subject: " + subject.encode() + b"\r\n"
        b"Date: Thu, 19 Nov 2026 10:00:00 +0000\r\n"
        b"Message-ID: <fixture@example.com>\r\n"
        b"MIME-Version: 1.0\r\n"
        b"Content-Type: " + ctype + b"\r\n" + extra
    )
    return head + b"\r\n" + body


def sign(
    msg,
    key,
    selector,
    domain,
    canon=(b"relaxed", b"relaxed"),
    headers=None,
    identity=None,
    length=False,
    alg=b"rsa-sha256",
    expire=None,
):
    d = dkim.DKIM(msg, signature_algorithm=alg)
    if expire is not None:
        original = d.gen_header

        def gen_header(fields, *a, **kw):
            fields = list(fields[:-1]) + [(b"x", str(expire).encode()), fields[-1]]
            return original(fields, *a, **kw)

        d.gen_header = gen_header
    hdrs = [h.encode() for h in (headers or HEADERS)]
    sig = d.sign(
        selector.encode(),
        domain.encode(),
        key,
        identity=identity,
        canonicalize=canon,
        include_headers=hdrs,
        length=length,
    )
    return sig + msg


def main():
    rsa = ensure_rsa("rsa2048.pem", 2048)
    small = ensure_rsa("rsa512.pem", 512)
    seed, ed_pub = ensure_ed25519()
    out = {}
    out["relaxed"] = sign(message(), rsa, "test", "example.com")
    out["simple"] = sign(message(), rsa, "test", "example.com", (b"simple", b"simple"))
    out["relaxed_simple"] = sign(message(), rsa, "test", "example.com", (b"relaxed", b"simple"))
    out["overs"] = sign(
        message(), rsa, "test", "example.com", headers=HEADERS + ["from", "x-absent"]
    )
    out["folded"] = sign(
        message(
            subject="A long subject " + "word " * 30,
            extra=b"X-Long: " + b"alpha beta gamma delta " * 8 + b"\r\n",
        ),
        rsa,
        "test",
        "example.com",
        headers=HEADERS + ["x-long"],
    )
    out["eightbit"] = sign(
        message(body="Grüße aus München ✓\r\n".encode()),
        rsa,
        "test",
        "example.com",
        (b"simple", b"simple"),
    )
    out["empty"] = sign(message(body=b""), rsa, "test", "example.com")
    out["nocontent"] = sign(
        message(), rsa, "test", "example.com", headers=["from", "subject", "date"]
    )
    out["net"] = sign(message(), rsa, "test", "example.net")
    out["sub"] = sign(message(), rsa, "test", "mail.example.com")
    out["duck"] = sign(
        message(frm="Bob <bob@victim.duckdns.org>"), rsa, "test", "evil.duckdns.org"
    )
    out["small"] = sign(message(), small, "small", "example.com")
    out["ed"] = sign(message(), seed, "ed", "example.com", alg=b"ed25519-sha256")
    out["xpast"] = sign(message(), rsa, "test", "example.com", expire=1000000000)
    out["length"] = sign(message(), rsa, "test", "example.com", length=True)
    out["agent"] = sign(message(), rsa, "test", "example.com", identity=b"@sub.example.com")
    out["cte_signed"] = sign(
        message(extra=b"Content-Transfer-Encoding: 7bit\r\n"),
        rsa,
        "test",
        "example.com",
        headers=HEADERS + ["content-transfer-encoding"],
    )
    # A valid signature that does not cover Content-Transfer-Encoding, then
    # an unsigned `Content-Transfer-Encoding: base64` added to the header.
    signed = sign(message(), rsa, "test", "example.com")
    end = signed.index(b"\r\n\r\n") + 2
    out["cte_appended"] = signed[:end] + b"Content-Transfer-Encoding: base64\r\n" + signed[end:]
    # Two Content-Type fields, both listed in h=.
    out["ct_twice"] = sign(
        message(extra=b"Content-Type: text/plain; charset=utf-8\r\n"),
        rsa,
        "test",
        "example.com",
        headers=HEADERS + ["content-type"],
    )
    # An HTML message with a remote image (the core's sender-trust tests).
    out["html"] = sign(
        message(
            body=b'<p>Look</p><img src="http://images.example.net/a.png" alt="a">\r\n',
            ctype=b"text/html; charset=utf-8",
        ),
        rsa,
        "test",
        "example.com",
    )
    for name, data in out.items():
        open(path(name + ".eml"), "wb").write(data)
    spki = rsa_spki_b64("rsa2048.pem")
    with open(path("keys.txt"), "w") as f:
        for name in ("example.com", "example.net", "mail.example.com", "evil.duckdns.org"):
            f.write("test._domainkey.%s\tv=DKIM1; k=rsa; p=%s\n" % (name, spki))
        f.write("small._domainkey.example.com\tv=DKIM1; k=rsa; p=%s\n" % rsa_spki_b64("rsa512.pem"))
        f.write("ed._domainkey.example.com\tv=DKIM1; k=ed25519; p=%s\n" % ed_pub)


if __name__ == "__main__":
    main()
