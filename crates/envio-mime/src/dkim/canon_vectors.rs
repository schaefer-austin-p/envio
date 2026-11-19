//! Vectors signed by an independent signer (dkimpy 1.1.8, a throwaway RSA-2048
//! key that exists only for these tests, example.com), and checked by the
//! independent dkimpy verifier too. The `*_mid` headers were signed with openssl over
//! the RFC 6376 3.5 reading (the `b=` value and its whitespace deleted) and verified by
//! dkimpy; they put `b=` in the middle, folded right after the `=`.

/// RSAPublicKey (PKCS#1) DER, base64.
pub(super) const PUB: &str = "MIIBCgKCAQEA2RCBs8/faNmqJHsk6O0k6WutUlUcAT9fbgVFsvaY3NDKtJg6YAGiLMUQ1UF/v01SbQ9DuiNjy04qSBArXCfN26wHdC3jo+pHmiuuJ2r+AjRfTnQKWeyNoQVBnEl9zpjq/81oTW2KaDcpXQQRINpwTk/WSJpjjmNroMquJOMI9DIDDxkAyLkOfMYx8cKJXznWdrDROHGhBE0dCIoODeDdbX5SBBoPespTkP8fVHhF9YiPKTHO1QcF3vWyfd1S9cXQJqhIlkkUODBo3lGKStyrJsbK99OhwTqOaaZozygX/TEkL7fPQz/bymQ3iPSENs+rb0TtPjaL1g88f9jZiD8+2wIDAQAB";

pub(super) const MSG: &str = concat!(
    "From: Alice <alice@example.com>\r\n",
    "To: bob@example.org\r\n",
    "Subject: Test  message\r\n",
    "X-Trace: x\r\n",
    "X-Trace: y\r\n",
    "\r\n",
    "Hello \t world  \r\n",
    "line two\r\n",
    "\r\n",
    "\r\n",
);

pub(super) const RELAXED: &str = concat!(
    "DKIM-Signature: v=1; a=rsa-sha256; c=relaxed/relaxed; d=example.com;\r\n",
    " i=@example.com; q=dns/txt; s=sel; t=1795090241; h=from : to : subject\r\n",
    " : x-trace : x-trace; bh=RXJvS3AZ5yyl+JsI1YW3JLAB1Rlo0gl3yq8+PQpFDXI=;\r\n",
    " b=gS9uh+IvCllNTx1OkiFHPSS7OQSalG7m47ol8+m1oaSvKRIs/uYDH9BK1i8YnO/FYLPvP\r\n",
    " vb3XWTSi/dAjaD7MPaiIh/R26JfvNkTQf/N21sWp598lcaZekBrZL82NdrdMsBAGBt66Ti1\r\n",
    " JMhUhv8bxPiA/p4pQMXWKKIfYhb19CZXrJzfE7D78kSwi0ifj97h0HYsjumCV38j5UL41SP\r\n",
    " ntHHknMLt6633jSJ0yfspevdhM/41ouzd+UfJ25dH4Oh5HRgO/YNLgLffYkrzfCqIWlZ7Ta\r\n",
    " RQwlb+TIafZ2SB1f8AEexrY92RJQCjdNm0LvWXu80PAAEAzepvVgTLELDekw==\r\n",
);

pub(super) const RELAXED_FOLD: &str = concat!(
    "DKIM-Signature: v=1; a=rsa-sha256; c=relaxed/relaxed; d=example.com;\r\n",
    " i=@example.com; q=dns/txt; s=sel; t=1795090241; h=from : to : subject\r\n",
    " : x-trace : x-trace; bh=RXJvS3AZ5yyl+JsI1YW3JLAB1Rlo0gl3yq8+PQpFDXI=;\r\n",
    " b=\r\n",
    " gS9uh+IvCllNTx1OkiFHPSS7OQSalG7m47ol8+m1oaSvKRIs/uYDH9BK1i8YnO/FYLPvP\r\n",
    " vb3XWTSi/dAjaD7MPaiIh/R26JfvNkTQf/N21sWp598lcaZekBrZL82NdrdMsBAGBt66Ti1\r\n",
    " JMhUhv8bxPiA/p4pQMXWKKIfYhb19CZXrJzfE7D78kSwi0ifj97h0HYsjumCV38j5UL41SP\r\n",
    " ntHHknMLt6633jSJ0yfspevdhM/41ouzd+UfJ25dH4Oh5HRgO/YNLgLffYkrzfCqIWlZ7Ta\r\n",
    " RQwlb+TIafZ2SB1f8AEexrY92RJQCjdNm0LvWXu80PAAEAzepvVgTLELDekw==\r\n",
);

