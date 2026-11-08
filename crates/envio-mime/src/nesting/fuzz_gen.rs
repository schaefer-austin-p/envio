//! The fuzzer's input generator: a seeded PRNG and markup biased towards
//! tables, lists and foreign content, where html5ever reopens and moves
//! elements.

/// xorshift64: small, deterministic, no dependency.
pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        // xorshift gets stuck at zero.
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    pub(super) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[rustfmt::skip]
const PIECES: &[&str] = &[
    "<div>", "</div>", "<p>", "</p>", "<li>", "</li>", "<ul>", "</ul>", "<dd>", "<dt>", "</dd>",
    "<dl>", "<td>", "</td>", "<tr>", "</tr>", "<th>", "<table>", "</table>", "<tbody>", "<b>",
    "</b>", "<i>", "</i>", "<a>", "</a>", "<span>", "</span>", "<svg>", "</svg>", "<math>",
    "</math>", "<g>", "</g>", "<g/>", "<path/>", "<foreignObject>", "</foreignObject>", "<desc>",
    "<title>", "</title>", "<mi>", "</mi>", "<annotation-xml>", "<style>", "</style>", "<textarea>",
    "</textarea>", "<script>", "</script>", "<select>", "</select>", "<option>", "<optgroup>",
    "<rb>", "<rt>", "<ruby>", "</ruby>", "<form>", "</form>", "<body>", "</body>", "<html>",
    "</html>", "<font color=r>", "<font>", "</font>", "<!-->", "<!--", "-->", "--!>", "<![CDATA[",
    "]]>", "x", " ", "\"", "'", " a=\"", ">", "<br>", "</br>", "<img>", "<h1>", "</h2>", "<button>",
    "</button>", "<nobr>", "<em>", "</em>", "<center>", "<plaintext>", "<xmp>", "<noscript>",
    "</noscript>", "<template>", "</template>", "<caption>", "<col>", "<colgroup>", "<frameset>",
    "<object>", "</object>", "<applet>", "<marquee>", "<div/>", "<p/>", "<blockquote>", "<pre>",
    "<image>", "<input>", "<hr>", "<!x>", "<?x>", "<</",
];

/// Table, select, list and dl pieces, drawn half the time.
#[rustfmt::skip]
const TABLE_PIECES: &[&str] = &[
    "<table>", "</table>", "<td>", "</td>", "<tr>", "</tr>", "<th>", "</th>", "<tbody>", "</tbody>",
    "<thead>", "<tfoot>", "<caption>", "</caption>", "<colgroup>", "</colgroup>", "<col>",
    "<select>", "</select>", "<option>", "<optgroup>", "</option>", "<ul>", "</ul>", "<ol>", "<li>",
    "</li>", "<dl>", "</dl>", "<dd>", "<dt>", "</dd>", "</dt>", "<p>", "</p>", "<b>", "</b>",
    "<div>", "</div>", "<template>", "</template>", "<form>", "x", "<applet>", "<marquee>",
    "<object>", "</thead>", "</tfoot>", "</th>", "<caption>", "<table>",
];

/// One case: a small vocabulary and a short unit repeated `repeats` times,
/// with an occasional stray piece, so patterns recur and nest deeply.
pub(super) fn generate(rng: &mut Rng, repeats: usize) -> String {
    let k = 2 + rng.below(6);
    let vocab: Vec<&str> = (0..k)
        .map(|_| {
            if rng.below(2) == 0 {
                TABLE_PIECES[rng.below(TABLE_PIECES.len())]
            } else {
                PIECES[rng.below(PIECES.len())]
            }
        })
        .collect();
    let unit_len = 1 + rng.below(6);
    let unit: Vec<&str> = (0..unit_len).map(|_| vocab[rng.below(k)]).collect();
    let mut html = String::new();
    for _ in 0..repeats {
        if rng.below(10) == 0 {
            html.push_str(vocab[rng.below(k)]);
        } else {
            unit.iter().for_each(|piece| html.push_str(piece));
        }
    }
    html
}
