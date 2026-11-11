import QtQuick 2.15
import Lomiri.Content 1.3

// Asks the user for files through Content Hub (Ubuntu Touch only; with
// ContentExport the only file that imports Lomiri.Content). The picker lists
// the apps that can hand over files; the files arrive in the app's cache
// and `imported` carries their paths. `done` fires when the user cancelled
// or the transfer is over.
Rectangle {
    id: root
    property var theme
    signal imported(var paths)
    signal done()

    property var transfer: null
    color: theme ? theme.ground : "transparent"

    function start() { transfer = null }

    ContentPeerPicker {
        anchors.fill: parent
        visible: root.transfer === null
        showTitle: true
        contentType: ContentType.All
        handler: ContentHandler.Source
        onPeerSelected: {
            peer.selectionType = ContentTransfer.Multiple
            root.transfer = peer.request()
        }
        onCancelPressed: root.done()
    }
    ContentTransferHint {
        anchors.fill: parent
        activeTransfer: root.transfer
    }
    Connections {
        target: root.transfer
        ignoreUnknownSignals: true
        function onStateChanged() {
            if (root.transfer.state === ContentTransfer.Charged) {
                const paths = []
                for (let i = 0; i < root.transfer.items.length; i++)
                    paths.push(decodeURIComponent(root.transfer.items[i].url.toString().replace(/^file:\/\//, "")))
                root.imported(paths)
                root.done()
            } else if (root.transfer.state === ContentTransfer.Aborted) {
                root.done()
            }
        }
    }
}