pub(super) const RELAXED_MID: &str = concat!(
    "DKIM-Signature: v=1; a=rsa-sha256; c=relaxed/relaxed; d=example.com; s=sel;\r\n",
    " h=from:to:subject:x-trace:x-trace; bh=RXJvS3AZ5yyl+JsI1YW3JLAB1Rlo0gl3yq8+PQpFDXI=;\r\n",
    " b=\r\n",
    " Xqbj0vcwj3q6gPW4w3/FTIzGNhndBlOgC3UENk7jWXBucnul33On/0RAmr3O\r\n",
    " dgjBuwDDNu8yrSrpjBTbcORRqYUxkfHbx8pGDDar7Qh6845F7h/aHBAI5ZvJ\r\n",
    " 3rDaQO511yApH4mz+HAW7zNBcJ24qCHpwHjkArnbBOv3AxdZ8ezWrH0pePWc\r\n",
    " 2+Qd8+Mnexie6N0D+jticBQoL9A8gBgqyppJkmczaVlY+9NdVeVWbRL0FKyJ\r\n",
    " k3YnM10s1V3Vv+aSKCVqHBxMyjlAcytGjaxh5ChS7sKgO0BCYcOczf/3edo1\r\n",
    " 8B5EElE3tNkAGYgJHYtwT9d44Ot5KRpwHFr3rPFkWA==;\r\n",
    " i=@example.com\r\n",
);

pub(super) const SIMPLE: &str = concat!(
    "DKIM-Signature: v=1; a=rsa-sha256; c=simple/simple; d=example.com;\r\n",
    " i=@example.com; q=dns/txt; s=sel; t=1795090241; h=from : to : subject\r\n",
    " : x-trace : x-trace; bh=bEfOSh8FdoY8ar8lPCRYbOZwVYd4+sfOyFxioKLfOyk=;\r\n",
    " b=J4yNzV3WAgmqLNTE+dqefiaWZ23NC/yxEnj3X6IXnAzaZ7aC3zNbwJrSJLQi1JVZ/jHl+\r\n",
    " 87PVfbHMSkWiXNFnvjE9frwzHFZgxQEtCwgqMjliN4q69PuVeroKFUR7RDnkJdu56Fx3NbO\r\n",
    " SH8ykJz8xYO6+F79le42Ljjw1XgJhr1DS37Uban7bzSkZ4+87lNrrRSPoB2krmu7PyHsRDH\r\n",
    " IJavCz9EIOIG1s844YYgUjY/VNqIY+gYXi7LdEbp60sLlCMu01cmfN/6L+6ekjEj6LSN7sl\r\n",
    " 38+dkv4UhOmYsFmboHP6D43PboCcrV+0zVeNAcMEu5UytppakH/Gignl1t6g==\r\n",
);

pub(super) const SIMPLE_FOLD: &str = concat!(
    "DKIM-Signature: v=1; a=rsa-sha256; c=simple/simple; d=example.com;\r\n",
    " i=@example.com; q=dns/txt; s=sel; t=1795090241; h=from : to : subject\r\n",
    " : x-trace : x-trace; bh=bEfOSh8FdoY8ar8lPCRYbOZwVYd4+sfOyFxioKLfOyk=;\r\n",
    " b=\r\n",
    " J4yNzV3WAgmqLNTE+dqefiaWZ23NC/yxEnj3X6IXnAzaZ7aC3zNbwJrSJLQi1JVZ/jHl+\r\n",
    " 87PVfbHMSkWiXNFnvjE9frwzHFZgxQEtCwgqMjliN4q69PuVeroKFUR7RDnkJdu56Fx3NbO\r\n",
    " SH8ykJz8xYO6+F79le42Ljjw1XgJhr1DS37Uban7bzSkZ4+87lNrrRSPoB2krmu7PyHsRDH\r\n",
    " IJavCz9EIOIG1s844YYgUjY/VNqIY+gYXi7LdEbp60sLlCMu01cmfN/6L+6ekjEj6LSN7sl\r\n",
    " 38+dkv4UhOmYsFmboHP6D43PboCcrV+0zVeNAcMEu5UytppakH/Gignl1t6g==\r\n",
);

pub(super) const SIMPLE_MID: &str = concat!(
    "DKIM-Signature: v=1; a=rsa-sha256; c=simple/simple; d=example.com; s=sel;\r\n",
    " h=from:to:subject:x-trace:x-trace; bh=bEfOSh8FdoY8ar8lPCRYbOZwVYd4+sfOyFxioKLfOyk=;\r\n",
    " b=\r\n",
    " 0MW3NBLBWzpMuyYmtWSRV+9EnpR0/IlQSl+GSvizE/AieopoprpHCp5m/E7c\r\n",
    " si5eOVky/g8oMuCE0VhsHwp5vDSxelXfeihFqzizCblMU0BasKF24PExqum8\r\n",
    " EqQW7E74ULhVuwyLYLqF//nt4eCPGmrIl6BKQO4lRxpRGfaHqH+QDrnZ6jD4\r\n",
    " thhSOOJMOCBN9/dGiMSe1q4vUOZ0awV55/Pmbkn5ybfEcezzHXkPiiPa1nrC\r\n",
    " 1g4ZIY9XD7+UXwANPcStqvnVxICQY1hHJ/cXMGHBaaEZzsFTTy2FkzJdJZNY\r\n",
    " J6MChxgrj5B7fFqcpkQCUzvksrkdYDGqeXulM7TN4g==;\r\n",
    " i=@example.com\r\n",
);
