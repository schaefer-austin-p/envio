import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/pages"

// The list pane's splitter (#192): the width it shows and saves, its
// clamping to 36..70 gu and to what the window leaves, and its behaviour
// across the 90 gu and 180 gu breakpoints.
Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    Component { id: coreComponent; MockCore {} }
    Component {
        id: shellComponent
        AppShell {
            width: 400
            height: 700
            uriSource: ""
        }
    }

    TestCase {
        name: "Splitter"
        when: windowShown

        readonly property var ann: ({ key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org", status: "", state: "ok", unread: 0, unreadText: "" })

        function find(item, name) {
            if (item.objectName === name) return item
            const kids = item.children || []
            for (let i = 0; i < kids.length; i++) {
                const hit = find(kids[i], name)
                if (hit !== null) return hit
            }
            if (item.contentItem && item.contentItem !== item) return find(item.contentItem, name)
            return null
        }
        // A booted shell of the given width (gu); the saved list width is
        // `saved` gu (0: the settings have not arrived).
        function bootWide(gu, saved) {
            const c = coreComponent.createObject(root)
            c.accounts.append(ann)
            c.listPaneGu = saved
            const s = shellComponent.createObject(root, {
                theme: thm, core: c, accounts: c.accounts, folders: c.folders,
                messages: c.messages, outbox: c.outbox, suggestions: c.suggestions,
                searchResults: c.searchResults
            })
            c.ready = true
            s.width = thm.gu(gu)
            s.height = 700
            tryVerify(function() { return s.depth === 1 })
            return { core: c, shell: s }
        }
        function lineX(b) { return find(b.shell, "paneHairline").x }
        // Drags the handle by dx px with the mouse (scene coordinates).
        function drag(b, dx) {
            const h = find(b.shell, "splitter")
            const p = h.mapToItem(root, h.width / 2, 100)
            mousePress(root, p.x, p.y)
            mouseMove(root, p.x + dx / 2, p.y)
            mouseMove(root, p.x + dx, p.y)
            mouseRelease(root, p.x + dx, p.y)
        }

        function test_the_saved_width_sets_the_list_width() {
            const b = bootWide(130, 60)
            compare(lineX(b), thm.gu(60))
            b.shell.destroy(); b.core.destroy()
        }

        function test_without_settings_the_list_is_50_gu() {
            const b = bootWide(130, 0)
            compare(lineX(b), thm.gu(50))
            b.shell.destroy(); b.core.destroy()
        }

        function test_a_drag_moves_the_list_and_saves_the_width_once() {
            const b = bootWide(130, 50)
            drag(b, thm.gu(10))
            compare(lineX(b), thm.gu(60))
            compare(b.core.called("setListPane").length, 1)
            compare(b.core.called("setListPane")[0].args[0], 60)
            b.shell.destroy(); b.core.destroy()
        }

        function test_a_drag_stops_at_70_gu() {
            const b = bootWide(160, 50)
            drag(b, thm.gu(100))
            compare(lineX(b), thm.gu(70))
            compare(b.core.called("setListPane")[0].args[0], 70)
            b.shell.destroy(); b.core.destroy()
        }

        function test_a_drag_stops_at_36_gu() {
            const b = bootWide(160, 50)
            drag(b, -thm.gu(100))
            compare(lineX(b), thm.gu(36))
            compare(b.core.called("setListPane")[0].args[0], 36)
            b.shell.destroy(); b.core.destroy()
        }

        function test_the_reader_keeps_40_gu() {
            const b = bootWide(100, 50)
            // 100 gu leaves at most 60 gu for the list.
            drag(b, thm.gu(30))
            compare(lineX(b), thm.gu(60))
            const pane = find(b.shell, "pane")
            verify(pane.width >= thm.gu(40) - 1, "pane is " + pane.width)
            b.shell.destroy(); b.core.destroy()
        }

        function test_no_room_at_90_gu_hides_the_handle() {
            const b = bootWide(90, 50)
            compare(b.shell.wide, true)
            compare(find(b.shell, "splitter").visible, false)
            compare(lineX(b), thm.gu(50))
            b.shell.destroy(); b.core.destroy()
        }

        function test_a_wide_window_shows_the_handle() {
            const b = bootWide(130, 50)
            compare(find(b.shell, "splitter").visible, true)
            b.shell.destroy(); b.core.destroy()
        }

        function test_narrow_has_no_splitter_and_never_rewrites_the_width() {
            const b = bootWide(130, 70)
            b.shell.width = thm.gu(80)
            tryCompare(b.shell, "wide", false)
            compare(find(b.shell, "splitter").visible, false)
            b.shell.width = thm.gu(130)
            tryCompare(b.shell, "wide", true)
            compare(lineX(b), thm.gu(70))
            compare(b.core.called("setListPane").length, 0)
            b.shell.destroy(); b.core.destroy()
        }

        function test_a_small_window_clamps_the_shown_width_and_growing_restores_it() {
            const b = bootWide(130, 70)
            b.shell.width = thm.gu(100)
            tryCompare(b.shell, "wide", true)
            compare(lineX(b), thm.gu(60))
            b.shell.width = thm.gu(130)
            compare(lineX(b), thm.gu(70))
            compare(b.core.called("setListPane").length, 0)
            b.shell.destroy(); b.core.destroy()
        }

        function test_very_wide_puts_the_drawer_left_of_the_list() {
            const b = bootWide(200, 60)
            compare(b.shell.veryWide, true)
            compare(lineX(b), thm.gu(30) + thm.gu(60))
            b.shell.destroy(); b.core.destroy()
        }

        function test_the_drawer_takes_room_from_the_maximum() {
            // 190 gu with the drawer: 190 - 30 - 40 = 120, so 70 is allowed;
            // 180 gu is the same. The cap is 70 either way.
            const b = bootWide(180, 70)
            compare(b.shell.veryWide, true)
            compare(b.shell.listGu, 70)
            // The reader still has 80 gu.
            verify(find(b.shell, "pane").width >= thm.gu(40))
            b.shell.destroy(); b.core.destroy()
        }

        function test_arrow_keys_step_one_gu_and_save_after_a_rest() {
            const b = bootWide(130, 50)
            const h = find(b.shell, "splitter")
            h.forceActiveFocus()
            keyClick(Qt.Key_Right)
            keyClick(Qt.Key_Right)
            compare(lineX(b), thm.gu(52))
            tryVerify(function() { return b.core.called("setListPane").length === 1 })
            compare(b.core.called("setListPane")[0].args[0], 52)
            b.shell.destroy(); b.core.destroy()
        }
    }
}
