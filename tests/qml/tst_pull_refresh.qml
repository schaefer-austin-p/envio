import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/pages"
import "../../qml/pages/inbox"

// #125: the pull-to-refresh indicator and the list's hold while a refresh
// runs. The Behaviours.PullToRefresh mechanics have their own tests in bindle.
Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    MockCore { id: core }
    MessageList {
        id: ml
        theme: thm
        core: core
        messages: core.messages
        anchors.fill: parent
    }

    TestCase {
        name: "PullRefresh"
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
        function find(name) {
            const hit = everything(ml, []).filter(function(o) { return o.objectName === name })
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }
        function message(id) {
            return { id: id, account: "ann", accountName: "Ann", from: "Lucia", fromAddress: "lucia@example.org", subject: "Plans " + id,
                     preview: "I found a train…", date: 0, dateBucket: "time", unread: true,
                     flagged: false, hasAttachments: false, selected: false, open: false }
        }
        function init() {
            core.reset()
            core.messages.clear()
            core.canRefresh = true
            core.lastTicket = 0
            core.focusMessage = 0
            core.refreshScope = ""
            ml.minBusyMs = 600
            ml.capMs = 15000
            if (ml.refreshing) ml.finish()
            for (let i = 1; i <= 30; i++) core.messages.append(message(i))
            const list = find("messageList")
            list.positionViewAtBeginning()
            waitForRendering(ml)
            tryVerify(function() { return list.topMargin === 0 && !ml.refreshing })
        }

        // A finger drags down until the list is pulled `px` past its rest
        // position (some travel is lost to the drag threshold), then lets go
        // (or stays, with `hold`).
        function pull(px, hold) {
            const list = find("messageList")
            const rest = list.originY - list.topMargin
            mousePress(list, 100, 100)
            let y = 100
            for (let i = 0; i < 80 && rest - list.contentY < px; i++) {
                y += 5
                mouseMove(list, 100, y)
            }
            if (hold !== true) mouseRelease(list, 100, y)
            return y
        }

        function test_a_short_pull_does_not_refresh() {
            pull(thm.gu(10) / 2)
            wait(100)
            compare(core.called("refresh").length, 0)
            tryVerify(function() { return find("messageList").contentY >= find("messageList").originY - 1 })
        }

        function test_the_indicator_says_whether_letting_go_refreshes() {
            const ind = find("pullIndicator")
            verify(!ind.visible, "hidden at rest")
            const y = pull(thm.gu(10) / 2, true)
            compare(find("pullLabel").text, "Pull to refresh")
            compare(ind.Accessible.name, "Pull to refresh")
            const list = find("messageList")
            mouseMove(list, 100, y + thm.gu(10))
            tryCompare(find("pullLabel"), "text", "Release to refresh")
            mouseRelease(list, 100, y + thm.gu(10))
        }

        function test_the_line_fits_the_band_of_a_short_pull() {
            const list = find("messageList")
            const ind = find("pullIndicator")
            const y = pull(48, true)
            tryVerify(function() { return ind.visible && ind.height >= 40 })
            const label = find("pullLabel")
            const glyph = find("pullGlyph")
            const line = find("pullLine")
            const top = line.mapToItem(ind, 0, 0).y
            verify(top >= 0 && top + line.height <= ind.height,
                "the disc and label sit inside the band: " + top + "+" + line.height + " in " + ind.height)
            verify(label.height <= ind.height && glyph.height <= ind.height)
            const g = glyph.mapToItem(ind, 0, 0)
            const l = label.mapToItem(ind, 0, 0)
            verify(l.x >= g.x + glyph.width, "the label is right of the disc")
            compare(Math.round(g.y + glyph.height / 2), Math.round(l.y + label.height / 2), "one line, centred")
            mouseRelease(list, 100, y)
        }

        function test_a_long_pull_refreshes_once_and_holds_the_list() {
            pull(thm.gu(10) + 60)
            compare(core.called("refresh").length, 1)
            compare(core.called("refresh")[0].args[0], "")
            verify(ml.refreshing)
            const list = find("messageList")
            tryCompare(list, "topMargin", thm.gu(7))
            compare(find("pullLabel").text, "Refreshing")
            tryVerify(function() { return Math.abs((list.originY - list.contentY) - thm.gu(7)) < 2 })
        }

        function test_the_ack_ends_the_hold_but_not_before_the_minimum() {
            ml.minBusyMs = 400
            pull(thm.gu(10) + 60)
            verify(ml.refreshing)
            core.refreshed(core.lastTicket)
            verify(ml.refreshing, "acked at once: the line stays readable")
            tryVerify(function() { return !ml.refreshing })
            tryCompare(find("messageList"), "topMargin", 0)
            tryVerify(function() { return !find("pullIndicator").visible })
        }

        function test_a_slow_refresh_ends_at_the_cap() {
            ml.capMs = 300
            pull(thm.gu(10) + 60)
            verify(ml.refreshing)
            wait(100)
            verify(ml.refreshing)
            tryVerify(function() { return !ml.refreshing })
            compare(core.called("refresh").length, 1)
        }

        function test_a_second_release_while_busy_does_not_refresh_again() {
            pull(thm.gu(10) + 60)
            compare(core.called("refresh").length, 1)
            tryCompare(find("messageList"), "topMargin", thm.gu(7))
            pull(thm.gu(10) + 90)
            compare(core.called("refresh").length, 1)
        }

        function test_nothing_to_refresh_shows_no_indicator_and_ignores_the_pull() {
            core.canRefresh = false
            const y = pull(thm.gu(10) + 60, true)
            verify(!find("pullIndicator").visible)
            mouseRelease(find("messageList"), 100, y)
            wait(100)
            compare(core.called("refresh").length, 0)
            verify(!ml.refreshing)
            ml.refresh()
            compare(core.called("refresh").length, 0, "the key does nothing either")
        }

        function test_the_key_refreshes_with_the_tab_scope_and_opens_the_room() {
            core.refreshScope = "ann"
            ml.refresh()
            compare(core.called("refresh").length, 1)
            compare(core.called("refresh")[0].args[0], "ann")
            const list = find("messageList")
            tryVerify(function() { return find("pullIndicator").visible && find("pullIndicator").height > thm.gu(6) })
            tryVerify(function() { return Math.abs((list.originY - list.contentY) - thm.gu(7)) < 2 })
            ml.refresh()
            compare(core.called("refresh").length, 1, "one at a time")
        }

        // The scope is the core's (the shown folder's account, from Rust): a
        // pull passes it as is.
        function test_a_pull_passes_the_shown_lists_account() {
            core.refreshScope = "ann"
            pull(thm.gu(10) + 60)
            compare(core.called("refresh").length, 1)
            compare(core.called("refresh")[0].args[0], "ann")
        }

        function test_an_ack_of_another_refresh_does_not_end_the_hold() {
            ml.minBusyMs = 100
            pull(thm.gu(10) + 60)
            verify(ml.refreshing)
            const mine = core.lastTicket
            core.refreshed(0)
            core.refreshed(mine + 1)
            wait(300)
            verify(ml.refreshing, "a resume's or the problem page's ack is not ours")
            core.refreshed(mine)
            tryVerify(function() { return !ml.refreshing })
        }

        function test_a_late_ack_after_the_cap_is_ignored() {
            ml.capMs = 200
            ml.minBusyMs = 50
            ml.refresh()
            const first = core.lastTicket
            tryVerify(function() { return !ml.refreshing })
            ml.refresh()
            verify(ml.refreshing)
            verify(core.lastTicket !== first)
            core.refreshed(first)
            wait(150)
            verify(ml.refreshing, "the first refresh's ack is not the second's")
            core.refreshed(core.lastTicket)
            tryVerify(function() { return !ml.refreshing })
        }

        // A screen reader reaches the action on the list, which is always
        // there (the indicator is hidden at rest).
        function test_the_accessible_action_of_the_list_refreshes() {
            verify(!find("pullIndicator").visible)
            ml.accessible.pressAction()
            compare(core.called("refresh").length, 1)
            verify(ml.refreshing)
        }

        function test_the_indicator_is_an_accessible_status_line() {
            const ind = find("pullIndicator")
            compare(ind.Accessible.role, Accessible.Indicator)
            compare(ind.parent, ml, "outside the list's content")
            compare(ind.y, 0)
        }

        // The hold leaves room above row 0: focusing it must not scroll the
        // row up under the indicator.
        function test_focus_on_the_first_row_while_holding_does_not_scroll_it_under_the_indicator() {
            ml.refresh()
            const list = find("messageList")
            tryVerify(function() { return Math.abs((list.originY - list.contentY) - thm.gu(7)) < 2 })
            const before = list.contentY
            core.focusMessage = 1
            wait(150)
            compare(list.contentY, before)
        }
    }
}
