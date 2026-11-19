import QtQuick 2.15
import "../../platform"
import "../../components"

// Pick the folder to move the selected messages to. It lists the folders of
// one account (the selection's; the caller disables Move across accounts),
// by role name where a folder has a role.
Sheet {
    id: root
    objectName: "moveSheet"
    required property var core
    required property var folders
    property string account: ""
    // Message ids to move; empty moves the selection.
    property var targets: []
    signal chosen(var folder)

    title: qsTr("Move to folder")

    function openFor(key) {
        account = key
        core.listFolders(key)
        open()
    }

    Repeater {
        model: root.folders
        delegate: ListRow {
            id: row
            required property var model
            objectName: "folder-" + model.id
            theme: root.theme
            width: root.width
            visible: model.account === root.account
            height: visible ? implicitHeight : 0
            onClicked: {
                root.close()
                root.chosen(row.model.id)
                if (root.targets.length > 0)
                    root.core.actOn(root.targets, "move", row.model.id)
                else
                    root.core.act("move", row.model.id)
            }
            FolderLabel {
                x: root.theme.gu(2.5) + row.model.depth * root.theme.gu(2)
                y: root.theme.gu(1.75)
                width: parent.width - x - root.theme.gu(2.5)
                theme: root.theme
                role: row.model.role
                folderName: row.model.name
            }
        }
    }
}
