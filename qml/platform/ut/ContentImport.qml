import QtQuick 2.15
import Lomiri.Content 1.3
import "../fileUrl.js" as FileUrl

// Asks the user for files through Content Hub (Ubuntu Touch only; with
// ContentExport the only file that imports Lomiri.Content). The picker lists
// the apps that can hand over files; the files arrive in the app's cache
// and `imported` carries their paths. `done` fires when the user cancelled
// or the transfer is over.
Rectangle {
    id: root
    property var theme
    // `transfer` is finalized by the receiver once the copies are made.
    signal imported(var paths, var transfer)
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
                const urls = []
                for (let i = 0; i < root.transfer.items.length; i++)
                    urls.push(root.transfer.items[i].url.toString())
                const paths = FileUrl.pathsOfUrls(urls)
                // The receiver finalizes the transfer once the files are
                // copied (platform/ShareIn.qml); with nothing to copy, now.
                if (paths.length > 0) root.imported(paths, root.transfer)
                else root.transfer.finalize()
                root.done()
            } else if (root.transfer.state === ContentTransfer.Aborted) {
                root.transfer.finalize()
                root.done()
            }
        }
    }
}
