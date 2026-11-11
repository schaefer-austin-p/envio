import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// The edit menu of a text field, opened on press and hold: Controls 2 has
// none, and the phone's keyboard has no paste key. A password field offers
// Paste only, so what is typed there cannot be copied out. Never logs text.
Menu {
    id: root
    required property Theme theme
    // The TextField (or any item with cut/copy/paste/selectAll) it edits.
    required property Item field
    property bool password: false

    objectName: "editMenu"
    implicitWidth: theme.gu(25)
    padding: theme.gu(0.75)

    background: Rectangle {
        radius: root.theme.gu(1.5)
        color: root.theme.ground
        border.width: 1
        border.color: root.theme.hairline
    }

    component EditRow: MenuItem {
        id: row
        implicitHeight: visible ? root.theme.touch : 0
        padding: 0
        leftPadding: root.theme.gu(2)
        rightPadding: root.theme.gu(2)
        background: Rectangle {
            color: row.down || row.highlighted ? root.theme.selected : "transparent"
        }
        contentItem: Text {
            text: row.text
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
            color: row.enabled ? root.theme.text : root.theme.secondary
            font.family: root.theme.fontUi
            font.pixelSize: 16
        }
    }

    EditRow {
        objectName: "editCut"
        text: qsTr("Cut")
        visible: !root.password
        enabled: root.field.selectedText !== ""
        onTriggered: root.field.cut()
    }
    EditRow {
        objectName: "editCopy"
        text: qsTr("Copy")
        visible: !root.password
        enabled: root.field.selectedText !== ""
        onTriggered: root.field.copy()
    }
    EditRow {
        objectName: "editPaste"
        text: qsTr("Paste")
        enabled: root.field.canPaste
        onTriggered: root.field.paste()
    }
    EditRow {
        objectName: "editSelectAll"
        text: qsTr("Select all")
        visible: !root.password
        enabled: root.field.text !== ""
        onTriggered: root.field.selectAll()
    }
}
