import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"

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
    }
}
