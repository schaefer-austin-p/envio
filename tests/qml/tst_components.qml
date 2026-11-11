import QtQuick 2.15
import QtTest 1.2
import QtQuick.Controls 2.15
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

        function holdOn(f) {
            mousePress(f.input, 20, f.input.height / 2)
            wait(1000)
            mouseRelease(f.input, 20, f.input.height / 2)
        }

        function items(f) {
            const m = f.editMenu
            const out = {}
            for (let i = 0; i < m.count; i++) out[m.itemAt(i).objectName] = m.itemAt(i)
            return out
        }

        function test_press_and_hold_opens_the_edit_menu() {
            const f = createTemporaryObject(fieldComponent, root, { text: "abc" })
            waitForRendering(f)
            verify(!f.editMenu.visible)
            holdOn(f)
            tryVerify(function() { return f.editMenu.visible })
            compare(f.editMenu.objectName, "editMenu")
            const it = items(f)
            for (const n of ["editCut", "editCopy", "editPaste", "editSelectAll"]) {
                verify(it[n] !== undefined, n)
                verify(it[n].height >= 44, n + " is touch high")
            }
            f.editMenu.close()
        }

        function test_paste_puts_the_clipboard_in_the_field() {
            helper.text = "clip@example.org"
            helper.selectAll()
            helper.copy()
            const f = createTemporaryObject(fieldComponent, root, {})
            waitForRendering(f)
            holdOn(f)
            tryVerify(function() { return f.editMenu.visible })
            const paste = items(f).editPaste
            verify(paste.enabled)
            paste.triggered()
            compare(f.text, "clip@example.org")
        }

        function test_press_and_hold_keeps_the_selection_for_copy() {
            const f = createTemporaryObject(fieldComponent, root, { text: "keep@example.org" })
            waitForRendering(f)
            f.input.forceActiveFocus()
            f.input.selectAll()
            holdOn(f)
            tryVerify(function() { return f.editMenu.visible })
            compare(f.input.selectedText, "keep@example.org")
            const it = items(f)
            verify(it.editCopy.enabled, "Copy is enabled")
            verify(it.editCut.enabled, "Cut is enabled")
            it.editCopy.triggered()
            f.editMenu.close()
            helper.text = ""
            helper.paste()
            compare(helper.text, "keep@example.org")
        }

        function test_password_field_offers_paste_only() {
            helper.text = "x"
            helper.selectAll()
            helper.copy()
            const f = createTemporaryObject(fieldComponent, root, { text: "secret", password: true })
            waitForRendering(f)
            holdOn(f)
            tryVerify(function() { return f.editMenu.visible })
            const it = items(f)
            verify(it.editPaste !== undefined && it.editPaste.visible)
            for (const n of ["editCut", "editCopy", "editSelectAll"])
                verify(it[n] === undefined || !it[n].visible || !it[n].enabled, n + " not offered")
            f.editMenu.close()
        }

        function test_button_label_is_centred() {
            for (const icon of ["", "pencil"]) {
                const b = createTemporaryObject(buttonComponent, root, { iconName: icon })
                waitForRendering(b)
                let label = null
                const walk = function(it) {
                    if (it.objectName === "buttonLabel") label = it
                    for (let i = 0; i < it.children.length; i++) walk(it.children[i])
                    if (it.contentItem) walk(it.contentItem)
                }
                walk(b)
                verify(label !== null)
                const centre = label.mapToItem(b, label.width / 2, label.height / 2)
                verify(Math.abs(centre.x - b.width / 2) <= (icon === "" ? 1 : 16),
                       "label centre " + centre.x + " of " + b.width)
                verify(Math.abs(centre.y - b.height / 2) <= 1)
            }
        }

        function test_touch_targets_are_44() {
            const ib = createTemporaryObject(iconButtonComponent, root)
            verify(ib.width >= 44 && ib.height >= 44)
        }
    }

    TextField { id: helper; width: 100; y: 600 }
    Component { id: fieldComponent; Field { theme: thm; width: 300; y: 300 } }
    Field { id: pwField; theme: thm; password: true; width: 300; y: 400 }

    Component {
        id: buttonComponent
        AppButton { theme: thm; text: "Continue"; width: 300; y: 450 }
    }
    Component {
        id: iconButtonComponent
        IconButton { theme: thm; iconName: "back" }
    }
}
