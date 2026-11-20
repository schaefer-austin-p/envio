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

    // The Lucide icon of a folder's role; a folder without one gets a folder.
    function roleIcon(role) {
        switch (role) {
        case "inbox": return "inbox"
        case "sent": return "send"
        case "drafts": return "file-pen"
        case "archive": return "archive"
        case "trash": return "trash-2"
        case "junk": return "shield-alert"
        default: return "folder"
        }
    }

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
            // Not the local Drafts entry (#303, a negative id): it has no
            // server folder to move to.
            visible: model.account === root.account && model.id > 0
            iconName: root.roleIcon(model.role)
            // Child folders keep the per-level indent, icon included.
            iconInset: theme.gu(2.5) + model.depth * theme.gu(2)
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
