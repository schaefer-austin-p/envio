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
    ListRow {
        id: chevRow
        theme: thm
        // Shares the plain row's spot (nothing else is hit-testable lower
        // down); the tests swap which of the two shows.
        visible: false
        width: 300
        chevron: true
        Text { text: "Appearance" }
    }
    SignalSpy { id: chevRowClicked; target: chevRow; signalName: "clicked" }
    Chevron { id: lone; theme: thm; y: 300 }
    SignalSpy { id: rowHeld; target: row; signalName: "pressAndHold" }
    property bool boundOn: true
    ListRow {
        id: boundRow
        theme: thm
        visible: false
        width: 300
        enabled: root.boundOn
        Text { text: "Bound" }
    }
    SignalSpy { id: boundClicked; target: boundRow; signalName: "clicked" }

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

        // Runs even when an assertion failed, so an open dialog cannot
        // swallow the clicks of later tests.
        function cleanup() {
            dialog.close()
            dialog.message = "Cancel is the safe choice."
        }

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
        function chevronOf(r) {
            for (let i = 0; i < r.children.length; i++) {
                const c = r.children[i]
                if (c.objectName === "chevron") return c
                for (let j = 0; j < c.children.length; j++)
                    if (c.children[j].objectName === "chevron") return c.children[j]
            }
            return null
        }
        function test_chevron_is_the_small_icon_in_secondary() {
            for (const look of ["carbon", "airmail"]) {
                for (const dark of [false, true]) {
                    thm.name = look
                    thm.darkMode = dark ? "dark" : "light"
                    compare(lone.width, thm.iconSmall)
                    compare(lone.color, thm.secondary)
                }
            }
            thm.name = "carbon"
            thm.darkMode = "system"
        }
        function test_chevron_points_right_or_down_by_direction() {
            compare(lone.direction, "forward")
            compare(lone.name, "forward")
            lone.direction = "down"
            compare(lone.name, "down")
            lone.direction = "forward"
        }
        function test_a_plain_row_has_no_chevron() {
            compare(chevronOf(row), null)
        }
        function test_a_row_with_a_chevron_shows_it_at_the_trailing_edge() {
            row.visible = false
            chevRow.visible = true
            const c = chevronOf(chevRow)
            verify(c !== null)
            verify(c.visible)
            verify(c.width > 0)
            verify(c.mapToItem(chevRow, 0, 0).x + c.width <= chevRow.width)
            verify(c.mapToItem(chevRow, 0, 0).x > chevRow.width / 2)
            compare(c.color, thm.secondary)
            chevRow.visible = false
            row.visible = true
        }
        function test_the_chevron_is_decoration_the_row_takes_the_tap() {
            row.visible = false
            chevRow.visible = true
            const c = chevronOf(chevRow)
            verify(c !== null)
            chevRowClicked.clear()
            mouseClick(chevRow, 10, 10)
            compare(chevRowClicked.count, 1, "the row itself takes a tap")
            chevRowClicked.clear()
            const p = c.mapToItem(chevRow, c.width / 2, c.height / 2)
            mouseClick(chevRow, p.x, p.y)
            compare(chevRowClicked.count, 1)
            chevRow.visible = false
            row.visible = true
        }
        function test_stamp_tint_comes_from_the_address() {
            compare(StampTint.addressOf("Ann <a@x.org>"), "a@x.org")
            compare(StampTint.addressOf("  A@X.org "), "a@x.org")
            const t = StampTint.isBlue(StampTint.addressOf("a@x.org"))
            compare(StampTint.isBlue(StampTint.addressOf("Ann <a@x.org>")), t)
            compare(StampTint.isBlue(StampTint.addressOf("Ann Lee <A@X.org>")), t)
            compare(StampTint.initials("Ann Lee <a@x.org>"), "AL")
        }
        function test_stamp_mask_has_holes_on_the_edge() {
            thm.name = "airmail"
            const canvas = findChild(stamp, "stampCanvas")
            const alpha = function(x, y) { return canvas.getContext("2d").getImageData(x, y, 1, 1).data[3] }
            tryVerify(function() { return alpha(20, 24) > 0 })
            compare(alpha(8, 0), 0)
            compare(alpha(0, 8), 0)
            verify(alpha(4, 0) > 0)
            verify(alpha(4, 4) > 0)
            thm.name = "carbon"
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

        // A sideways drag cancels the tap without replacing the row's own
        // `enabled` binding (the From row's follows the account count).
        function test_sideways_drag_keeps_an_enabled_binding() {
            const far = Qt.styleHints.startDragDistance * 3
            row.visible = false
            boundRow.visible = true
            boundClicked.clear()
            mouseClick(boundRow, 20, boundRow.height / 2)
            compare(boundClicked.count, 1, "a tap opens")
            boundClicked.clear()
            mousePress(boundRow, 20, boundRow.height / 2)
            mouseMove(boundRow, 20 + far / 2, boundRow.height / 2)
            mouseMove(boundRow, 20 + far, boundRow.height / 2)
            mouseRelease(boundRow, 20 + far, boundRow.height / 2)
            compare(boundClicked.count, 0, "the swipe is not a tap")
            root.boundOn = false
            compare(boundRow.enabled, false, "the binding still disables the row")
            root.boundOn = true
            compare(boundRow.enabled, true)
            boundRow.visible = false
            row.visible = true
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

        function messageText() {
            return everything(dialog.contentItem, []).filter(function(o) { return o.text === dialog.message })[0]
        }
        function test_dialog_message_wraps_an_unbroken_word() {
            dialog.message = "New fingerprint: 03:10:17:24:31:38:45:52:59:66:73:80:87:94:01:08:15:22:29:36:43:50:57:64:71:78:85:92:99:06:13:20"
            dialog.open()
            tryVerify(function() { return dialog.opened })
            const t = messageText()
            verify(t.contentWidth <= t.width, "contentWidth " + t.contentWidth + " > width " + t.width)
            verify(t.lineCount >= 3)
        }
        // Ordinary prose still breaks at spaces: it wraps exactly as WordWrap would.
        function test_dialog_message_keeps_word_breaks() {
            dialog.message = "The server presented a certificate that is not the one you trusted before, so check it first."
            dialog.open()
            tryVerify(function() { return dialog.opened })
            const t = messageText()
            const ref = createTemporaryObject(wordWrapComponent, root, { text: t.text, width: t.width, fontFamily: t.font.family, pixelSize: t.font.pixelSize })
            verify(t.lineCount >= 2)
            compare(t.lineCount, ref.lineCount)
            compare(t.contentWidth, ref.contentWidth)
        }
        function test_banner_wraps_an_unbroken_word() {
            const b = createTemporaryObject(bannerComponent, root)
            verify(b)
            const label = everything(b, []).filter(function(o) { return o.text === b.text && o.wrapMode !== undefined })[0]
            verify(label.contentWidth <= label.width, "contentWidth " + label.contentWidth + " > width " + label.width)
            verify(label.lineCount >= 2)
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

        // Select all goes through the field's HoldSelection (bindle): the
        // menu comes back with Cut and Copy live, as there is no drag to
        // select on the phone.
        function test_select_all_opens_the_menu_again_with_copy_live() {
            for (const c of [fieldComponent, textBlockComponent]) {
                const f = createTemporaryObject(c, root, { text: "all@example.org" })
                waitForRendering(f)
                holdOn(f)
                tryVerify(function() { return f.editMenu.visible })
                verify(!items(f).editCopy.enabled, "nothing selected yet")
                const selectAll = items(f).editSelectAll
                f.editMenu.close()
                tryVerify(function() { return !f.editMenu.visible })
                selectAll.triggered()
                tryVerify(function() { return f.editMenu.visible })
                compare(f.input.selectedText, "all@example.org")
                verify(items(f).editCopy.enabled, "Copy is enabled")
                f.editMenu.close()
                tryVerify(function() { return !f.editMenu.visible })
            }
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

        // A sheet taller than the window keeps clear of the ring's top band in
        // Airmail (y >= frameInset); Carbon's inset is 0, so it may reach 0.
        function test_tall_sheet_keeps_the_ring_top_band_data() {
            return [{ tag: "carbon", airmail: false }, { tag: "airmail", airmail: true }]
        }
        function test_tall_sheet_keeps_the_ring_top_band(data) {
            thm.name = data.airmail ? "airmail" : "carbon"
            growSheet.open()
            tryVerify(function() { return growSheet.opened })
            growBody.height = 3000
            tryVerify(function() { return growSheet.implicitHeight > growSheet.parent.height })
            verify(growSheet.y >= thm.frameInset, "y " + growSheet.y)
            if (data.airmail) compare(growSheet.y, thm.frameInset)
            else compare(growSheet.y, 0)
            growSheet.close()
            tryVerify(function() { return !growSheet.visible })
            growBody.height = 0
            thm.name = "carbon"
        }

        // #83: a sheet taller than the window is capped at parent.height -
        // frameInset and scrolls, so its last row can be reached.
        function test_tall_sheet_is_capped_and_scrolls_to_its_last_row() {
            tallSheet.open()
            tryVerify(function() { return tallSheet.opened })
            tryVerify(function() { return tallSheet.implicitHeight > tallSheet.parent.height })
            const cap = tallSheet.parent.height - thm.frameInset
            compare(tallSheet.height, cap)
            verify(tallSheet.y >= thm.frameInset, "y " + tallSheet.y)
            verify(tallSheet.y + tallSheet.height <= tallSheet.parent.height, "bottom runs off")
            const flick = tallSheet.contentItem
            verify(flick.interactive, "a tall sheet flicks")
            // The last row starts below the sheet's bottom until scrolled.
            const bottomOf = function() {
                return tallLast.mapToItem(tallSheet.contentItem, 0, tallLast.height).y
            }
            verify(bottomOf() > flick.height, "last row starts out of view")
            flick.contentY = flick.contentHeight - flick.height
            tryVerify(function() { return bottomOf() <= flick.height })
            verify(tallLast.mapToItem(null, 0, tallLast.height).y <= tallSheet.parent.height)
            tallSheet.close()
            tryVerify(function() { return !tallSheet.visible })
        }

        // A sheet shorter than the window keeps its natural height and does not flick.
        function test_short_sheet_does_not_flick() {
            shortSheet.open()
            tryVerify(function() { return shortSheet.opened })
            compare(shortSheet.height, shortSheet.implicitHeight)
            verify(!shortSheet.contentItem.interactive)
            shortSheet.close()
            tryVerify(function() { return !shortSheet.visible })
        }

        // A sheet whose parent does not start at window y 0 rests on the keyboard's top.
        function test_sheet_inset_is_measured_in_window_coordinates() {
            offsetSheet.keyboard = Qt.rect(0, 400, 400, 200)
            const bottom = offsetParent.mapToItem(null, 0, offsetParent.height).y
            fuzzyCompare(offsetSheet.keyboardInset, bottom - 400, 0.01)
            verify(offsetSheet.keyboardInset > 0)
            offsetSheet.keyboard = Qt.rect(0, 0, 0, 0)
            compare(offsetSheet.keyboardInset, 0)
        }

        function test_touch_targets_are_44() {
            const ib = createTemporaryObject(iconButtonComponent, root)
            verify(ib.width >= 44 && ib.height >= 44)
        }
    }

    Item {
        id: offsetParent
        y: 100
        width: 400
        height: 400
    }
    Sheet { id: offsetSheet; theme: thm; parent: offsetParent }

    TextField { id: helper; width: 100; y: 600 }
    Component { id: fieldComponent; Field { theme: thm; width: 300; y: 300 } }
    Component { id: textBlockComponent; TextBlock { theme: thm; width: 300; y: 300 } }
    Field { id: pwField; theme: thm; password: true; width: 300; y: 400 }

    Component {
        id: bannerComponent
        Banner { theme: thm; width: 300; text: "Certificate: 03:10:17:24:31:38:45:52:59:66:73:80:87:94:01:08:15:22:29:36:43:50:57:64:71:78:85:92:99:06:13:20" }
    }
    Component {
        id: wordWrapComponent
        Text {
            property string fontFamily: ""
            property int pixelSize: 15
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            lineHeight: 1.3
            font.family: fontFamily
            font.pixelSize: pixelSize
        }
    }
    Component {
        id: buttonComponent
        AppButton { theme: thm; text: "Continue"; width: 300; y: 450 }
    }
    Sheet {
        id: tallSheet
        theme: thm
        title: "Tall"
        Repeater { model: 30; Rectangle { width: 100; height: 44; color: "#ccc" } }
        Rectangle { id: tallLast; width: 100; height: 44; color: "#f00" }
    }
    Sheet { id: shortSheet; theme: thm; Item { width: 10; height: 80 } }
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
