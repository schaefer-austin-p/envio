import QtQuick 2.15
import QtQuick.Controls 2.15
import "../../platform"
import "../../components"

// One address line (To, Cc or Bcc): a chip for each address and a field for
// the next one, with the core's suggestions under it. The text typed so far
// becomes a chip on a comma or semicolon, Enter, or when the field loses
// focus; `entries` holds what the message goes to, and `edited` fires when
// the user changes it (not when the page fills it).
Item {
    id: root
    required property Theme theme
    required property var core
    required property var suggestions
    property string label: ""
    property var entries: []
    readonly property alias input: field.input
    signal edited()

    // Whether the suggestions belong to this field: the last one typed in.
    property bool asking: false

    implicitHeight: column.height

    function setEntries(list) { entries = list.slice() }
    function add(entry) {
        const e = entry.trim()
        if (e === "") return
        entries = entries.concat([e])
        edited()
    }
    function remove(i) {
        const list = entries.slice()
        list.splice(i, 1)
        entries = list
        edited()
    }
    // What is typed becomes chips: all of it, or (`all` false) only what a
    // separator ended.
    function commit(all) {
        const parts = field.text.split(/[,;]/)
        const rest = all === false ? parts.pop() : ""
        asking = false
        asker.stop()
        field.text = rest
        for (const p of parts) add(p)
    }

    Timer {
        id: asker
        interval: 150
        onTriggered: root.core.autocomplete(field.text.trim())
    }

    Column {
        id: column
        width: parent.width
        Row {
            width: parent.width
            Text {
                textFormat: Text.PlainText
                width: root.theme.gu(6.5)
                height: root.theme.touch
                verticalAlignment: Text.AlignVCenter
                leftPadding: root.theme.gu(2)
                text: root.label
                color: root.theme.secondary
                font.family: root.theme.fontUi
                font.pixelSize: 14
            }
            Flow {
                width: parent.width - root.theme.gu(6.5) - root.theme.gu(1)
                Repeater {
                    model: root.entries
                    delegate: AbstractButton {
                        id: chip
                        required property string modelData
                        required property int index
                        objectName: root.objectName + "-remove-" + index
                        implicitHeight: root.theme.touch
                        implicitWidth: Math.min(root.width - root.theme.gu(9), chipText.implicitWidth + root.theme.gu(6))
                        Accessible.role: Accessible.Button
                        Accessible.name: qsTr("Remove %1").arg(modelData)
                        contentItem: Item {}
                        background: Rectangle {
                            anchors { fill: parent; topMargin: root.theme.gu(0.5); bottomMargin: root.theme.gu(0.5); rightMargin: root.theme.gu(0.5) }
                            radius: height / 2
                            color: chip.down ? root.theme.selected : root.theme.tintB
                            Text {
                                id: chipText
                                textFormat: Text.PlainText
                                anchors { left: parent.left; leftMargin: root.theme.gu(1.5); right: closeIcon.left; rightMargin: root.theme.gu(0.5); verticalCenter: parent.verticalCenter }
                                text: chip.modelData
                                elide: Text.ElideRight
                                color: root.theme.text
                                font.family: root.theme.fontUi
                                font.pixelSize: 14
                            }
                            Icon {
                                id: closeIcon
                                theme: root.theme
                                name: "close"
                                size: 16
                                color: root.theme.secondary
                                anchors { right: parent.right; rightMargin: root.theme.gu(1); verticalCenter: parent.verticalCenter }
                            }
                        }
                        onClicked: root.remove(index)
                    }
                }
                Field {
                    id: field
                    theme: root.theme
                    framed: false
                    width: Math.max(root.theme.gu(14), parent.width - root.theme.gu(1))
                    inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
                    onAccepted: root.commit(true)
                }
                Connections {
                    target: field.input
                    function onTextChanged() {
                        if (/[,;]/.test(field.text)) root.commit(false)
                        else if (field.text.trim() === "") { root.asking = false; asker.stop() }
                        else { root.asking = true; asker.restart() }
                    }
                    function onActiveFocusChanged() {
                        if (!field.input.activeFocus) root.commit(true)
                    }
                }
            }
        }
        Suggestions {
            id: list
            width: parent.width
            theme: root.theme
            core: root.core
            model: root.suggestions
            visible: root.asking && field.text.trim() !== "" && list.count > 0
            height: visible ? implicitHeight : 0
            onChosen: function(entry) {
                root.asking = false
                asker.stop()
                field.text = ""
                root.add(entry)
            }
        }
        Rectangle { width: parent.width; height: 1; color: root.theme.hairline }
    }
}
