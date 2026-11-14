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
    // Read-only text (the reader's body): Copy and Select all, no Cut or Paste.
    property bool copyOnly: false

    // What the field allows, read once as the menu opens. Live bindings to
    // the field's selection and clipboard state loop on the phone: opening
    // the menu moves focus, which changes that state while it is evaluated.
    property bool hasSelection: false
    property bool hasText: false
    property bool pasteable: false
    onAboutToShow: {
        hasSelection = field.selectedText !== ""
        hasText = field.text !== ""
        pasteable = field.canPaste
    }

    // Copy puts plain text on the clipboard (a rich TextEdit's own copy()
    // also carries HTML). The clipboard has no QML API; a hidden TextEdit copies.
    function copyPlain() {
        plainClip.text = field.selectedText
        plainClip.selectAll()
        plainClip.copy()
    }
    // A property, not a child: a child Item would become a menu entry.
    readonly property TextEdit plainClip: TextEdit { visible: false; textFormat: TextEdit.PlainText }

    objectName: "editMenu"
    implicitWidth: theme.gu(25)
    padding: theme.gu(0.75)

    background: Rectangle {
        radius: root.theme.menuRadius
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
            textFormat: Text.PlainText
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
        visible: !root.password && !root.copyOnly
        enabled: root.hasSelection
        onTriggered: root.field.cut()
    }
    EditRow {
        objectName: "editCopy"
        text: qsTr("Copy")
        visible: !root.password
        enabled: root.hasSelection
        onTriggered: root.copyOnly ? root.copyPlain() : root.field.copy()
    }
    EditRow {
        objectName: "editPaste"
        text: qsTr("Paste")
        visible: !root.copyOnly
        enabled: root.pasteable
        onTriggered: root.field.paste()
    }
    EditRow {
        objectName: "editSelectAll"
        text: qsTr("Select all")
        visible: !root.password
        enabled: root.hasText
        onTriggered: root.field.selectAll()
    }
}
