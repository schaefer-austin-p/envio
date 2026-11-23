import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"

// The rule between list rows is the lighter rowRule role, not hairline
// (design audit R9): light and dark values per look.
Item {
    width: 400
    height: 100

    Theme { id: thm }
    ListRow {
        id: plain
        theme: thm
        width: 300
        Text { x: thm.gu(2.5); y: thm.gu(1.75); text: "Plain" }
    }
    ListRow {
        id: row
        lightRule: true
        theme: thm
        width: 300
        Text { x: thm.gu(2.5); y: thm.gu(1.75); text: "Row" }
    }

    TestCase {
        name: "ListRowRule"
        when: windowShown

        function rule(row) {
            for (let i = 0; i < row.background.children.length; i++)
                if (row.background.children[i].objectName === "rowRule")
                    return row.background.children[i]
            return null
        }
        function check(look, mode, expected) {
            thm.name = look
            thm.darkMode = mode
            const r = rule(row)
            verify(r !== null)
            compare(r.color.toString(), expected.toLowerCase())
            compare(thm.rowRule.toString(), expected.toLowerCase())
            verify(thm.rowRule.toString() !== thm.hairline.toString())
        }
        function test_default_row_keeps_the_hairline() {
            for (const [look, mode] of [["carbon", "light"], ["airmail", "dark"]]) {
                thm.name = look
                thm.darkMode = mode
                compare(rule(plain).color.toString(), thm.hairline.toString())
            }
        }
        function test_carbon_light() { check("carbon", "light", "#E4E1D8") }
        function test_airmail_light() { check("airmail", "light", "#EAE3D3") }
        function test_carbon_dark() { check("carbon", "dark", "#262B32") }
        function test_airmail_dark() { check("airmail", "dark", "#222C3A") }
    }
}
