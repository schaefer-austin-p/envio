import QtQuick 2.15
import "../../platform"
import "../../components"

// Settings (board 13): the groups, each leading to its page.
Item {
    id: root
    objectName: "settingsPage"
    required property Theme theme
    required property var core
    signal backRequested()
    signal appearanceRequested()
    signal syncRequested()
    signal imagesRequested()
    signal accountsRequested()
    signal aboutRequested()

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        title: qsTr("Settings")
        onBackClicked: root.backRequested()
    }

    Column {
        anchors { top: bar.bottom; left: parent.left; right: parent.right }
        Repeater {
            model: [
                { key: "appearance", label: qsTr("Appearance") },
                { key: "sync", label: qsTr("Sync") },
                { key: "images", label: qsTr("Load remote images") },
                { key: "accounts", label: qsTr("Accounts") },
                { key: "about", label: qsTr("About") }
            ]
            delegate: ListRow {
                id: row
                required property var modelData
                objectName: "row-" + modelData.key
                theme: root.theme
                width: parent.width
                chevron: true
                onClicked: {
                    switch (modelData.key) {
                    case "appearance": root.appearanceRequested(); break
                    case "sync": root.syncRequested(); break
                    case "images": root.imagesRequested(); break
                    case "accounts": root.accountsRequested(); break
                    default: root.aboutRequested()
                    }
                }
                Text {
                    textFormat: Text.PlainText
                    x: root.theme.gu(2.5)
                    y: root.theme.gu(1.75)
                    width: parent.width - x - row.chevronSpace
                    text: row.modelData.label
                    elide: Text.ElideRight
                    color: root.theme.text
                    font.family: root.theme.fontUi
                    font.pixelSize: 16
                }
            }
        }
    }
}
