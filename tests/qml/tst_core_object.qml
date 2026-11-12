import QtQuick 2.15
import QtTest 1.2
import Envio 1.0

// The real EnvioCore (Rust core and worker) over a temporary XDG_* tree,
// which tests/qml/run.sh sets. Run by the envio-qmltest runner, which links
// the Envio module.
TestCase {
    id: testCase
    name: "CoreObject"

    // What happened after start(), in order: "reset" is the accounts model
    // being reset, "ready" is `ready` turning true.
    property var seen: []

    EnvioCore {
        id: core
        onReadyChanged: if (ready) testCase.seen.push("ready")
    }
    RowsModel {
        id: accounts
        kind: "accounts"
        onModelReset: testCase.seen.push("reset")
    }
    Repeater { id: accountRows; model: accounts }

    RowsModel { id: suggestions; kind: "suggestions" }
    Repeater {
        id: suggestionRows
        model: suggestions
        delegate: Item {
            property string who: model.name
            property string address: model.address
            property bool known: model.fromContacts
            // Raw, to check the role reaches QML as a JS number.
            property var lastSeen: model.lastSeen
        }
    }

    // Attached only after the rows exist.
    RowsModel { id: late; kind: "suggestions" }
    Repeater {
        id: lateRows
        model: late
        delegate: Item { property string address: model.address }
    }

    function initTestCase() {
        core.attachModel(accounts)
        core.attachModel(suggestions)
        // Attaching resets a model from what Ui holds (nothing yet).
        tryVerify(function () { return testCase.seen.length >= 1 })
        testCase.seen = []
        core.start()
    }

    function cleanupTestCase() {
        core.stop()
    }

    function test_start_reports_ready() {
        tryVerify(function () { return core.ready }, 20000)
        compare(accountRows.count, 0)
    }

    // `ready` must not overtake the accounts' rows: a page that picks the
    // wizard or the inbox from them would choose wrongly.
    function test_ready_follows_the_accounts_rows() {
        tryVerify(function () { return core.ready }, 20000)
        compare(testCase.seen, ["reset", "ready"])
    }

    function test_settings_arrive_after_start() {
        tryCompare(core, "theme", "carbon", 20000)
        compare(core.darkMode, "system")
        compare(core.imagesAlways, false)
        compare(core.defaultAccount, "")
    }

    // A reset through the real core, with roleNames and data.
    function test_suggestions_fill_a_model() {
        tryVerify(function () { return core.ready }, 20000)
        core.addContact("Nia Test", "nia@example.org")
        core.autocomplete("nia")
        tryCompare(suggestionRows, "count", 1, 20000)
        var row = suggestionRows.itemAt(0)
        compare(row.who, "Nia Test")
        compare(row.address, "nia@example.org")
        compare(row.known, true)
    }

    // A model made after the rows arrived starts in step.
    function test_a_late_model_catches_up() {
        tryVerify(function () { return core.ready }, 20000)
        core.addContact("Nia Test", "nia@example.org")
        core.autocomplete("nia")
        tryCompare(suggestionRows, "count", 1, 20000)
        compare(lateRows.count, 0)
        core.attachModel(late)
        tryCompare(lateRows, "count", 1, 20000)
        compare(lateRows.itemAt(0).address, "nia@example.org")
    }

    // The allowed senders follow allow and disallow, through the saved settings.
    function test_allowed_senders_follow_allow_and_disallow() {
        tryVerify(function () { return core.ready }, 20000)
        core.allowImagesFrom("Ada@Example.org")
        tryCompare(core, "allowedSenders", ["ada@example.org"], 20000)
        core.disallowImagesFrom("ada@example.org")
        tryCompare(core, "allowedSenders", [], 20000)
    }

    // What Main.qml binds the Theme to: the settings the core sends back after
    // a save (Main itself cannot be loaded without an ApplicationWindow).
    function test_saved_settings_come_back_for_the_theme_binding() {
        tryVerify(function () { return core.ready }, 20000)
        core.saveSettings("airmail", "dark", false, "")
        tryCompare(core, "theme", "airmail", 20000)
        compare(core.darkMode, "dark")
        core.saveSettings("carbon", "system", false, "")
        tryCompare(core, "theme", "carbon", 20000)
        compare(core.darkMode, "system")
    }

    function test_count_text_is_plural_aware() {
        verify(core.countText("selected", 1) !== core.countText("selected", 2))
        compare(core.countText("results", 1), "1 result")
        compare(core.countText("results", 2), "2 results")
        compare(core.countText("keptdays", 90), "Finds mail older than the 90 days kept here")
        compare(core.countText("nonsense", 2), "")
    }

    function test_link_view_names_the_real_host() {
        const v = core.linkView("https://bank.example@evil.example/login")
        compare(v.host, "evil.example")
        compare(v.userinfo, "bank.example")
        compare(v.afterHost, "/login")
        compare(v.openable, true)
        verify(v.warning.indexOf("evil.example") >= 0)
        compare(core.linkView("https://exa mple.org/").openable, false)
    }

    function test_link_view_gives_an_idn_host_in_ascii() {
        const v = core.linkView("https://xn--bcher-kva.example/")
        compare(v.host, "xn--bcher-kva.example")
        compare(v.unicodeHost, "b\u00fccher.example")
        verify(v.unicodeNote.length > 0)
    }

    // 64-bit values must reach QML as numbers. A QVariant, signal argument or
    // return value of `std::int64_t` is a `long` on LP64 Linux, which Qt 5's
    // QML engine leaves as an opaque object: dates read 0, `===` on ids fails.
    function test_number_roles_reach_qml_as_numbers() {
        tryVerify(function () { return core.ready }, 20000)
        core.addContact("Nia Test", "nia@example.org")
        core.autocomplete("nia")
        tryCompare(suggestionRows, "count", 1, 20000)
        compare(typeof suggestionRows.itemAt(0).lastSeen, "number")
    }

    function test_compose_id_is_a_number() {
        compare(typeof core.startCompose("nonsense", 0, "", ""), "number")
    }
}
