import QtQuick 2.15
import QtTest 1.2
import Envio 1.0

// The real EnvioCore (Rust core and worker) over a temporary XDG_* tree,
// which tests/qml/run.sh sets. Run by the envio-qmltest runner, which links
// the Envio module.
TestCase {
    id: testCase
    name: "CoreObject"

    EnvioCore { id: core }
    RowsModel { id: accounts; kind: "accounts" }
    Repeater { id: accountRows; model: accounts }

    function initTestCase() {
        core.attachModel(accounts)
        core.start()
    }

    function cleanupTestCase() {
        core.stop()
    }

    function test_start_reports_ready() {
        tryVerify(function () { return core.ready }, 20000)
        compare(accountRows.count, 0)
    }

    function test_settings_arrive_after_start() {
        tryCompare(core, "theme", "carbon", 20000)
        compare(core.darkMode, "system")
        compare(core.imagesAlways, false)
        compare(core.defaultAccount, "")
    }

    function test_count_text_is_plural_aware() {
        verify(core.countText("selected", 1) !== core.countText("selected", 2))
        compare(core.countText("results", 1), "1 result")
        compare(core.countText("results", 2), "2 results")
        compare(core.countText("nonsense", 2), "")
    }
}
