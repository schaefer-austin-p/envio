import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/pages/accounts"

Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    MockCore { id: core }
    UpdateSignInPage {
        id: page
        theme: thm
        core: core
        accounts: core.accounts
        account: "ann"
        anchors.fill: parent
    }
    SignalSpy { id: closed; target: page; signalName: "closed" }
    SignalSpy { id: again; target: page; signalName: "signInAgainRequested" }

    TestCase {
        name: "UpdateSignIn"
        when: windowShown

        property var opened: []

        function everything(item, out) {
            out.push(item)
            for (const prop of ["data", "children"]) {
                if (item[prop] === undefined) continue
                for (let i = 0; i < item[prop].length; i++) everything(item[prop][i], out)
                break
            }
            return out
        }
        function find(name) {
            const hit = everything(page, []).filter(function(o) { return o.objectName === name })
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }
        function row(extra) {
            const base = { key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org",
                           status: "", state: "ok", oauth: true, updateSignIn: true,
                           moveStage: "", moveTitle: "", moveText: "", moveHint: "",
                           moveNoteTitle: "", moveNoteText: "", canSwitch: false }
            return Object.assign(base, extra || {})
        }
        function init() {
            core.reset()
            core.accounts.clear()
            core.accounts.append(row())
            page.session.reset()
            opened = []
            page.session.uri.opener = function(u) { opened.push(String(u)); return true }
            closed.clear()
            tryCompare(page, "accountEmail", "ann@example.org")
        }
        function move(extra) { core.accounts.set(0, row(extra)) }

        function test_explains_before_the_move() {
            compare(find("accountName").text, "Ann")
            verify(find("explanation").visible)
            verify(find("updateSignInButton").visible)
            verify(!find("moveCard").visible)
            verify(!find("switchAnyway").visible)
        }
        function test_the_button_starts_a_graph_sign_in_for_the_account() {
            mouseClick(find("updateSignInButton"))
            compare(core.called("startOAuth").length, 1)
            compare(core.called("startOAuth")[0].args[0], "ann@example.org")
            compare(core.called("startOAuth")[0].args[1], "ann")
            compare(core.called("startOAuth")[0].args[2], "graph")
            verify(find("browserStep").visible)
        }
        function test_a_work_account_is_told_and_nothing_moves() {
            mouseClick(find("updateSignInButton"))
            core.oauthDone("ann@example.org", "work", "This is a work account.")
            verify(find("updateSignInNotice").visible)
            compare(find("updateSignInNotice").text, "This is a work account.")
            verify(find("updateSignInButton").visible)
            compare(closed.count, 0)
        }
        function test_the_move_shows_the_cores_words() {
            move({ moveStage: "moving", moveTitle: "Moving", moveText: "Finishing 2 changes.", moveHint: "You can leave." })
            tryVerify(function() { return find("moveCard").visible })
            compare(find("moveCard").title, "Moving")
            compare(find("moveCard").text, "Finishing 2 changes.")
            compare(find("moveHint").text, "You can leave.")
            verify(!find("updateSignInButton").visible)
            verify(!find("switchAnyway").visible)
        }
        function test_stuck_offers_switch_anyway_only_when_the_core_allows() {
            move({ moveStage: "stuck", moveTitle: "Stuck", moveText: "x", canSwitch: false })
            tryVerify(function() { return find("moveCard").visible })
            verify(!find("switchAnyway").visible)
            move({ moveStage: "stuck", moveTitle: "Stuck", moveText: "x", canSwitch: true })
            tryVerify(function() { return find("switchAnyway").visible })
            mouseClick(find("switchAnyway"))
            compare(core.called("switchAnyway").length, 1)
            compare(core.called("switchAnyway")[0].args[0], "ann")
        }
        function test_a_refused_sign_in_offers_sign_in_again() {
            move({ moveStage: "signin", moveTitle: "Sign in again", moveText: "x", canSwitch: true })
            tryVerify(function() { return find("signInAgain").visible })
            verify(!find("updateSignInButton").visible)
            again.clear()
            mouseClick(find("signInAgain"))
            compare(again.count, 1)
            compare(again.signalArguments[0][0], "ann")
            move({ moveStage: "moving", moveTitle: "Moving", moveText: "x" })
            tryVerify(function() { return !find("signInAgain").visible })
        }
        function test_done_shows_the_dropped_notice_and_closes() {
            move({ moveStage: "done", moveTitle: "Done", moveText: "ok",
                   moveNoteTitle: "1 change was not made", moveNoteText: "It was waiting." })
            tryVerify(function() { return find("moveDone").visible })
            verify(find("droppedNotice").visible)
            compare(find("droppedNotice").title, "1 change was not made")
            mouseClick(find("moveDone"))
            compare(closed.count, 1)
        }
        function test_done_without_a_dropped_action_has_no_notice() {
            move({ moveStage: "done", moveTitle: "Done", moveText: "ok" })
            tryVerify(function() { return find("moveDone").visible })
            verify(!find("droppedNotice").visible)
        }
    }
}
