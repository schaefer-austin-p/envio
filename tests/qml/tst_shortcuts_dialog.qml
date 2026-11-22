import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"
import "../../qml/pages"

// #438: the shortcuts dialog and the empty-reader hint follow frame 7 of
// the two-pane board: a surface card, 41 px row pitch, 8 px radius, 22 px
// side padding, a 14 px hint with 4 px between its chips.
Item {
    id: root
    width: 400
    height: 600

    Theme { id: thm }
    ShortcutsDialog { id: dialog; theme: thm }
    ReaderPane { id: pane; theme: thm; width: 400; height: 400 }

    TestCase {
        id: tc
        name: "ShortcutsDialogBoard"
        when: windowShown

        function kidsOf(item) {
            return (item.contentItem && item.contentItem !== item ? [item.contentItem] : []).concat(Array.from(item.children || []))
        }
        function find(item, name) {
            for (const k of kidsOf(item)) {
                if (k.objectName === name) return k
                const r = find(k, name)
                if (r) return r
            }
            return null
        }
        function all(item, name, out) {
            out = out || []
            for (const k of kidsOf(item)) {
                if (k.objectName === name) out.push(k)
                all(k, name, out)
            }
            return out
        }
        function check(palette, mode) {
            thm.name = palette
            thm.darkMode = mode
            const tag = palette + " " + mode
            dialog.open()
            tryVerify(function() { return dialog.opened })
            const card = dialog.background
            verify(Qt.colorEqual(card.color, thm.surface), tag + " card is the surface colour: " + card.color)
            verify(!Qt.colorEqual(card.color, thm.ground), tag + " card differs from the ground")
            verify(Qt.colorEqual(card.border.color, thm.field), tag + " card border")
            compare(card.radius, palette === "carbon" ? 8 : 10)
            compare(dialog.leftPadding, 22)
            compare(dialog.rightPadding, 22)
            const rows = all(dialog.contentItem, "shortcutRow")
            compare(rows.length, 6)
            for (const r of rows) {
                // As on the board: 11 px padding twice, the 1 px rule, and the 25 px chip (22 plus its 3 px skirt) as the tallest part.
                verify(r.height >= 47 && r.height <= 50, tag + " row pitch " + r.height)
            }
            dialog.close()
            tryVerify(function() { return !dialog.visible })
        }
        function test_carbon_light() { check("carbon", "light") }
        function test_carbon_dark() { check("carbon", "dark") }
        function test_airmail_light() { check("airmail", "light") }

        function test_hint() {
            const before = find(pane, "emptyHintBefore")
            const after = find(pane, "emptyHintAfter")
            verify(before !== null && after !== null, "hint texts exist")
            compare(before.font.pixelSize, 14)
            compare(after.font.pixelSize, 14)
            const chips = all(pane, "keyChip")
            compare(chips.length, 2)
            // One plain space before the chips, 4 px between them.
            verify(before.text.endsWith(" "), "space before the chips")
            verify(after.text.startsWith(" "), "space after the chips")
            compare(find(pane, "emptyHintChips").spacing, 4)
        }
    }
}
