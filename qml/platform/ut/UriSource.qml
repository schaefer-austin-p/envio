import QtQuick 2.15
import Lomiri.Components 1.3

// The links the system hands the app while it runs (URL dispatcher; the
// manifest's `urls` hook lists mailto:). A link that started the app comes
// on the command line, which platform/Uri.qml reads. Ubuntu Touch only.
Item {
    signal opened(string uri)

    Connections {
        target: UriHandler
        function onOpened(uris) {
            for (let i = 0; i < uris.length; i++) opened(String(uris[i]))
        }
    }
}
