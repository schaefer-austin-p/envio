import QtQuick 2.15
import QtQuick.Controls 2.15 as QC
import "../../platform"
import "../../components"

// The right-click menu of a message row (two-pane board, frame 7): an
// anchored popup on the dialog surface. It only reports the choice; the
// caller acts on `messageId`, never on the checked rows.
QC.Popup {
    id: root
    objectName: "rowMenu"
    required property Theme theme
    property int messageId: 0
    property bool unread: false
    property string account: ""
    signal selectRequested()
    signal markRequested(bool read)
    signal moveRequested()
    signal archiveRequested()
    signal deleteRequested()

    // Counted in theme.popupsUp while shown (also when destroyed shown).
    property bool counted: false
    onVisibleChanged: syncCount()
    Component.onDestruction: { visible = false; syncCount() }
    function syncCount() {
        if (visible === counted) return
        counted = visible
        theme.popupsUp += visible ? 1 : -1
    }

    // Opens with its top-left corner at (px, py) of the window, moved in
    // where it would run off the window.
    function openAt(px, py) {
        const w = parent ? parent.width : 0
        const h = parent ? parent.height : 0
        x = Math.max(0, Math.min(px, w - width))
        y = Math.max(0, Math.min(py, h - height))
        open()
    }

    parent: QC.Overlay.overlay
    // Modal so an outside click only closes it, and without a scrim.
    modal: true
    closePolicy: QC.Popup.CloseOnEscape | QC.Popup.CloseOnPressOutside
    QC.Overlay.modal: Item {}
    width: theme.gu(30)
    topPadding: 6
    bottomPadding: 6
    leftPadding: 0
    rightPadding: 0

    background: Item {
        // A soft drop shadow (the board's 0 8px 24px): a stack of growing,
        // faint rounded rectangles, offset down.
        Repeater {
            model: 6
            Rectangle {
                required property int index
                x: -index * 2
                y: 6 - index * 2
                width: parent.width + index * 4
                height: parent.height + index * 4
                radius: root.theme.fieldRadius
                color: root.theme.dark ? Qt.rgba(0, 0, 0, 0.08) : Qt.rgba(30 / 255, 42 / 255, 56 / 255, 0.035)
            }
        }
        Rectangle {
            anchors.fill: parent
            color: root.theme.surface
            radius: root.theme.fieldRadius
            border.width: 1
            border.color: root.theme.field
        }
    }

    component Entry: Item {
        id: entry
        property string name: ""
        property string icon: ""
        property string label: ""
        property string hint: ""
        property bool danger: false
        signal activated()
        objectName: "menu-" + name
        width: parent ? parent.width : 0
        height: 40
        Rectangle {
            anchors { fill: parent; leftMargin: 4; rightMargin: 4 }
            radius: root.theme.chipRadius
            color: area.containsMouse ? root.theme.hover : "transparent"
        }
        Icon {
            id: glyph
            x: 14
            anchors.verticalCenter: parent.verticalCenter
            theme: root.theme
            name: entry.icon
            size: 18
            color: entry.danger ? root.theme.accent : root.theme.text
        }
        Text {
            objectName: "label"
            textFormat: Text.PlainText
            anchors { left: glyph.right; leftMargin: 12; right: key.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
            text: entry.label
            elide: Text.ElideRight
            color: entry.danger ? root.theme.accent : root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 14
        }
        Text {
            id: key
            textFormat: Text.PlainText
            anchors { right: parent.right; rightMargin: 14; verticalCenter: parent.verticalCenter }
            text: entry.hint
            color: root.theme.secondary
            font.family: root.theme.fontLabel
            font.pixelSize: 11
        }
        MouseArea {
            id: area
            anchors.fill: parent
            hoverEnabled: true
            onClicked: { root.close(); entry.activated() }
        }
    }

    contentItem: Column {
        id: col
        width: root.availableWidth
        Entry {
            name: "select"; icon: "square-check"; label: qsTr("Select")
            onActivated: root.selectRequested()
        }
        Entry {
            name: "mark"; icon: root.unread ? "mail-open" : "mail"
            label: root.unread ? qsTr("Mark as read") : qsTr("Mark as unread")
            onActivated: root.markRequested(root.unread)
        }
        Entry {
            name: "move"; icon: "folder-input"; label: qsTr("Move to folder")
            onActivated: root.moveRequested()
        }
        Entry {
            name: "archive"; icon: "archive"; label: qsTr("Archive")
            onActivated: root.archiveRequested()
        }
        Item {
            width: parent.width
            height: 9
            Rectangle { y: 4; width: parent.width; height: 1; color: root.theme.hairline }
        }
        Entry {
            name: "delete"; icon: "trash-2"; label: qsTr("Delete"); hint: qsTr("Del"); danger: true
            onActivated: root.deleteRequested()
        }
    }
}
