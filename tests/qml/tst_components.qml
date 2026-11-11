import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"

Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }

    ListRow {
        id: row
        theme: thm
        width: 300
        Text { text: "Plans for the weekend" }
    }
    SignalSpy { id: rowClicked; target: row; signalName: "clicked" }
    SignalSpy { id: rowHeld; target: row; signalName: "pressAndHold" }

    Toast { id: toast; theme: thm; width: 300 }
    SignalSpy { id: toastExpired; target: toast; signalName: "expired" }
    SignalSpy { id: toastAction; target: toast; signalName: "actionTriggered" }

    AppDialog {
        id: dialog
        theme: thm
        title: "Unverified certificate"
        message: "Cancel is the safe choice."
        primaryText: "Cancel"
        secondaryText: "Trust this certificate"
    }
    SignalSpy { id: dialogPrimary; target: dialog; signalName: "primaryActivated" }
    SignalSpy { id: dialogSecondary; target: dialog; signalName: "secondaryActivated" }

    TestCase {
        name: "Components"
        when: windowShown

        function init() {
            rowClicked.clear()
            rowHeld.clear()
            toastExpired.clear()
            toastAction.clear()
            dialogPrimary.clear()
            dialogSecondary.clear()
            toast.duration = 5000
            toast.hide()
        }

        function test_row_press_and_hold_signals() {
            mouseClick(row, 10, 10)
            compare(rowClicked.count, 1)
            compare(rowHeld.count, 0)
            mousePress(row, 10, 10)
            wait(1100)
            mouseRelease(row, 10, 10)
            compare(rowHeld.count, 1)
            compare(rowClicked.count, 1, "a long press is not also a tap")
        }

        function everything(item, out) {
            out.push(item)
            for (const prop of ["data", "children"]) {
                if (item[prop] === undefined) continue
                for (let i = 0; i < item[prop].length; i++) everything(item[prop][i], out)
                break
            }
            return out
        }

        function test_row_has_no_swipe() {
            const all = everything(row, [])
            verify(all.length > 1)
            for (const obj of all) {
                verify(String(obj).indexOf("Swipe") < 0, "swipe type " + obj)
                verify(obj.leadingActions === undefined && obj.trailingActions === undefined,
                       "swipe actions on " + obj)
            }
            verify(row.height >= 44)
        }

        function test_toast_defaults_to_five_seconds() {
            compare(toast.duration, 5000)
            compare(toast.visible, false)
        }

        function test_toast_hides_after_timeout() {
            toast.duration = 100
            toast.show("Archived", "Undo")
            compare(toast.visible, true)
            compare(toast.text, "Archived")
            compare(toast.actionText, "Undo")
            tryVerify(function() { return !toast.visible }, 2000)
            compare(toastExpired.count, 1)
            compare(toastAction.count, 0)
        }

        function test_toast_action_hides_and_signals() {
            toast.show("Archived", "Undo")
            mouseClick(toast, toast.width - 20, toast.height / 2)
            compare(toastAction.count, 1)
            compare(toast.visible, false)
            wait(50)
            compare(toastExpired.count, 0)
        }

        function test_dialog_keeps_primary_first() {
            dialog.open()
            tryVerify(function() { return dialog.opened })
            const all = everything(dialog.contentItem, [])
            const primary = all.filter(function(o) { return o.objectName === "primaryAction" })[0]
            const secondary = all.filter(function(o) { return o.objectName === "secondaryAction" })[0]
            verify(primary && secondary)
            verify(primary.primary && !secondary.primary)
            tryVerify(function() { return primary.mapToItem(null, 0, 0).y < secondary.mapToItem(null, 0, 0).y },
                      2000, "primary action is listed first")
            verify(primary.height >= 44 && secondary.height >= 44)
            mouseClick(primary)
            compare(dialogPrimary.count, 1)
            compare(dialogSecondary.count, 0)
            tryVerify(function() { return !dialog.visible })
        }

        function test_password_field_keeps_its_hints() {
            pwField.inputMethodHints = Qt.ImhEmailCharactersOnly
            const h = pwField.effectiveHints
            verify(h & Qt.ImhNoPredictiveText, "no prediction")
            verify(h & Qt.ImhSensitiveData, "sensitive")
            verify(h & Qt.ImhEmailCharactersOnly, "the caller's hint stays")
            compare(pwField.password, true)
        }

        function test_touch_targets_are_44() {
            const ib = createTemporaryObject(iconButtonComponent, root)
            verify(ib.width >= 44 && ib.height >= 44)
        }
    }

    Field { id: pwField; theme: thm; password: true; width: 300; y: 400 }

    Component {
        id: iconButtonComponent
        IconButton { theme: thm; iconName: "back" }
    }
}
