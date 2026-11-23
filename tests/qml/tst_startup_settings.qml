import QtQuick 2.15
import QtTest 1.2
import QtQuick.Window 2.15
import Envio 1.0
import "../../qml/platform"

// The saved look, dark mode and Fullscreen are in the first frame (#463).
// Each case writes settings.json (`fixtures`, tests/qml/main.cpp) and then
// makes its core, so the file is there at construction; none calls `start()`
// except the window case, which then checks that the worker's event changes
// nothing. `cleanup` removes the file, so the other tests see none.
Item {
    id: root
    width: 400
    height: 700

    // What a user who chose Airmail, dark and Fullscreen has saved.
    readonly property string saved: JSON.stringify({
        theme: "airmail", dark: "dark", fullscreen: true,
        sync_window_days: 90, list_pane_gu: 40, notify_show_text: true
    })
    // The look the defaults give, to compare colours with.
    Theme { id: airmailDark; name: "airmail"; darkMode: "dark" }
    Theme { id: carbonSystem; name: "carbon"; darkMode: "system" }

    TestCase {
        name: "StartupSettings"
        when: windowShown

        property var made: []

        function cleanup() {
            for (const o of made) o.destroy()
            made = []
            fixtures.removeSettings()
        }

        function makeCore(text) {
            if (text === null) fixtures.removeSettings()
            else fixtures.writeSettings(text)
            const core = Qt.createQmlObject('import Envio 1.0; EnvioCore {}', root)
            made.push(core)
            return core
        }

        function test_saved_values_are_there_before_start() {
            const core = makeCore(root.saved)
            // No start(): no worker, so no event can have set these.
            compare(core.ready, false)
            compare(core.theme, "airmail")
            compare(core.darkMode, "dark")
            compare(core.fullscreen, true)
            compare(core.syncDays, 90)
            compare(core.listPaneGu, 40)
            compare(core.notifyShowText, true)
        }

        function test_no_file_is_the_defaults() {
            const core = makeCore(null)
            compare(core.theme, "carbon")
            compare(core.darkMode, "system")
            compare(core.fullscreen, false)
        }

        function test_a_corrupt_file_is_the_defaults_and_stays_put() {
            const core = makeCore("{ not json")
            compare(core.theme, "carbon")
            compare(core.darkMode, "system")
            compare(core.fullscreen, false)
            // Reading it set nothing aside: that is the worker's job.
            verify(fixtures.exists("settings.json"))
            verify(!fixtures.exists("settings.json.damaged"))
        }

        // The window as Main wires it: what it has at its first show is
        // already the saved look and visibility, and the worker's event
        // after start() changes nothing.
        function test_the_first_frame_has_the_saved_values() {
            fixtures.writeSettings(root.saved)
            const comp = Qt.createComponent("FirstFrameWindow.qml")
            compare(comp.status, Component.Ready, comp.errorString())
            const win = comp.createObject(root)
            made.push(win)
            verify(win.snapped)
            compare(win.firstTheme, "airmail")
            compare(win.firstVisibility, Window.FullScreen)
            compare(win.firstColor, airmailDark.ground)
            verify(win.firstColor !== carbonSystem.ground)
            const echoes = Qt.createQmlObject(
                'import QtTest 1.2; SignalSpy { signalName: "settingsEchoed" }', root)
            echoes.target = win.core
            win.core.start()
            tryVerify(function () { return echoes.count >= 1 }, 20000)
            tryVerify(function () { return win.core.ready }, 20000)
            compare(win.changes, 0)
            compare(win.core.theme, "airmail")
            win.core.stop()
            echoes.destroy()
        }
    }
}
