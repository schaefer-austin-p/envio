import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"

// #348: rounded frame corners only in a Lomiri session with a window that
// covers the screen.
Item {
    id: root
    width: 100
    height: 100

    DisplayCorners { id: dc; screenWidth: 360; screenHeight: 640 }

    TestCase {
        name: "DisplayCorners"

        function init() { dc.phoneSession = true; dc.windowWidth = 360; dc.windowHeight = 640; dc.panelAllowance = 108 }

        function test_lomiri_full_screen_is_rounded() { compare(dc.rounded, true) }
        function test_lomiri_narrower_is_square() {
            dc.windowWidth = 300
            dc.windowHeight = 640 - 108
            compare(dc.rounded, false)
        }
        function test_lomiri_below_the_indicator_panel_is_rounded() {
            dc.windowHeight = 640 - 108   // staged window under the panel
            compare(dc.rounded, true)
        }
        function test_lomiri_much_shorter_is_square() {
            dc.windowHeight = 640 - 160
            compare(dc.rounded, false)
        }
        function test_lomiri_smaller_window_is_square() {
            dc.windowWidth = 300
            compare(dc.rounded, false)
            dc.windowWidth = 360
            dc.windowHeight = 500
            compare(dc.rounded, false)
        }
        function test_desktop_with_suru_is_square() {
            // Full-size window, no Lomiri session: Suru presence is not asked.
            dc.phoneSession = false
            compare(dc.rounded, false)
        }
        function test_follows_a_resize() {
            compare(dc.rounded, true)
            dc.windowWidth = 200; dc.windowHeight = 300
            compare(dc.rounded, false)
            dc.windowWidth = 360; dc.windowHeight = 640
            compare(dc.rounded, true)
        }
        function test_unknown_screen_is_square() {
            dc.screenWidth = 0
            compare(dc.rounded, false)
            dc.screenWidth = 360
        }
    }
}
