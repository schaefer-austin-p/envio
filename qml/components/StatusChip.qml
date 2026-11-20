import QtQuick 2.15
import "../platform"

// A 24 px status chip (the Drafts list's "Not uploaded", #266 boards): an
// icon and a small upper-case label on tint A. The ink is the theme's text
// role, so it reads on the dark theme's tint too. Not Chip.qml, which is the
// 44 px filter button. `iconName` is an Icon.qml name.
Rectangle {
    id: root
    objectName: "statusChip"
    required property Theme theme
    property string text: ""
    property string iconName: "uploadoff"
    // The widest the chip may be; a longer label elides on its one line.
    property real maxWidth: Infinity

    implicitWidth: Math.min(row.implicitWidth + theme.gu(2), maxWidth)
    implicitHeight: 24
    width: implicitWidth
    height: implicitHeight
    radius: theme.chipRadius
    color: theme.tintA

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 6
        Icon {
            objectName: "statusChipIcon"
            visible: root.iconName !== ""
            theme: root.theme
            name: root.iconName
            size: 14
            color: root.theme.text
            anchors.verticalCenter: parent.verticalCenter
        }
        Text {
            objectName: "statusChipText"
            textFormat: Text.PlainText
            elide: Text.ElideRight
            maximumLineCount: 1
            width: Math.min(implicitWidth, Math.max(0, root.maxWidth - root.theme.gu(2) - row.spacing
                                                       - (root.iconName !== "" ? 14 : 0)))
            text: root.text
            font.capitalization: Font.AllUppercase
            color: root.theme.text
            font.family: root.theme.fontLabel
            font.pixelSize: 11
            font.letterSpacing: 0.5
            anchors.verticalCenter: parent.verticalCenter
        }
    }
}
