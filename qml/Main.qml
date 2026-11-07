import QtQuick 2.15
import QtQuick.Controls 2.15
import Envio 1.0

ApplicationWindow {
    id: window
    width: 360
    height: 640
    visible: true
    title: "Envio"

    Label {
        anchors.centerIn: parent
        text: qsTr("Envio %1").arg(BuildInfo.version)
    }

    Component.onCompleted: console.info("envio " + BuildInfo.version + " (" + BuildInfo.commit + ")")
}
