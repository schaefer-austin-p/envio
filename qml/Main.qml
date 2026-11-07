import QtQuick 2.15
import QtQuick.Controls 2.15
import Envio 1.0
import "platform"
import "pages"

ApplicationWindow {
    id: window
    width: 360
    height: 640
    visible: true
    title: "Envio"
    color: appTheme.ground

    Theme { id: appTheme }

    InboxPage {
        anchors.fill: parent
        theme: appTheme
    }

    Component.onCompleted: console.info("envio " + BuildInfo.version + " (" + BuildInfo.commit + ")")
}
