import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"
import "../../qml/pages/accounts"
import "../../qml/pages/reader"
import "../../qml/bindle/Platform" as Platform

// #71: PageFooter pins the primary action to the page bottom (10 px above,
// 16 below, a 52 px button, a hairline only in Carbon) and lifts above the
// on-screen keyboard (#61: the field being typed in stays visible).
Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    MockCore { id: core }

    // A page with a Flickable over a footer, as the wizard steps are built.
    Item {
        id: page
        width: 400
        height: 700
        property alias rows: filler.height
        Flickable {
            id: flick
            anchors { top: parent.top; bottom: bar.top; left: parent.left; right: parent.right }
            clip: true
            contentHeight: filler.height + input.height
            Item { id: filler; width: 100; height: 100 }
            Field { id: input; theme: thm; y: filler.height; width: 280 }
        }
        Platform.KeyboardAvoider { id: avoider; flickable: flick }
        PageFooter {
            id: bar
            theme: thm
            AppButton { objectName: "footerButton"; theme: thm; primary: true; text: "Go"; width: parent.width }
        }
    }
    ReplyBar { id: replyBar; theme: thm; width: 400; y: 800 }
    WizardPage { id: wizard; theme: thm; core: core; width: 400; height: 700; visible: false }

    TestCase {
        name: "PageFooter"
        when: windowShown

        function everything(item, out) {
            out.push(item)
            if (item.contentItem !== undefined && item.contentItem !== null && item.contentItem !== item)
                everything(item.contentItem, out)
            for (const prop of ["data", "children"]) {
                if (item[prop] === undefined) continue
                for (let i = 0; i < item[prop].length; i++) everything(item[prop][i], out)
                break
            }
            return out
        }
        function find(name, from) {
            const hit = everything(from, []).filter(function(o) { return o.objectName === name })
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }
        function init() {
            thm.name = "carbon"
            thm.darkMode = "light"
            page.rows = 100
            flick.contentY = 0
            avoider.keyboard = Qt.rect(0, 0, 0, 0)
            bar.keyboard = Qt.rect(0, 0, 0, 0)
        }
        function cleanup() { input.input.focus = false; wizard.visible = false }

        function test_footer_metrics() {
            compare(bar.height, thm.footerPadTop + thm.controlHeight + thm.footerPadBottom)
            compare(thm.footerPadTop, 10)
            compare(thm.footerPadBottom, 16)
            const b = find("footerButton", bar)
            compare(b.height, thm.controlHeight)
            compare(b.mapToItem(bar, 0, 0).y, 10)
            compare(bar.height - (b.mapToItem(bar, 0, 0).y + b.height), 16)
        }
        function test_hairline_only_in_carbon() {
            verify(find("footerHairline", bar).visible)
            thm.name = "airmail"
            verify(!find("footerHairline", bar).visible)
            verify(!find("footerHairline", replyBar).visible)
            thm.name = "carbon"
            verify(find("footerHairline", replyBar).visible)
        }
        function test_button_at_the_bottom_when_content_is_short() {
            compare(bar.mapToItem(page, 0, bar.height).y, page.height)
            compare(find("footerButton", bar).mapToItem(page, 0, 0).y + thm.controlHeight + thm.footerPadBottom, page.height)
        }
        function test_button_stays_when_content_scrolls() {
            page.rows = 2000
            flick.contentY = 500
            compare(flick.contentY, 500)
            compare(bar.mapToItem(page, 0, bar.height).y, page.height)
            compare(flick.mapToItem(page, 0, flick.height).y, bar.y)
        }
        function test_focused_field_is_not_covered_by_footer_or_keyboard() {
            page.rows = 2000
            const kb = Qt.rect(0, 700 - 300, 400, 300)
            avoider.keyboard = kb
            bar.keyboard = kb
            input.input.forceActiveFocus()
            tryVerify(function() { return input.input.activeFocus })
            // The footer rides on the keyboard, not under it.
            compare(bar.mapToItem(page, 0, bar.height).y, kb.y)
            tryVerify(function() {
                return input.mapToItem(page, 0, input.height).y <= bar.mapToItem(page, 0, 0).y
            }, 5000, "the field is under the footer")
            verify(input.mapToItem(page, 0, 0).y >= 0, "the field is above the page")
            bar.keyboard = Qt.rect(0, 0, 0, 0)
            compare(bar.mapToItem(page, 0, bar.height).y, page.height)
        }
        function test_wizard_primary_actions_are_pinned() {
            wizard.visible = true
            for (const s of [["email", "continueButton"], ["signin", "signInContinue"],
                             ["servers", "connectButton"], ["options", "doneButton"]]) {
                wizard.step = s[0]
                const b = find(s[1], wizard)
                compare(b.mapToItem(wizard, 0, 0).y + thm.controlHeight + thm.footerPadBottom, wizard.height, s[0])
            }
        }
        function test_servers_step_footer_stays_while_scrolling_and_clears_keyboard() {
            wizard.visible = true
            wizard.step = "servers"
            const step = find("serversStep", wizard)
            const flickable = find("stepFlick", step)
            const button = find("connectButton", step)
            const field = find("serversPassword", step)
            const avoid = find("keyboardAvoider", step)
            const footer = find("pageFooter", step)
            flickable.contentY = Math.max(0, flickable.contentHeight - flickable.height)
            compare(button.mapToItem(wizard, 0, 0).y + thm.controlHeight + thm.footerPadBottom, wizard.height)
            const kb = Qt.rect(0, 700 - 300, 400, 300)
            avoid.keyboard = kb
            footer.keyboard = kb
            field.input.forceActiveFocus()
            tryVerify(function() { return field.input.activeFocus })
            tryVerify(function() {
                return field.mapToItem(wizard, 0, field.height).y <= footer.mapToItem(wizard, 0, 0).y
            }, 5000, "the password field is under the footer")
            verify(footer.mapToItem(wizard, 0, footer.height).y <= kb.y)
            field.input.focus = false
            avoid.keyboard = Qt.rect(0, 0, 0, 0)
            footer.keyboard = Qt.rect(0, 0, 0, 0)
        }
    }
}
