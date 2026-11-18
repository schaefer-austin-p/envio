import QtQuick 2.15
import Lomiri.Content 1.3

// Takes the files other apps share into Envio through Content Hub (Ubuntu
// Touch only; with ContentExport and ContentImport the only files that import
// Lomiri.Content). The hub emits `shareRequested` or `importRequested` both while the app runs and
// right after it was started for a share. `received` carries the transfer
// once it is charged; platform/ShareIn.qml turns it into paths. The items
// stay in the hub's inbox, so the core copies them.
Item {
    id: root
    signal received(var transfer)

    property var waiting: []

    Connections {
        target: ContentHub
        // "Share" (a gallery, another app's share sheet) and "Open with
        // another app" (the File Manager: a destination hook) arrive on
        // different signals; both hand over files to keep. Our own picker
        // (ContentImport, a Source request) is answered on the peer's
        // exportRequested, never on ours, so it cannot land here.
        function onShareRequested(transfer) { root.take(transfer) }
        function onImportRequested(transfer) { root.take(transfer) }
    }
    function take(transfer) {
        if (transfer.state === ContentTransfer.Charged) root.received(transfer)
        else root.waiting = root.waiting.concat([transfer])
    }

    // A transfer that was not charged yet: wait for it.
    Instantiator {
        model: root.waiting
        delegate: QtObject {
            id: waiter
            required property var modelData
            readonly property var conn: Connections {
                target: waiter.modelData
                ignoreUnknownSignals: true
                function onStateChanged() {
                    const state = waiter.modelData.state
                    if (state !== ContentTransfer.Charged && state !== ContentTransfer.Aborted) return
                    const transfer = waiter.modelData
                    root.waiting = root.waiting.filter(function(t) { return t !== transfer })
                    if (state === ContentTransfer.Charged) root.received(transfer)
                }
            }
        }
    }
}
