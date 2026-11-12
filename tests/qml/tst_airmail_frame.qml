import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"
import "../../qml/pages"

// #65: Airmail's ring (AirmailFrame) around the pages, Sheets and dialogs.
// Carbon keeps inset 0 and draws no Canvas.
Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    Component { id: coreComponent; MockCore {} }
    AirmailFrame {
        id: frame
        theme: thm
        width: 200
        height: 300
        Rectangle { anchors.fill: parent; color: "#00ff00" }
    }
    Sheet { id: sheet; theme: thm; title: "Sheet"; Item { width: 10; height: 40 } }
    AppDialog { id: dialog; theme: thm; title: "Dialog"; message: "Hello"; primaryText: "Ok" }

    TestCase {
        name: "AirmailFrame"
        when: windowShown

        function init() { thm.name = "carbon"; thm.darkMode = "light" }
        function cleanup() { sheet.close(); dialog.close() }

        function alpha(canvas, x, y) {
            return canvas.getContext("2d").getImageData(x, y, 1, 1).data[3]
        }
        function test_theme_roles() {
            compare(thm.frameInset, 0)
            thm.name = "airmail"
            compare(thm.frameInset, 10)
            compare(thm.frameOuterRadius, 30)
            compare(thm.frameInnerRadius, 20)
        }

        function test_carbon_has_no_ring_and_no_inset() {
            verify(!frame.ring.visible)
            compare(frame.area.x, 0)
            compare(frame.area.y, 0)
            compare(frame.area.width, 200)
            compare(frame.area.height, 300)
        }

        function test_airmail_ring_and_inset() {
            thm.name = "airmail"
            verify(frame.ring.visible)
            compare(frame.area.x, thm.frameInset)
            compare(frame.area.y, thm.frameInset)
            compare(frame.area.width, 200 - 2 * thm.frameInset)
            compare(frame.area.height, 300 - 2 * thm.frameInset)
        }

        // The band is drawn, the corner outside the outer radius and the
        // middle of the page are not, and the stripes follow the order red,
        // paper, blue, paper at 135 degrees (x + y = 14 * sqrt(2) * k).
        function test_ring_pixels() {
            thm.name = "airmail"
            const c = frame.ring
            tryVerify(function() { return c.available })
            c.requestPaint()
            wait(100)
            tryVerify(function() { return alpha(c, 100, 4) === 255 }, 2000)
            compare(alpha(c, 100, 4), 255)         // top band
            compare(alpha(c, 4, 150), 255)         // left band
            compare(alpha(c, 195, 150), 255)       // right band
            compare(alpha(c, 100, 150), 0)         // inside the ring
            compare(alpha(c, 1, 1), 0)             // outside the outer radius
            compare(alpha(c, 12, 12), 255)         // the band covers the content corner
            compare(alpha(c, 17, 17), 0)           // inside the inner arc
            // No seam: the ring is whole along the top and the left edge.
            for (let x = 40; x < 160; x += 20) compare(alpha(c, x, 5), 255)
            for (let y = 40; y < 260; y += 20) compare(alpha(c, 5, y), 255)
            // Stripes along x + y: 14 * sqrt(2) = 19.8 wide; red from 79.2 to
            // 99, paper to 39.6 before it, blue from 39.6 to 59.4.
            const ctx = c.getContext("2d")
            compare(ctx.getImageData(80, 5, 1, 1).data[0], 0xB2)
            compare(ctx.getImageData(30, 5, 1, 1).data[0], 0xFB)
            compare(ctx.getImageData(50, 5, 1, 1).data[0], 0x1F)
        }

        function test_shell_page_width() {
            thm.name = "airmail"
            const c = coreComponent.createObject(root)
            c.accounts.append({ key: "ann", name: "Ann", email: "ann@example.org", status: "", state: "ok", unread: 0, unreadText: "" })
            const s = shellComponent.createObject(root, {
                theme: thm, core: c, accounts: c.accounts, folders: c.folders,
                messages: c.messages, outbox: c.outbox, suggestions: c.suggestions,
                searchResults: c.searchResults
            })
            c.ready = true
            tryVerify(function() { return s.depth === 1 && s.currentPage })
            tryCompare(s.currentPage, "width", 400 - 2 * thm.frameInset)
            thm.name = "carbon"
            tryCompare(s.currentPage, "width", 400)
            s.destroy()
            c.destroy()
        }

        // One ring, the shell's: a Sheet or dialog has no frame of its own,
        // shows its own background, and lies inside the ring's inner area.
        function test_sheet_lies_inside_the_one_ring() {
            thm.name = "airmail"
            sheet.open()
            tryVerify(function() { return sheet.opened })
            verify(sheet.background.visible)
            compare(sheet.background.radius, thm.sheetRadius)
            verify(!sheet.contentItem.ring)
            compare(sheet.x, thm.frameInset)
            compare(sheet.width, 400 - 2 * thm.frameInset)
            compare(sheet.y + sheet.height, 700 - thm.frameInset)
        }

        function test_dialog_lies_inside_the_one_ring() {
            thm.name = "airmail"
            dialog.open()
            tryVerify(function() { return dialog.opened })
            verify(dialog.background.visible)
            compare(dialog.background.radius, thm.dialogRadius)
            verify(!dialog.contentItem.ring)
            verify(dialog.x >= thm.frameInset)
            verify(dialog.x + dialog.width <= 400 - thm.frameInset)
            compare(dialog.padding, thm.gu(2.5))
        }

        function test_carbon_sheet_and_dialog_geometry_is_unchanged() {
            sheet.open()
            dialog.open()
            tryVerify(function() { return sheet.opened && dialog.opened })
            compare(sheet.x, 0)
            compare(sheet.width, 400)
            compare(sheet.y + sheet.height, 700)
            compare(sheet.background.radius, thm.gu(2))
            compare(dialog.x, thm.gu(2))
            compare(dialog.y, thm.gu(15))
            compare(dialog.width, 400 - thm.gu(4))
            compare(dialog.padding, thm.gu(2.5))
            compare(dialog.background.radius, thm.gu(1.5))
        }

        function test_dark_mode_repaints_the_stripes() {
            thm.name = "airmail"
            const c = frame.ring
            tryVerify(function() { return c.available })
            c.requestPaint()
            tryVerify(function() { return c.getContext("2d").getImageData(80, 5, 1, 1).data[0] === 0xB2 }, 2000)
            thm.darkMode = "dark"
            // Dark red is #C4473F.
            tryVerify(function() { return c.getContext("2d").getImageData(80, 5, 1, 1).data[0] === 0xC4 }, 2000)
        }

        function test_problem_page_sits_inside_the_ring() {
            thm.name = "airmail"
            const c = coreComponent.createObject(root)
            const s = shellComponent.createObject(root, {
                theme: thm, core: c, accounts: c.accounts, folders: c.folders,
                messages: c.messages, outbox: c.outbox, suggestions: c.suggestions,
                searchResults: c.searchResults, problem: "The store is locked."
            })
            let page = null
            const area = s.frameArea
            for (let i = 0; i < area.children.length; i++)
                if (area.children[i].objectName === "problemPage") page = area.children[i]
            verify(page !== null)
            verify(page.visible)
            compare(page.width, 400 - 2 * thm.frameInset)
            compare(page.x, 0)
            s.destroy()
            c.destroy()
        }
    }

    Component {
        id: shellComponent
        AppShell { width: 400; height: 700; uriSource: "" }
    }
}
