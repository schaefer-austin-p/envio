import QtQuick 2.15
import "../platform"
import "../components"

// A full-page error: the core could not start, or the mail cache will not
// open. Unsent drafts are safe in their own files meanwhile; "Try again"
// asks the app to start (or look) again.
Rectangle {
    id: root
    objectName: "problemPage"
    required property Theme theme
    property string text: ""
    // "Try again" was tapped and nothing has answered yet.
    property bool busy: false
    signal retry()

    color: theme.ground

    Column {
        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; margins: root.theme.gu(3) }
        spacing: root.theme.gu(2)
        Icon {
            theme: root.theme
            name: "alert"
            size: 32
            color: root.theme.accent
            anchors.horizontalCenter: parent.horizontalCenter
        }
        Text {
            textFormat: Text.PlainText
            objectName: "problemText"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            text: root.text
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 16
            lineHeight: 1.3
        }
        AppButton {
            objectName: "tryAgain"
            width: parent.width
            theme: root.theme
            primary: true
            enabled: !root.busy
            text: root.busy ? qsTr("Trying again…") : qsTr("Try again")
            onClicked: root.retry()
        }
    }
    // Swallows taps meant for the page underneath.
    MouseArea { anchors.fill: parent; z: -1 }
}
