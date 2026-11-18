import QtQuick 2.15
import Lomiri.Content 1.3

// Hands one file to another app through Content Hub (Ubuntu Touch only; the
// only file that imports Lomiri.Content for export). The file is already in
// the app's cache. "share" lists the apps that take shares; "open",
// "save" and "calendar" list destinations (Document Viewer, Files, Calendar). `done` fires when the
// user cancelled or the hand-over is charged.
Rectangle {
    id: root
    property var theme
    property string path: ""
    property string mode: "share"
    signal done()

    color: theme ? theme.ground : "transparent"

    function start(file, how) {
        path = file
        mode = how
    }

    Component {
        id: itemComponent
        ContentItem {}
    }

    ContentPeerPicker {
        id: picker
        anchors.fill: parent
        showTitle: true
        // "calendar" lists the apps that take events (Calendar).
        contentType: root.mode === "calendar" ? ContentType.Events : ContentType.All
        handler: root.mode === "share" ? ContentHandler.Share : ContentHandler.Destination
        onPeerSelected: {
            picker.peer.selectionType = ContentTransfer.Single
            const transfer = picker.peer.request()
            if (transfer.state === ContentTransfer.InProgress) {
                transfer.items = [itemComponent.createObject(root, { url: "file://" + root.path.split("/").map(encodeURIComponent).join("/") })]
                transfer.state = ContentTransfer.Charged
            }
            root.done()
        }
        onCancelPressed: root.done()
    }
}
