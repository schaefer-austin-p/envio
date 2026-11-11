import QtQuick 2.15
import "../platform"

// A page's top bar: optional back button, a title, and trailing items
// (IconButtons) placed in `actions`. `selecting` swaps the ground for the
// selected colour (selection mode).
Rectangle {
    id: root
    required property Theme theme
    property string title: ""
    property bool showBack: false
    property bool selecting: false
    signal backClicked()
    default property alias actions: actionRow.data

    implicitHeight: theme.touch + theme.gu(1)
    color: selecting ? theme.selected : theme.ground

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: root.theme.hairline
    }

    IconButton {
        id: back
        visible: root.showBack
        theme: root.theme
        iconName: root.selecting ? "close" : "back"
        text: root.selecting ? qsTr("Leave selection") : qsTr("Back")
        anchors.left: parent.left
        anchors.leftMargin: root.theme.gu(0.5)
        anchors.verticalCenter: parent.verticalCenter
        onClicked: root.backClicked()
    }
    Text {
        anchors.left: root.showBack ? back.right : parent.left
        anchors.leftMargin: root.showBack ? root.theme.gu(0.5) : root.theme.gu(2.5)
        anchors.right: actionRow.left
        anchors.verticalCenter: parent.verticalCenter
        text: root.title
        elide: Text.ElideRight
        color: root.theme.heading
        font.family: root.theme.fontTitle
        font.pixelSize: root.theme.airmail ? 24 : 18
        font.weight: Font.DemiBold
    }
    Row {
        id: actionRow
        anchors.right: parent.right
        anchors.rightMargin: root.theme.gu(0.5)
        anchors.verticalCenter: parent.verticalCenter
    }
}
