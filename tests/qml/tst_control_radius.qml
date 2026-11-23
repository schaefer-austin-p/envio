import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"
import "../../qml/pages/accounts"

// #68: controls take their shape from Theme (controlRadius, controlHeight,
// controlFont, field/chip/dialog/sheet radii). Carbon keeps its old values.
Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    AppButton { id: btn; theme: thm; text: "Go"; primary: true; width: 200 }
    Toast { id: toast; theme: thm }
    Chip { id: chip; theme: thm; text: "Work" }
    Field { id: field; theme: thm; width: 300 }
    AppDialog { id: dialog; theme: thm }
    Sheet { id: sheet; theme: thm }
    EditMenu { id: menu; theme: thm }
    Avatar { id: avatar; theme: thm; size: 44 }
    SignInStep { id: step; theme: thm; width: 360; height: 400; foundText: "imap.x.org" }

    // The first Rectangle-like item below `item` with a radius and a size.
    function rect(item) {
        var q = [item]
        while (q.length) {
            var i = q.shift()
            if (i && i.radius !== undefined && i.color !== undefined && i.width > 0) return i
            for (var k = 0; k < i.children.length; k++) q.push(i.children[k])
        }
        return null
    }

    TestCase {
        name: "ControlRadius"
        when: windowShown

        function init() { thm.name = "carbon"; thm.darkMode = "light" }

        function test_carbon_values_are_the_old_ones_in_both_modes() {
            var modes = ["light", "dark"]
            for (var m = 0; m < 2; m++) {
                thm.darkMode = modes[m]
                compare(thm.controlRadius, 8)
                compare(thm.controlHeight, thm.gu(6.5))
                compare(thm.fieldRadius, thm.gu(1))
                compare(thm.chipRadius, 6)
                compare(thm.cardRadius, thm.gu(1))
                compare(thm.menuRadius, thm.gu(1.5))
                compare(thm.optionRadius, 6)
                compare(thm.checkRadius, 4)
                compare(thm.avatarRadius(44), 22)
                compare(thm.dialogRadius, thm.gu(1.5))
                compare(thm.sheetRadius, thm.gu(2))
                compare(thm.controlFont.family, thm.fontMono)
                compare(thm.controlFont.pixelSize, 15)
                compare(thm.controlFont.weight, Font.Medium)
                compare(btn.background.radius, 8)
                compare(toast.radius, thm.gu(1))
            }
        }

        function test_compact_control_height_and_radius() {
            compare(thm.compactControlHeight, 44)
            compare(thm.compactControlRadius, 8)
            thm.name = "airmail"
            compare(thm.compactControlHeight, 44)
            compare(thm.compactControlRadius, thm.compactControlHeight / 2)
            compare(thm.compactControlRadius, 22)
        }

        function test_airmail_values() {
            thm.name = "airmail"
            compare(thm.controlHeight, 52)
            compare(thm.controlRadius, 26)
            compare(thm.controlFont.family, thm.fontUi)
            compare(thm.controlFont.weight, Font.DemiBold)
            compare(thm.controlFont.pixelSize, 15)
            compare(thm.fieldRadius, 10)
            compare(thm.chipRadius, 6)
            compare(thm.cardRadius, 10)
            compare(thm.menuRadius, 10)
            compare(thm.optionRadius, 10)
            compare(thm.avatarRadius(44), 4)
            compare(thm.checkRadius, 4)
            compare(thm.dialogRadius, 20)
            compare(thm.sheetRadius, 20)
            verify(thm.controlHeight >= thm.touch)
        }

        function test_components_follow_the_theme_in_each_theme() {
            var names = ["carbon", "airmail"]
            for (var i = 0; i < 2; i++) {
                thm.name = names[i]
                compare(btn.background.radius, thm.controlRadius)
                compare(btn.height, thm.controlHeight)
                compare(toast.radius, thm.airmail ? toast.height / 2 : thm.controlRadius)
                compare(menu.background.radius, thm.menuRadius)
                compare(avatar.radius, thm.avatarRadius(avatar.size))
                compare(findChild(step, "foundText").parent.radius, thm.cardRadius)
                compare(dialog.background.radius, thm.dialogRadius)
                compare(sheet.background.radius, thm.sheetRadius)
                compare(root.rect(field).radius, thm.fieldRadius)
                compare(root.rect(chip.background).radius, thm.chipRadius)
                verify(btn.height >= thm.touch)
            }
            thm.name = "airmail"
            compare(toast.radius, toast.height / 2)
        }

        function test_button_label_uses_the_control_font() {
            var names = ["carbon", "airmail"]
            for (var i = 0; i < 2; i++) {
                thm.name = names[i]
                var label = findChild(btn, "buttonLabel")
                compare(label.font.family, thm.controlFont.family)
                compare(label.font.pixelSize, 15)
                compare(label.font.weight, thm.controlFont.weight)
            }
        }
    }
}
