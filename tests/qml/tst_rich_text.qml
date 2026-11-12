import QtQuick 2.15
import QtTest 1.2

// The rich Text the reader uses, fed what envio-mime renders for its own
// corpus, the attack files and a few generated shapes. tools/ci/render-corpus.sh
// writes the files and index.txt; tests/qml/main.cpp hands the directory over
// as `corpusDir`.
Item {
    id: root
    width: 400
    height: 700

    Component {
        id: bodyComponent
        Text {
            width: 360
            textFormat: Text.RichText
            wrapMode: Text.Wrap
        }
    }

    // What the reader uses (ReaderPage): a read-only TextEdit.
    Component {
        id: editComponent
        TextEdit {
            width: 360
            readOnly: true
            textFormat: TextEdit.RichText
            wrapMode: TextEdit.Wrap
        }
    }

    Component {
        id: imageComponent
        Image { width: 10; height: 10 }
    }

    TestCase {
        name: "RichText"
        when: windowShown

        function read(name) {
            const xhr = new XMLHttpRequest()
            xhr.open("GET", "file://" + corpusDir + "/" + name, false)
            xhr.send()
            return xhr.responseText
        }
        function names() {
            verify(corpusDir !== "", "ENVIO_CORPUS_DIR is not set: run tests/qml/run.sh")
            const list = read("index.txt").split("\n").filter(function(n) { return n !== "" })
            verify(list.length > 20, "the corpus index is empty")
            return list
        }
        function height(html) {
            const body = createTemporaryObject(bodyComponent, root, { text: html })
            return body.implicitHeight
        }

        function test_every_file_lays_out() {
            for (const name of names()) {
                const html = read(name)
                // The data-image attack renders to nothing at all.
                if (html.trim() === "") continue
                verify(height(html) > 0, name + " has no height")
            }
        }

        // The positive controls: if the counter did not see these, the zero
        // assertions below would prove nothing.
        function test_the_counter_sees_an_image() {
            netRequests.reset()
            createTemporaryObject(imageComponent, root, { source: "http://example.invalid/x.png" })
            tryVerify(function() { return netRequests.count() > 0 })
        }

        function test_the_counter_sees_rich_text_fetching() {
            netRequests.reset()
            height('<p>Hi <img src="http://example.invalid/x.png"></p>')
            tryVerify(function() { return netRequests.count() > 0 })
        }

        function test_the_corpus_fetches_nothing() {
            netRequests.reset()
            for (const name of names()) height(read(name))
            wait(500)
            compare(netRequests.count(), 0)
        }

        // The sanitiser's own output, checked again where it is cheap.
        function test_no_remote_image_in_the_rendered_html() {
            const remote = /<img\b[^>]*\bsrc\s*=\s*["']?\s*(https?:|\/\/)/i
            for (const name of names()) {
                const html = read(name)
                verify(!remote.test(html), name + " has a remote image")
            }
        }

        function test_blockquote_depth_is_visible() {
            // Each level indents the text, so the same words wrap onto more lines.
            const filler = "word ".repeat(40)
            const deep = height(read("quote-3.html").replace(/level \d/, filler))
            const shallow = height(read("quote-1.html").replace(/level \d/, filler))
            verify(deep > shallow, deep + " is not taller than " + shallow)
        }

        function timed(name) {
            const start = Date.now()
            const h = height(read(name))
            const took = Date.now() - start
            verify(h > 0, name + " has no height")
            verify(took < 1000, name + " took " + took + " ms")
        }

        // Layout time and height at 360 px for the layout-table look-alikes
        // (20-30 KB of HTML, 150+ spans, 35+ tables), in a Text and in the
        // reader's read-only TextEdit; best of 3. The Text numbers are only
        // logged (the "before"); the TextEdit numbers are bounded.
        function layout(label, name, component) {
            let best = 1e9
            let h = 0
            for (let i = 0; i < 3; i++) {
                const start = Date.now()
                const item = createTemporaryObject(component, root, { text: read(name) })
                h = item.height > 0 ? item.height : item.implicitHeight
                best = Math.min(best, Date.now() - start)
            }
            console.info("LAYOUT " + label + " " + name + " ms=" + best + " height=" + h)
            return { ms: best, h: h }
        }
        function check(name, maxHeight) {
            layout("Text", name, bodyComponent)
            const edit = layout("TextEdit", name, editComponent)
            verify(edit.ms < 1000, name + " took " + edit.ms + " ms")
            verify(edit.h > 0 && edit.h < maxHeight, name + " is " + edit.h + " px high")
        }
        function test_receipt_like_layout() {
            // Height bounds: 3x the heights measured in the CI image (receipt 1356 px, newsletter 2177 px).
            check("corpus-receipt-like.html", 4100)
        }
        function test_newsletter_like_layout() {
            check("corpus-newsletter-like.html", 6600)
        }

        // The x-extent (px) of the first 80 characters of the paragraph that holds
        // `marker` (x of the first to x of the last), as laid out
        // in the reader's read-only TextEdit at width 360. A text column that
        // Qt squeezes beside a fixed cell shows up as an extent of a few
        // characters (#90).
        function columnWidth(name, marker) {
            const edit = createTemporaryObject(editComponent, root, { text: read(name) })
            const text = edit.getText(0, edit.length)
            const from = text.indexOf(marker)
            verify(from >= 0, marker + " is not in " + name)
            let to = text.indexOf("\n", from)
            if (to < 0 || to > from + 80) to = Math.min(text.length, from + 80)
            let left = 1e9
            let right = 0
            for (let i = from; i < to; i++) {
                const r = edit.positionToRectangle(i)
                left = Math.min(left, r.x)
                right = Math.max(right, r.x)
            }
            console.info("COLUMN " + name + " " + marker + " width=" + (right - left))
            return right - left
        }
        function test_notice_like_text_column_uses_most_of_the_width() {
            // #90 passes at 0.6 x 360 px = 216 px.
            const w = columnWidth("corpus-notice-like.html", "NOTICE-BODY")
            verify(w >= 216, "the notice's text column is " + w + " px wide")
            // Measured in the CI image at 360 px: the NOTICE-BODY paragraph's
            // extent was 41 px and the page 1737 px high before #90, and is
            // 238 px and 317 px high after. The height bound is 3x that.
            check("corpus-notice-like.html", 951)
        }
        function test_receipt_like_text_column_uses_most_of_the_width() {
            const w = columnWidth("corpus-receipt-like.html", "Example Plan, monthly")
            verify(w >= 216, "the receipt's text column is " + w + " px wide")
        }

        // envio-mime caps nesting at 256 levels (MAX_DEPTH): a message that
        // deep or deeper is rendered as plain text. The cap is Rust's; here
        // it is asserted, with the time budget (the phone sets the real one).
        function test_deep_nesting_renders_within_the_budget() {
            timed("nesting-200.html")
            verify(read("nesting-200.html").indexOf("white-space:pre-wrap") < 0)
        }

        function test_nesting_at_the_cap_falls_back_to_text() {
            for (const name of ["nesting-256.html", "nesting-300.html"]) {
                verify(read(name).indexOf("white-space:pre-wrap") >= 0, name)
                timed(name)
            }
        }
    }
}
