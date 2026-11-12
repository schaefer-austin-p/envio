import QtQuick 2.15
import QtTest 1.2
import "../../qml/components/richText.js" as RichText

// #74: Qt 5's Text.linkColor only reaches StyledText, so the reader puts the
// colour into the HTML. Rendered, because the property alone proves nothing.
// (TestCase.grabImage comes back empty on the offscreen software backend, so
// the item is grabbed with grabToImage and its pixels read through a Canvas.)
Item {
    id: root
    width: 300
    height: 120

    Component {
        id: linkComponent
        Rectangle {
            property alias label: label
            width: 300
            height: 120
            color: "white"
            Text {
                id: label
                textFormat: Text.RichText
                font.pixelSize: 40
                color: "black"
            }
        }
    }

    Canvas {
        id: reader
        width: 300
        height: 120
        visible: false
        renderTarget: Canvas.Image
        renderStrategy: Canvas.Immediate
    }

    TestCase {
        name: "LinkColor"
        when: windowShown

        readonly property string html: "<a href=\"https://x.test\">link</a>"

        // Pixels of the item, as RGBA bytes.
        function pixels(item) {
            waitForRendering(item)
            let url = ""
            item.grabToImage(function(res) { url = String(res.url) })
            tryVerify(function() { return url !== "" })
            reader.loadImage(url)
            tryVerify(function() { return reader.isImageLoaded(url) })
            const ctx = reader.getContext("2d")
            ctx.clearRect(0, 0, 300, 120)
            ctx.drawImage(url, 0, 0)
            return ctx.getImageData(0, 0, 300, 120).data
        }

        // How many pixels are exactly rgb(r, g, b); a blank grab must not
        // pass the negative case, so some non-white ink is required too.
        function count(item, r, g, b) {
            const d = pixels(item)
            let exact = 0
            let ink = 0
            for (let i = 0; i < d.length; i += 4) {
                if (d[i] !== 255 || d[i + 1] !== 255 || d[i + 2] !== 255)
                    ++ink
                if (d[i] === r && d[i + 1] === g && d[i + 2] === b)
                    ++exact
            }
            verify(ink > 0, "nothing was rendered")
            return exact
        }

        // The underline is solid, so no anti-aliased edge is needed.
        function test_link_colour_alone_does_not_reach_rich_text() {
            const r = createTemporaryObject(linkComponent, root)
            r.label.linkColor = "#00ff00"
            r.label.text = html
            compare(count(r, 0, 255, 0), 0)
        }

        function test_with_link_colour_paints_the_link() {
            const r = createTemporaryObject(linkComponent, root)
            r.label.text = RichText.withLinkColor(html, "#00ff00")
            verify(count(r, 0, 255, 0) > 20)
        }

        function test_with_link_colour_string() {
            compare(RichText.withLinkColor("<p>x</p>", "#112233"),
                    "<style>a{color:#112233}</style><p>x</p>")
            compare(RichText.withLinkColor("", "#112233"), "<style>a{color:#112233}</style>")
        }

        // The Airmail board's body line height (1.6) goes in the same
        // stylesheet; 1.0 adds nothing.
        function test_with_line_height_string() {
            compare(RichText.withLinkColor("<p>x</p>", "#112233", 1.6),
                    "<style>a{color:#112233}p,li,div{line-height:160%}</style><p>x</p>")
            compare(RichText.withLinkColor("<p>x</p>", "#112233", 1.0),
                    "<style>a{color:#112233}</style><p>x</p>")
        }

        function test_with_line_height_makes_a_paragraph_taller() {
            const text = "<p>" + "word ".repeat(60) + "</p>"
            const a = createTemporaryObject(linkComponent, root)
            a.label.width = 300
            a.label.wrapMode = Text.Wrap
            a.label.text = RichText.withLinkColor(text, "#112233", 1.0)
            const b = createTemporaryObject(linkComponent, root)
            b.label.width = 300
            b.label.wrapMode = Text.Wrap
            b.label.text = RichText.withLinkColor(text, "#112233", 1.6)
            verify(b.label.contentHeight > a.label.contentHeight * 1.3,
                   a.label.contentHeight + " vs " + b.label.contentHeight)
        }
    }
}
