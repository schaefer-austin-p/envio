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

        // Layout time and height at 360 px for the layout-table look-alikes.
        // Measured in the CI image (best of 3): receipt 17 ms, 208 px high
        // (506 px before the width fix); newsletter 106 ms, 1041 px (5007 px
        // before). The bounds below are 3x the measured heights, rounded up.
        function layout(name) {
            let best = 1e9
            let h = 0
            for (let i = 0; i < 3; i++) {
                const start = Date.now()
                h = height(read(name))
                best = Math.min(best, Date.now() - start)
            }
            console.info("LAYOUT " + name + " ms=" + best + " implicitHeight=" + h)
            verify(best < 1000, name + " took " + best + " ms")
            return h
        }
        function test_receipt_like_layout() {
            verify(layout("corpus-receipt-like.html") < 640)
        }
        function test_newsletter_like_layout() {
            verify(layout("corpus-newsletter-like.html") < 3200)
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
