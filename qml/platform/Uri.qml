import QtQuick 2.15

// mailto: links from outside the app. One that started the app is on the
// command line (`Exec=envio %U`); one that arrives while it runs comes from
// `ut/UriSource.qml`, loaded only where it exists (the phone). `mailto`
// carries the whole link; the page that starts a compose decides when.
Item {
    id: root
    // What delivers the links that arrive while the app runs; empty where
    // there is none (the tests, which load no Lomiri module).
    property string sourceFile: "ut/UriSource.qml"
    // The command line; a link that started the app is one of these. A
    // property so the tests can hand in their own.
    property var arguments: Qt.application.arguments
    signal mailto(string uri)

    function offer(uri) {
        if (/^mailto:/i.test(uri)) mailto(uri)
    }

    Component.onCompleted: {
        // Later, so the handler that listens has been set up.
        Qt.callLater(function() {
            for (const arg of root.arguments) root.offer(arg)
        })
        if (root.sourceFile === "") return
        source.setSource(root.sourceFile, {})
        if (source.status !== Loader.Ready) source.source = ""
    }

    Loader { id: source }
    Connections {
        target: source.item
        ignoreUnknownSignals: true
        function onOpened(uri) { root.offer(uri) }
    }
}
