import QtQuick 2.15
import Qt.labs.platform 1.1 as Labs

// Attachments and the Content Hub. Pages use this and never import Lomiri
// themselves: the hub's pickers are `ut/ContentExport.qml` and
// `ut/ContentImport.qml`, loaded only when something is exported or
// imported. Where they do not exist (desktop, tests) nothing happens but a
// log line. Fill the page, so the picker can cover it.
Item {
    id: root
    required property Theme theme

    // Where attachments are written before they are handed over: the app's
    // own cache (the same folder as the Rust side's cache directory).
    readonly property string attachmentsDir: {
        const base = decodeURIComponent(Labs.StandardPaths.writableLocation(Labs.StandardPaths.GenericCacheLocation).toString())
        return base.replace(/^file:\/\//, "") + "/" + Qt.application.name + "/attachments"
    }

    // Whether the hub's picker exists here: its file needs Lomiri.Content,
    // which only the phone has. Compiled, not shown.
    readonly property bool hubAvailable: {
        const probe = Qt.createComponent(Qt.resolvedUrl("ut/ContentExport.qml"))
        const ok = probe.status === Component.Ready
        probe.destroy()
        return ok
    }

    // The paths of the files the user picked to attach.
    signal filesImported(var paths)

    // Asks the user for files; `filesImported` follows.
    function importFiles() {
        picker.setSource("ut/ContentImport.qml", { theme: root.theme })
        if (picker.status !== Loader.Ready) {
            console.info("Content Hub is not available here; nothing was imported")
            picker.source = ""
            return
        }
        picker.item.start()
    }

    // `mode` is "open", "save", "share" or "calendar". (Not `export`: that is a
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
    Loader {
        id: picker
        anchors.fill: parent
        z: 20
    }
    Connections {
        target: picker.item
        ignoreUnknownSignals: true
        function onImported(paths) { root.filesImported(paths) }
        function onDone() { picker.source = "" }
    }
    Connections {
        target: hub.item
        ignoreUnknownSignals: true
        function onDone() { hub.source = "" }
    }
}
