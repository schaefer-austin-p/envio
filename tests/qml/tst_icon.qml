import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"
import "../../qml/components/iconShapes.js" as Shapes

// The ring of search, alert and info is its own subpath: no stray stroke
// joins the end of the handle or dot to the start of the circle.
Item {
    id: root
    width: 200
    height: 100

    Theme { id: thm }

    Row {
        Icon { id: search; theme: thm; name: "search"; size: 48 }
        Icon { id: alert; theme: thm; name: "alert"; size: 48 }
        Icon { id: info; theme: thm; name: "info"; size: 48 }
        Icon { id: offline; theme: thm; name: "offline"; size: 48 }
        Icon { id: clip; theme: thm; name: "clip"; size: 48 }
    }

    TestCase {
        name: "Icon"
        when: windowShown

        // At size 48 one icon unit is 2 px. Reads the canvas's own pixels.
        function alpha(icon, x, y) {
            return icon.getContext("2d").getImageData(x, y, 1, 1).data[3]
        }

        function check(icon, ringX, ringY, strayX, strayY) {
            tryVerify(function() { return alpha(icon, ringX, ringY) > 0 }, 2000,
                      icon.name + ": the ring is drawn")
            compare(alpha(icon, strayX, strayY), 0, icon.name + ": no stray stroke to the ring")
        }

        function test_search_has_no_stray_stroke() { check(search, 35, 22, 37, 31) }
        function test_alert_has_no_stray_stroke() { check(alert, 43, 24, 33, 29) }
        function test_info_has_no_stray_stroke() { check(info, 43, 24, 33, 19) }

        // Cloud-off is an SVG path: the slash crosses the middle, the
        // cloud's base runs along y = 19, and the top-left corner is empty.
        function test_offline_draws_the_cloud_and_the_slash() {
            tryVerify(function() { return alpha(offline, 24, 24) > 0 }, 2000, "the slash is drawn")
            verify(alpha(offline, 26, 38) > 0, "the cloud's base is drawn")
            compare(alpha(offline, 4, 44), 0, "outside the shape stays empty")
            compare(alpha(offline, 44, 4), 0, "the top-right corner stays empty")
        }

        // Every Lucide name of the sheet and menu items, and the top-bar
        // names that now draw Lucide, paints something inside the 24 grid.
        function test_every_name_draws() {
            const names = ["pencil", "key-round", "log-in", "refresh-cw", "trash-2", "mail-open", "mail",
                "flag", "flag-off", "reply-all", "folder-input", "user-plus", "inbox", "send",
                "file-pen", "archive", "shield-alert", "folder", "external-link", "download",
                "share-2", "copy", "circle-user-round", "save", "scissors", "clipboard-paste",
                "text-select", "square-check", "check",
                "trash", "move", "mailopen", "external"]
            for (const n of names) {
                verify(Shapes.names.indexOf(n) >= 0, n + " is listed")
                const ic = Qt.createQmlObject(
                    'import QtQuick 2.15; import "../../qml/components"; Icon { theme: thm; size: 48 }',
                    root, "dyn-" + n)
                tryVerify(function() { return ic.available }, 2000, "canvas ready")
                ic.name = n
                tryVerify(function() { return anyInk(ic) }, 2000, n + " is drawn")
                ic.destroy()
            }
        }
        function anyInk(icon) {
            const d = icon.getContext("2d").getImageData(0, 0, 48, 48).data
            for (let i = 3; i < d.length; i += 4)
                if (d[i] > 0) return true
            return false
        }

        // key-round's dot is a stroke: the centre of (16.5, 7.5) is inked.
        function test_key_round_has_its_dot() {
            const ic = Qt.createQmlObject(
                'import QtQuick 2.15; import "../../qml/components"; Icon { theme: thm; size: 48; name: "key-round" }',
                root, "key")
            tryVerify(function() { return ic.available }, 2000, "canvas ready")
            tryVerify(function() { return alpha(ic, 33, 15) > 0 }, 2000, "the dot is drawn")
            ic.destroy()
        }

        // The paper clip is the design's: a rounded outer bend at the bottom
        // left (the old one was an octagon with a corner there).
        function test_clip_is_the_rounded_paper_clip() {
            tryVerify(function() { return alpha(clip, 9, 39) > 0 }, 2000, "the outer bend is drawn")
            compare(alpha(clip, 16, 32), 0, "inside the inner loop stays empty")
            compare(alpha(clip, 4, 4), 0, "the top-left corner stays empty")
        }
    }
}
