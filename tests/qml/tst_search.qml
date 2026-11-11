import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/pages/search"

Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    MockCore { id: core }
    SearchPage {
        id: page
        theme: thm
        core: core
        accounts: core.accounts
        results: core.searchResults
        folderId: 7
        anchors.fill: parent
    }
    SignalSpy { id: opened; target: page; signalName: "messageRequested" }

    TestCase {
        name: "Search"
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
        function all(name) {
            return everything(page, []).filter(function(o) { return o.objectName === name })
        }
        function find(name) {
            const hit = all(name)
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }
        function shown(name) {
            return all(name).filter(function(o) { return o.visible }).length > 0
        }
        function message(id) {
            return { id: id, account: "ann", accountName: "Ann", from: "Lucia", subject: "Plans " + id,
                     preview: "I found a train…", date: 0, dateBucket: "time", unread: false,
                     flagged: false, hasAttachments: false, selected: false }
        }
        function account(key, name) {
            return { key: key, name: name, email: key + "@example.org", status: "", state: "ok", unread: 0,
                     pinService: "", pinFingerprint: "", pinHex: "", pinText: "" }
        }
        function init() {
            find("searchField").input.text = ""
            core.accounts.clear()
            core.accounts.append(account("ann", "Ann"))
            core.accounts.append(account("bob", "Bob"))
            core.searchResults.clear()
            page.scope = "all"
            page.serverBusy = false
            page.failures = []
            opened.clear()
            core.reset()
        }
        function typeQuery(text) {
            find("searchField").input.text = text
        }
        function searches() { return core.called("search") }

        function test_search_runs_local_first() {
            typeQuery("train")
            tryVerify(function() { return searches().length === 1 })
            compare(searches()[0].args, ["all", "train", false])
            core.searchResults.append(message(1))
            core.searchResults.append(message(2))
            // The count is the core's text for the number of rows.
            tryCompare(find("resultCount"), "text", "results 2")
            // A scope chip asks again, within that scope.
            mouseClick(find("chip-account-bob"))
            compare(searches()[1].args, ["account:bob", "train", false])
            mouseClick(find("chip-folder"))
            compare(searches()[2].args, ["folder:7", "train", false])
            mouseClick(find("chip-all"))
            compare(searches()[3].args, ["all", "train", false])
            // Nothing asks for the servers on its own.
            compare(searches().filter(function(c) { return c.args[2] }).length, 0)
        }

        function test_a_result_opens_the_message() {
            typeQuery("train")
            core.searchResults.append(message(5))
            tryVerify(function() { return shown("row-5") })
            mouseClick(find("row-5"))
            compare(opened.count, 1)
            compare(opened.signalArguments[0][0], 5)
        }

        function test_server_search_button() {
            verify(!shown("searchServers"), "no button without a question")
            typeQuery("train")
            tryVerify(function() { return shown("searchServers") })
            mouseClick(find("searchServers"))
            compare(searches()[searches().length - 1].args, ["all", "train", true])
        }

        function test_search_failed_lists_accounts() {
            typeQuery("train")
            tryVerify(function() { return shown("searchServers") })
            mouseClick(find("searchServers"))
            core.searchFailed("Ann: sign-in failed")
            core.searchFailed("Bob: offline")
            tryVerify(function() { return all("failedLine").length === 2 })
            compare(all("failedLine")[0].text, "Ann: sign-in failed")
            compare(all("failedLine")[1].text, "Bob: offline")
            // A new question forgets them.
            mouseClick(find("chip-all"))
            tryVerify(function() { return all("failedLine").length === 0 })
        }

        function test_server_search_shows_busy_until_results() {
            typeQuery("train")
            tryVerify(function() { return shown("searchServers") })
            verify(!shown("searchBusy"))
            mouseClick(find("searchServers"))
            verify(shown("searchBusy"))
            compare(find("searchingText").text, "Searching the servers…")
            verify(!shown("searchServers"))
            // The results arrive (or every account failed): the core says done.
            core.searchDone()
            verify(!shown("searchBusy"))
            verify(shown("searchServers"))
        }

        function test_cancel_drops_late_results() {
            typeQuery("train")
            tryVerify(function() { return shown("searchServers") })
            mouseClick(find("searchServers"))
            verify(shown("searchBusy"))
            mouseClick(find("cancelServerSearch"))
            verify(!shown("searchBusy"))
            // The same question again, locally: Rust drops the servers' late
            // answer to the earlier one (envio-ui test
            // a_cancelled_server_search_drops_late_results).
            compare(searches()[searches().length - 1].args, ["all", "train", false])
        }
    }
}
