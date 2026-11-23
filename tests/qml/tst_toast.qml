import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"

// The toast pill of the carbon-two-pane board (#353): shrink to fit,
// centred, 28 px up (3.5 gu), a check icon, the Undo action inside it, the
// inverted primary roles, and eliding at phone width.
Item {
    id: root
    width: 800
    height: 600

    Theme { id: thm }
    Item {
        id: stage
        width: 800
        height: 600
        Toast {
            id: toast
            theme: thm
            anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: thm.toastBottomMargin }
        }
    }
    SignalSpy { id: actionSpy; target: toast; signalName: "actionTriggered" }

    TestCase {
        name: "Toast"
        when: windowShown

        function init() {
            thm.name = "carbon"
            thm.darkMode = "light"
            stage.width = 800
            actionSpy.clear()
            toast.hide()
        }

        function find(item, name) {
            if (item.objectName === name) return item
            const kids = item.children || []
            for (let i = 0; i < kids.length; i++) {
                const hit = find(kids[i], name)
                if (hit !== null) return hit
            }
            return null
        }

        function test_shrinks_to_fit_the_text() {
            toast.show("Draft saved")
            wait(50)
            const narrow = toast.width
            verify(narrow < stage.width / 3, "a narrow text is a small pill, not a bar: " + narrow)
            toast.show("Message moved to the folder Archive 2026")
            wait(50)
            verify(toast.width > narrow, "a longer text makes a wider pill")
            compare(toast.width, toast.implicitWidth)
        }

        function test_height_follows_the_text_row_not_the_button() {
            toast.show("Draft saved")
            wait(50)
            const plain = toast.height
            verify(plain >= 40 && plain <= thm.touch, "Draft saved is about 44 px: " + plain)
            toast.show("Archived", "Undo")
            wait(50)
            compare(toast.height, thm.touch, "with Undo the pill is the touch size, not text plus the button")
            verify(toast.height < plain + thm.gu(1), "Undo adds under 1 gu: " + (toast.height - plain))
            compare(find(toast, "toastAction").height, thm.touch)
        }

        function test_has_a_check_icon() {
            toast.show("Draft saved")
            wait(50)
            const icon = find(toast, "toastIcon")
            verify(icon !== null)
            compare(icon.name, "check")
            verify(icon.visible)
        }

        function test_undo_inside_the_pill_still_fires() {
            toast.show("Archived", "Undo")
            wait(50)
            const label = find(toast, "toastAction")
            verify(label.visible)
            const p = label.mapToItem(toast, label.width / 2, label.height / 2)
            verify(p.x < toast.width && p.y < toast.height, "the action lies inside the pill")
            mouseClick(toast, p.x, p.y)
            compare(actionSpy.count, 1)
            compare(toast.visible, false)
        }

        function test_colours_follow_the_roles_in_every_look_and_mode() {
            const looks = ["carbon", "airmail"]
            const modes = ["light", "dark"]
            for (let i = 0; i < 2; i++)
                for (let j = 0; j < 2; j++) {
                    thm.name = looks[i]
                    thm.darkMode = modes[j]
                    toast.show("Draft saved")
                    wait(50)
                    compare(toast.color, thm.toastFill)
                    compare(thm.toastFill, thm.primary)
                    compare(thm.toastText, thm.primaryText)
                    verify(thm.toastShadow.a > 0)
                }
            thm.name = "carbon"
            thm.darkMode = "light"
            compare(thm.toastFill, "#1e2a38")
            compare(thm.toastText, "#f8f6f1")
        }

        function test_radius_is_8px_carbon_and_round_airmail() {
            toast.show("Draft saved")
            wait(50)
            compare(toast.radius, thm.toastRadius)
            thm.name = "airmail"
            compare(toast.radius, toast.height / 2)
        }

        function test_long_text_elides_at_phone_width() {
            stage.width = 360
            toast.show("A rather long message that cannot possibly fit a phone window in one line", "Undo")
            wait(50)
            compare(toast.width, stage.width - thm.gu(4))
            fuzzyCompare(toast.mapToItem(stage, 0, 0).x + toast.width / 2, stage.width / 2, 1)
            const text = find(toast, "toastText")
            verify(text.width < text.implicitWidth)
            verify(text.truncated)
            verify(toast.mapToItem(stage, 0, 0).x >= 0)
        }

        function test_long_text_is_capped_on_a_wide_parent() {
            stage.width = 1200
            toast.show("A rather long message that cannot possibly fit a phone window in one line, and then it goes on and on and on, well past any sensible pill width", "Undo")
            wait(50)
            compare(toast.width, thm.gu(60))
            verify(toast.width < stage.width - thm.gu(4))
            verify(find(toast, "toastText").truncated)
        }
    }
}
