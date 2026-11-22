import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"
import "../../qml/pages"

// #438: the shortcuts dialog and the empty-reader hint follow frame 7 of
// the two-pane board: a surface card, 48 px row pitch (rows with 25 px chips),
// 8 px radius, 22 px side padding, a 16 px title, a 40 px Close, a 14 px hint
// with 4 px between its text and chips.
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
            waitForRendering(dialog.contentItem)
            wait(100)
            const card = dialog.background
            verify(Qt.colorEqual(card.color, thm.surface), tag + " card is the surface colour: " + card.color)
            verify(!Qt.colorEqual(card.color, thm.ground), tag + " card differs from the ground")
            verify(Qt.colorEqual(card.border.color, thm.field), tag + " card border")
            compare(card.radius, palette === "carbon" ? 8 : 10)
            compare(dialog.leftPadding, 22)
            compare(dialog.rightPadding, 22)
            const rows = all(dialog.contentItem, "shortcutRow")
            compare(rows.length, 6)
            // The distance between consecutive rows is the pitch.
            for (let i = 1; i < rows.length; i++) {
                const pitch = rows[i].y - rows[i - 1].y
                verify(pitch >= 47 && pitch <= 50, tag + " distance between rows " + pitch)
            }
            const title = dialog.contentItem.children[0]
            compare(title.font.pixelSize, 16)
            compare(title.font.weight, Font.DemiBold)
            const close = find(dialog.contentItem, "secondaryAction")
            const lastRule = rows[5].mapToItem(null, 0, rows[5].height).y
            const first = rows[0].mapToItem(null, 0, 0).y
            const titleBottom = title.mapToItem(null, 0, title.height).y
            compare(first - titleBottom, 6)
            compare(close.mapToItem(null, 0, 0).y - lastRule, 12)
            compare(dialog.bottomPadding, 10)
            if (palette === "carbon") {
                compare(close.height, 40)
                compare(close.background.height, 40)
                compare(close.background.radius, 8)
                compare(close.sidePadding, 20)
                compare(close.labelFont.pixelSize, 14)
            }
            // 6 px between the chips of a row.
            const chipRow = rows[3].children[2]
            compare(chipRow.spacing, 6)
            const cs = all(chipRow, "keyChip")
            compare(cs[1].x - (cs[0].x + cs[0].width), 6)
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
            waitForRendering(pane)
            const before = find(pane, "emptyHintBefore")
            const after = find(pane, "emptyHintAfter")
            verify(before !== null && after !== null, "hint texts exist")
            compare(before.font.pixelSize, 14)
            compare(after.font.pixelSize, 14)
            const chips = all(pane, "keyChip")
            compare(chips.length, 2)
            // 4 px between "Or press", the chips and "to write one."; the
            // texts hold no spaces of their own.
            verify(before.text === before.text.trim() && after.text === after.text.trim(), "texts are trimmed")
            const chipRow = find(pane, "emptyHintChips")
            compare(chipRow.spacing, 4)
            compare(chipRow.x - (before.x + before.width), 4)
            compare(after.x - (chipRow.x + chipRow.width), 4)
            compare(chips[1].x - (chips[0].x + chips[0].width), 4)
        }
    }
}
