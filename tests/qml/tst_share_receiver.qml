import QtQuick 2.15
import QtTest 1.2
import Lomiri.Content 1.3

// platform/ut/ShareReceiver.qml with a stand-in for the hub (creating it on
// the real ContentHub would register a handler on the bus). Both signals the
// hub uses for a share must reach `received`: "share" (a share sheet) and
// "import" (the File Manager's "Open with another app").
TestCase {
    name: "ShareReceiver"
    when: windowShown

    Component {
        id: hubComponent
        QtObject {
            signal shareRequested(var transfer)
            signal importRequested(var transfer)
        }
    }
    SignalSpy { id: spy; signalName: "received" }

    function make() {
        const hub = hubComponent.createObject(this)
        const c = Qt.createComponent(Qt.resolvedUrl("../../qml/platform/ut/ShareReceiver.qml"))
        verify(c.status === Component.Ready, c.errorString())
        const r = c.createObject(this, { hub: hub })
        spy.target = r
        spy.clear()
        return { hub: hub, receiver: r }
    }

    function test_a_charged_share_is_received() {
        const m = make()
        const t = { state: ContentTransfer.Charged }
        m.hub.shareRequested(t)
        compare(spy.count, 1)
        compare(spy.signalArguments[0][0], t)
        m.receiver.destroy()
    }

    function test_a_charged_open_with_is_received() {
        const m = make()
        const t = { state: ContentTransfer.Charged }
        m.hub.importRequested(t)
        compare(spy.count, 1)
        compare(spy.signalArguments[0][0], t)
        m.receiver.destroy()
    }
}
