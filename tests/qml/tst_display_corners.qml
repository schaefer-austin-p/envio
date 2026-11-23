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

        function init() { dc.phoneSession = true; dc.windowWidth = 360; dc.windowHeight = 640 }

        function test_lomiri_full_screen_is_rounded() { compare(dc.rounded, true) }
        function test_lomiri_full_width_any_height_is_rounded() {
            dc.windowHeight = 640 - 108   // staged window under the indicator panel
            compare(dc.rounded, true)
            dc.windowHeight = 640 - 160
            compare(dc.rounded, true)
        }
        function test_lomiri_narrower_is_square() {
            dc.windowWidth = 360 - 50
            compare(dc.rounded, false)
            dc.windowHeight = 640 - 108
            compare(dc.rounded, false)
        }
        function test_lomiri_one_pixel_narrower_is_rounded() {
            dc.windowWidth = 359
            compare(dc.rounded, true)
        }
        function test_desktop_session_is_square() {
            // Full-size window, no Lomiri session.
            dc.phoneSession = false
            compare(dc.rounded, false)
        }
        function test_follows_a_resize() {
            compare(dc.rounded, true)
            dc.windowWidth = 200
            compare(dc.rounded, false)
            dc.windowWidth = 360
            compare(dc.rounded, true)
        }
        function test_unknown_screen_is_square() {
            dc.screenWidth = 0
            compare(dc.rounded, false)
            dc.screenWidth = 360
        }
    }
}
