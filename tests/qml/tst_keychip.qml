import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"

// #434: the shortcut chip is a keycap (variant B on the two-pane board): a
// skirt root, a gradient face 3 px above its bottom, a lighter top edge.
Item {
    id: root
    width: 200
    height: 100

    Theme { id: thm }
    KeyChip { id: chip; theme: thm; text: "Ctrl" }

    function same(a, b, msg) {
        tc.verify(Qt.colorEqual(a, b), msg + ": " + a + " vs " + b)
    }
    function find(item, name) {
        for (var i = 0; i < item.children.length; i++) {
            var c = item.children[i]
            if (c.objectName === name) return c
            var r = find(c, name)
            if (r) return r
        }
        return null
    }

    TestCase {
        id: tc
        name: "KeyChip"
        when: windowShown

        function check(palette, mode) {
            thm.name = palette
            thm.darkMode = mode
            var dark = mode === "dark"
            var face = find(chip, "keyFace")
            compare(chip.objectName, "keyChip")
            same(chip.color, dark ? Qt.darker(thm.ground, 1.55) : Qt.darker(thm.hairline, 1.6),
                 palette + " " + mode + " skirt")
            compare(face.gradient.stops.length, 2)
            same(face.gradient.stops[0].color, Qt.lighter(thm.surface, 1.10), "face top")
            same(face.gradient.stops[1].color, Qt.darker(thm.surface, dark ? 1.08 : 1.05), "face bottom")
            same(face.border.color, thm.hairline, "border")
            compare(face.y, 0)
            compare(chip.height - (face.y + face.height), 3)
            var top = find(chip, "keyTop")
            verify(top !== null, "keyTop exists")
            compare(top.height, 1)
            // Inset by the radius: it stays inside the rounded corners.
            compare(top.anchors.leftMargin, chip.radius)
            compare(top.anchors.rightMargin, chip.radius)
            same(top.color, Qt.rgba(1, 1, 1, dark ? 0.14 : 0.9), "top edge")
        }

        function test_carbon_light() { check("carbon", "light") }
        function test_carbon_dark() { check("carbon", "dark") }
        function test_airmail_light() { check("airmail", "light") }
        function test_airmail_dark() { check("airmail", "dark") }

        function test_face_follows_label() {
            var face = find(chip, "keyFace")
            var label = null
            for (var i = 0; i < face.children.length; i++)
                if (face.children[i].text !== undefined) label = face.children[i]
            verify(label !== null)
            compare(face.height, Math.max(22, label.implicitHeight + 6))
            verify(face.height >= 22)
            compare(chip.height, face.height + 3)
            verify(chip.width >= chip.height)
            // A taller label makes a taller face.
            var small = face.height
            label.font.pixelSize = 30
            verify(face.height > small)
            compare(face.height, label.implicitHeight + 6)
            label.font.pixelSize = 12
        }
    }
}
