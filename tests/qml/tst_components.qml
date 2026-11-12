import QtQuick 2.15
import QtTest 1.2
import QtQuick.Controls 2.15
import "../../qml/platform"
import "../../qml/components"
import "../../qml/components/fileSize.js" as FileSize
import "../../qml/platform/stampTint.js" as StampTint

Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    Avatar { id: stamp; theme: thm; initials: "LU" }

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

        function test_stamp_tint_is_stable_per_address() {
            compare(StampTint.isBlue("lucia@example.org"), StampTint.isBlue("LUCIA@example.org"))
            compare(StampTint.isBlue("Lucia"), true)
            compare(StampTint.isBlue("Ann"), false)
        }
        function test_stamp_initials() {
            compare(StampTint.initials("Green Coast Trails"), "GC")
            compare(StampTint.initials("lucia"), "L")
            compare(StampTint.initials(""), "")
        }
        function test_avatar_is_a_circle_in_carbon_and_a_stamp_in_airmail() {
            thm.name = "carbon"
            compare(stamp.width, thm.avatarSize)
            compare(stamp.height, thm.avatarSize)
            compare(stamp.stampVisible, false)
            thm.name = "airmail"
            compare(stamp.width, thm.stampWidth)
            compare(stamp.height, thm.stampHeight)
            compare(thm.stampWidth, 40)
            compare(thm.stampHeight, 48)
            compare(stamp.stampVisible, true)
            thm.name = "carbon"
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

        // The rows read the field once, as the menu opens (no live bindings:
        // they looped on the phone), and again on the next opening.
        function test_edit_menu_rows_are_read_when_it_opens() {
            const f = createTemporaryObject(fieldComponent, root, {})
            waitForRendering(f)
            holdOn(f)
            tryVerify(function() { return f.editMenu.visible })
            compare(items(f).editSelectAll.enabled, false)
            f.input.text = "abc"
            compare(items(f).editSelectAll.enabled, false)
            f.editMenu.close()
            tryVerify(function() { return !f.editMenu.visible })
            holdOn(f)
            tryVerify(function() { return f.editMenu.visible })
            compare(items(f).editSelectAll.enabled, true)
            f.editMenu.close()
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

        function test_file_size_picks_the_unit_after_rounding() {
            compare(FileSize.fileSize(812), "812 B")
            compare(FileSize.fileSize(4200), "4.2 kB")
            compare(FileSize.fileSize(9999), "10 kB")
            compare(FileSize.fileSize(84000), "84 kB")
            compare(FileSize.fileSize(999499), "999 kB")
            compare(FileSize.fileSize(999999), "1.0 MB")
            compare(FileSize.fileSize(999999999), "1.0 GB")
            compare(FileSize.fileSize(2000000000), "2.0 GB")
        }

        // #61: with the keyboard over the bottom of a form, the focused
        // field is scrolled above it. A simulated keyboard rectangle: the
        // offscreen platform has no input panel.
        function test_focused_field_scrolls_above_the_keyboard() {
            kbFlick.contentY = 0
            compare(avoider.covered, 0)
            avoider.keyboard = Qt.rect(0, 100 + 300 - 150, 400, 150)
            compare(avoider.covered, 150)
            kbField.input.forceActiveFocus()
            tryVerify(function() { return kbField.input.activeFocus })
            const bottom = kbField.mapToItem(kbFlick, 0, kbField.height).y
            const top = kbField.mapToItem(kbFlick, 0, 0).y
            verify(bottom <= kbFlick.height - 150, "field bottom " + bottom + " is under the keyboard")
            verify(top >= 0)
            avoider.keyboard = Qt.rect(0, 0, 0, 0)
            compare(avoider.covered, 0)
            kbField.input.focus = false
        }

        // The compose body is a TextArea: the cursor line, not the whole
        // (taller than the screen) text, ends up above the keyboard, and the
        // view is back within its bounds once the keyboard hides.
        function test_text_area_cursor_scrolls_above_the_keyboard() {
            kbFlick.contentY = 0
            kbArea.text = new Array(31).join("a line of the body\n")
            avoider.keyboard = Qt.rect(0, 100 + 300 - 150, 400, 150)
            kbArea.forceActiveFocus()
            tryVerify(function() { return kbArea.activeFocus })
            kbArea.cursorPosition = kbArea.length
            const cursor = kbArea.cursorRectangle
            tryVerify(function() {
                return kbArea.mapToItem(kbFlick, 0, cursor.y + cursor.height).y <= kbFlick.height - 150
            }, 5000, "the cursor line is under the keyboard")
            verify(kbArea.mapToItem(kbFlick, 0, cursor.y).y >= 0, "the cursor line is above the view")
            verify(kbFlick.contentY > 0, "the view did not scroll")
            avoider.keyboard = Qt.rect(0, 0, 0, 0)
            compare(avoider.covered, 0)
            const most = Math.max(0, kbFlick.contentHeight - kbFlick.height)
            tryVerify(function() { return kbFlick.contentY >= 0 && kbFlick.contentY <= most })
            kbArea.focus = false
            kbArea.text = ""
        }

        // #64: the icon keeps its own size, in grid units, centred in the button.
        function test_icon_button_icon_is_theme_icon_size() {
            const ib = createTemporaryObject(iconButtonComponent, root)
            const icon = ib.contentItem.children[0]
            compare(icon.width, thm.iconSize)
            compare(icon.height, thm.iconSize)
            verify(icon.width < ib.width)
            compare(thm.iconSize, thm.gu(3))
            compare(thm.iconSmall, thm.gu(2))
            compare(icon.x + icon.width / 2, ib.width / 2)
        }

        // The icon inside an AppButton is 20 px (boards: carbon-inbox, carbon-signin).
        function test_app_button_icon_is_icon_button_size() {
            const b = createTemporaryObject(appButtonIconComponent, root)
            const icon = b.contentItem.children[0].children[0]
            compare(icon.width, thm.iconButton)
            compare(thm.iconButton, thm.gu(2.5))
            verify(thm.iconButton > thm.iconSmall && thm.iconButton < thm.iconSize)
        }

        // Phone: a sheet whose content grew after it opened kept its first height
        // and rested at the screen's bottom edge with only its handle showing.
        function test_sheet_whose_content_grows_ends_at_the_window_bottom() {
            growSheet.open()
            tryVerify(function() { return growSheet.opened })
            growBody.height = 240
            tryVerify(function() { return growSheet.height >= 240 })
            compare(growSheet.y + growSheet.height, growSheet.parent.height)
            growSheet.close()
            tryVerify(function() { return !growSheet.visible })
            growBody.height = 0
        }

        function test_touch_targets_are_44() {
            const ib = createTemporaryObject(iconButtonComponent, root)
            verify(ib.width >= 44 && ib.height >= 44)
        }
    }

    Flickable {
        id: kbFlick
        y: 100
        width: 300
        height: 300
        contentHeight: 1200
        clip: true
        Field { id: kbField; theme: thm; y: 900; width: 280 }
        TextArea { id: kbArea; y: 200; width: 280 }
        KeyboardAvoider { id: avoider; flickable: kbFlick }
    }

    TextField { id: helper; width: 100; y: 600 }
    Component { id: fieldComponent; Field { theme: thm; width: 300; y: 300 } }
    Field { id: pwField; theme: thm; password: true; width: 300; y: 400 }

    Component {
        id: buttonComponent
        AppButton { theme: thm; text: "Continue"; width: 300; y: 450 }
    }
    Sheet {
        id: growSheet
        theme: thm
        title: "Grows"
        Item { id: growBody; width: 10; height: 0 }
    }
    Component {
        id: appButtonIconComponent
        AppButton { theme: thm; text: "New message"; iconName: "compose"; width: 300; y: 450 }
    }
    Component {
        id: iconButtonComponent
        IconButton { theme: thm; iconName: "back" }
    }
}
