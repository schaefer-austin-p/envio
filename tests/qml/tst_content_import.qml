import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform/fileUrl.js" as FileUrl

// The URL to path step of platform/ut/ContentImport.qml, which needs
// Lomiri and so cannot be loaded here.
TestCase {
    name: "ContentImport"

    function test_a_percent_escape_is_decoded() {
        compare(FileUrl.pathOfUrl("file:///a/b%20c.pdf"), "/a/b c.pdf")
    }

    function test_a_bare_percent_is_kept_and_does_not_throw() {
        // The plain call is what ContentImport used to make.
        let threw = false
        try { decodeURIComponent("/x/100%.pdf") } catch (e) { threw = true }
        verify(threw, "decodeURIComponent throws on a bare %")
        compare(FileUrl.pathOfUrl("file:///x/100%.pdf"), "/x/100%.pdf")
        compare(FileUrl.pathOfUrl("file:///a/a%20b%20100%.pdf"), "/a/a b 100%.pdf")
    }

    function test_urls_keep_their_order() {
        compare(FileUrl.pathsOfUrls([
            "file:///z/last%20one.txt",
            "file:///x/100%.pdf",
            "file:///a/first.png"
        ]), ["/z/last one.txt", "/x/100%.pdf", "/a/first.png"])
        compare(FileUrl.pathsOfUrls([]), [])
    }
}
