import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"

// ListRow's built-in icon: the content moves to 60 px, the icon is centred on
// the row, the height stays the text's (48 minimum), and the colour follows
// the role (text, accent for destructive, secondary when disabled).
Item {
    id: root
    width: 400
    height: 400

    Theme { id: thm }

    ListRow {
        id: plain
        theme: thm
        width: 300
        Text { id: plainText; x: thm.gu(2.5); y: thm.gu(1.75); text: "Plain" }
    }
    ListRow {
        id: withIcon
        y: 60
        theme: thm
        width: 300
        iconName: "pencil"
        Text { id: iconText; x: thm.gu(2.5); y: thm.gu(1.75); text: "Rename" }
    }
    ListRow {
        id: tall
        y: 120
        theme: thm
        width: 300
        iconName: "download"
        Column {
            x: thm.gu(2.5)
            y: thm.gu(1.5)
            Text { text: "Save"; height: 20 }
            Text { text: "Into Files"; height: 40 }
        }
    }
    ListRow {
        id: danger
        y: 220
        theme: thm
        width: 300
        iconName: "trash-2"
        iconColor: thm.accent
        Text { x: thm.gu(2.5); y: thm.gu(1.75); text: "Remove" }
    }
    ListRow {
        id: off
        y: 280
        theme: thm
        width: 300
        enabled: false
        iconName: "copy"
        Text { x: thm.gu(2.5); y: thm.gu(1.75); text: "Copy" }
    }

    TestCase {
        name: "ListRowIcon"
        when: windowShown

        function test_row_without_icon_is_unchanged() {
            compare(plain.iconItem, null)
            compare(plain.implicitHeight, thm.touch)
            compare(plainText.mapToItem(plain, 0, 0).x, 20)
        }
        function test_icon_row_keeps_the_height_and_moves_the_text_to_60() {
            compare(withIcon.implicitHeight, thm.touch)
            compare(iconText.mapToItem(withIcon, 0, 0).x, 60)
        }
        function test_icon_is_24_at_the_20_inset_and_centred() {
            const ic = withIcon.iconItem
            verify(ic !== null)
            compare(ic.width, 24)
            compare(ic.mapToItem(withIcon, 0, 0).x, 20)
            compare(ic.mapToItem(withIcon, 0, 0).y, (withIcon.height - ic.height) / 2)
        }
        function test_height_comes_from_the_text_block() {
            // 60 px of text plus 28 px of padding; the icon is centred on it.
            compare(tall.implicitHeight, 88)
            compare(tall.iconItem.mapToItem(tall, 0, 0).y, (88 - 24) / 2)
        }
        function test_icon_colour_per_role() {
            compare(withIcon.iconItem.color, thm.text)
            compare(danger.iconItem.color, thm.accent)
            compare(off.iconItem.color, thm.secondary)
        }
    }
}
