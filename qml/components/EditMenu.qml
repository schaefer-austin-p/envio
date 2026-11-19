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
    // The field's HoldSelection (bindle): Select all through it opens the
    // menu again with Cut and Copy live. Without one, Select all only selects.
    property var hold: null

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

    // The height the menu has once open. implicitHeight reads only the
    // padding while it is closed (the rows are invisible, so they have no
    // height), so a caller that places the menu before it opens uses this.
    readonly property real openHeight:
        ((password ? 0 : copyOnly ? 2 : 3) + (copyOnly ? 0 : 1)) * theme.touch
        + topPadding + bottomPadding

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
        property string iconName: ""
        readonly property color tint: row.enabled ? root.theme.text : root.theme.secondary
        contentItem: Row {
            spacing: root.theme.gu(1.5)
            Icon {
                visible: row.iconName !== ""
                theme: root.theme
                name: row.iconName
                size: root.theme.iconButton
                color: row.tint
                anchors.verticalCenter: parent.verticalCenter
            }
            Text {
                textFormat: Text.PlainText
                text: row.text
                height: parent.height
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
                color: row.tint
                font.family: root.theme.fontUi
                font.pixelSize: 16
            }
        }
    }

    EditRow {
        objectName: "editCut"
        iconName: "scissors"
        text: qsTr("Cut")
        visible: !root.password && !root.copyOnly
        enabled: root.hasSelection
        onTriggered: root.field.cut()
    }
    EditRow {
        objectName: "editCopy"
        iconName: "copy"
        text: qsTr("Copy")
        visible: !root.password
        enabled: root.hasSelection
        onTriggered: root.copyOnly ? root.copyPlain() : root.field.copy()
    }
    EditRow {
        objectName: "editPaste"
        iconName: "clipboard-paste"
        text: qsTr("Paste")
        visible: !root.copyOnly
        enabled: root.pasteable
        onTriggered: root.field.paste()
    }
    EditRow {
        objectName: "editSelectAll"
        iconName: "text-select"
        text: qsTr("Select all")
        visible: !root.password
        enabled: root.hasText
        onTriggered: root.hold ? root.hold.selectAll() : root.field.selectAll()
    }
}
