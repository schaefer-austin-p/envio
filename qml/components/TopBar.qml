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
    // The compose bar's smaller title, and the glyph of the leading button.
    property bool compact: false
    property string backIcon: "back"
    signal backClicked()
    default property alias actions: actionRow.data

    objectName: "topBar"
    implicitHeight: theme.barHeight
    color: selecting ? theme.selected : theme.ground

    IconButton {
        id: back
        objectName: root.selecting ? "leaveSelection" : "backButton"
        visible: root.showBack
        theme: root.theme
        iconName: root.selecting ? "close" : root.backIcon
        text: root.selecting ? qsTr("Leave selection") : qsTr("Back")
        anchors.left: parent.left
        anchors.leftMargin: root.theme.gu(0.5)
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: root.theme.barPadTop / 2
        onClicked: root.backClicked()
    }
    Text {
        textFormat: Text.PlainText
        anchors.left: root.showBack ? back.right : parent.left
        anchors.leftMargin: root.showBack ? root.theme.gu(0.5) : root.theme.gu(2.5)
        objectName: "title"
        anchors.right: actionRow.left
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: root.theme.barPadTop / 2
        text: root.title
        elide: Text.ElideRight
        color: root.theme.heading
        font: root.compact ? root.theme.barTitleCompactFont : root.theme.barTitleFont
    }
    Row {
        id: actionRow
        anchors.right: parent.right
        anchors.rightMargin: root.theme.gu(0.5)
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: root.theme.barPadTop / 2
    }
}
