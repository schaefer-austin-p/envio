import QtQuick 2.15
import "../../platform"
import "../../components"

// The line under the message (carbon-compose, airmail-compose): a check and
// "Draft saved 09:41" on the left once the draft has been written, and
// "Plain text" on the right. `savedAtMs` is the core's save time (Unix
// milliseconds, `draftStored`); 0 until the first save, when only the
// marker shows. The time is the locale's short time.
Item {
    id: root
    objectName: "composeFooter"
    required property Theme theme
    property real savedAtMs: 0

    readonly property string savedText: savedAtMs > 0
        ? qsTr("Draft saved %1").arg(Qt.formatTime(new Date(savedAtMs), Qt.locale().timeFormat(Locale.ShortFormat)))
        : ""

    implicitHeight: marker.implicitHeight + theme.gu(1) + theme.gu(1.75)
    height: implicitHeight

    Row {
        id: saved
        anchors { left: parent.left; leftMargin: root.theme.gu(2); verticalCenter: marker.verticalCenter }
        spacing: root.theme.gu(0.75)
        visible: root.savedAtMs > 0
        Icon {
            objectName: "savedCheck"
            theme: root.theme
            name: "check"
            size: 14
            color: root.theme.secondary
            anchors.verticalCenter: parent.verticalCenter
        }
        Text {
            objectName: "savedText"
            textFormat: Text.PlainText
            text: root.savedText
            color: root.theme.secondary
            font.family: root.theme.fontLabel
            font.pixelSize: 12
        }
    }
    Text {
        id: marker
        objectName: "plainText"
        textFormat: Text.PlainText
        anchors { right: parent.right; rightMargin: root.theme.gu(2); top: parent.top; topMargin: root.theme.gu(1) }
        text: qsTr("Plain text")
        color: root.theme.secondary
        font.family: root.theme.fontLabel
        font.pixelSize: 12
    }
}
