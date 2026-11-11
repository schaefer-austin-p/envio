import QtQuick 2.15
import Qt.labs.platform 1.1 as Labs

// Attachments and the Content Hub. Pages use this and never import Lomiri
// themselves: the hub's picker is `ut/ContentExport.qml`, loaded only when
// something is exported. Where it does not exist (desktop, tests) exporting
// does nothing but log that. Fill the page, so the picker can cover it.
Item {
    id: root
    required property Theme theme

    // Where attachments are written before they are handed over: the app's
    // own cache (the same folder as the Rust side's cache directory).
    readonly property string attachmentsDir: {
        const base = decodeURIComponent(Labs.StandardPaths.writableLocation(Labs.StandardPaths.GenericCacheLocation).toString())
        return base.replace(/^file:\/\//, "") + "/" + Qt.application.name + "/attachments"
    }

    // `mode` is "open", "save" or "share". (Not `export`: that is a
    // reserved word in JavaScript.)
    function exportFile(path, mode) {
        hub.setSource("ut/ContentExport.qml", { theme: root.theme })
        if (hub.status !== Loader.Ready) {
            console.info("Content Hub is not available here; nothing was exported")
            hub.source = ""
            return
        }
        hub.item.start(path, mode)
    }

    Loader {
        id: hub
        anchors.fill: parent
        z: 20
    }
    Connections {
        target: hub.item
        ignoreUnknownSignals: true
        function onDone() { hub.source = "" }
    }
}
