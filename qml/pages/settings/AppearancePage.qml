import QtQuick 2.15
import QtQuick.Controls 2.15
import "../../platform"
import "../../components"

// Appearance (board 13a): the look (Carbon or Airmail) as two cards with a
// preview strip, and Light, Dark or System. A choice is saved at once; the
// core answers with the settings and the theme follows.
Item {
    id: root
    objectName: "appearancePage"
    required property Theme theme
    required property var core
    // Phone only (WindowMode.available): the row is hidden elsewhere.
    property bool fullscreenAvailable: false
    signal backRequested()

    function save(look, dark) {
        core.saveSettings(look, dark, core.imagesAlways, core.defaultAccount)
    }

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        title: qsTr("Appearance")
        onBackClicked: root.backRequested()
    }

    Column {
        anchors { top: bar.bottom; left: parent.left; right: parent.right; margins: root.theme.gu(2) }
        anchors.topMargin: root.theme.gu(2)
        spacing: root.theme.gu(1.5)

        Row {
            width: parent.width
            spacing: root.theme.gu(1.5)
            Repeater {
                model: [
                    { key: "carbon", label: qsTr("Carbon") },
                    { key: "airmail", label: qsTr("Airmail") }
                ]
                delegate: AbstractButton {
                    id: card
                    required property var modelData
                    readonly property var preview: root.theme.roles[modelData.key][root.theme.dark ? "dark" : "light"]
                    objectName: "themeCard-" + modelData.key
                    width: (parent.width - root.theme.gu(1.5)) / 2
                    implicitHeight: root.theme.gu(14)
                    property bool selected: root.core.theme === modelData.key
                    text: modelData.label
                    Accessible.role: Accessible.RadioButton
                    Accessible.checked: selected
                    onClicked: root.save(modelData.key, root.core.darkMode)
                    background: Rectangle {
                        radius: root.theme.optionRadius
                        color: card.preview.ground
                        border.width: card.selected ? 3 : 1
                        border.color: card.selected ? root.theme.primary : root.theme.hairline
                    }
                    contentItem: Item {
                        Rectangle {
                            x: root.theme.gu(1.5)
                            y: root.theme.gu(1.5)
                            width: parent.width - root.theme.gu(3)
                            height: root.theme.gu(1.5)
                            color: card.preview.primary
                        }
                        Rectangle {
                            x: root.theme.gu(1.5)
                            y: root.theme.gu(4.5)
                            width: (parent.width - root.theme.gu(3)) * 0.7
                            height: root.theme.gu(1)
                            color: card.preview.surface
                            border.width: 1
                            border.color: card.preview.hairline
                        }
                        Rectangle {
                            x: root.theme.gu(1.5)
                            y: root.theme.gu(6.5)
                            width: root.theme.gu(3)
                            height: root.theme.gu(1)
                            color: card.preview.accent
                        }
                        Text {
                            textFormat: Text.PlainText
                            anchors { left: parent.left; bottom: parent.bottom; margins: root.theme.gu(1.5) }
                            text: card.text
                            color: card.preview.text
                            font.family: root.theme.fontUi
                            font.pixelSize: 15
                            font.weight: Font.Medium
                        }
                    }
                }
            }
        }

        Text {
            textFormat: Text.PlainText
            text: qsTr("Colour scheme")
            color: root.theme.secondary
            font.family: root.theme.eyebrowFamily
            font.pixelSize: root.theme.eyebrowSize
            font.weight: root.theme.eyebrowWeight
            font.letterSpacing: root.theme.eyebrowSpacing
            font.capitalization: root.theme.eyebrowCaps ? Font.AllUppercase : Font.MixedCase
        }
        Repeater {
            model: [
                { key: "light", label: qsTr("Light") },
                { key: "dark", label: qsTr("Dark") },
                { key: "system", label: qsTr("System") }
            ]
            delegate: RadioRow {
                required property var modelData
                objectName: "darkMode-" + modelData.key
                theme: root.theme
                width: parent.width
                text: modelData.label
                selected: root.core.darkMode === modelData.key
                onActivated: root.save(root.core.theme, modelData.key)
            }
        }

        ListRow {
            id: fullscreenRow
            objectName: "fullscreenRow"
            visible: root.fullscreenAvailable
            theme: root.theme
            width: parent.width
            separator: false
            implicitHeight: Math.max(root.theme.touch + root.theme.gu(2.5), fullscreenCol.height + root.theme.gu(2.5))
            onClicked: root.core.chooseFullscreen(!root.core.fullscreen)
            Column {
                id: fullscreenCol
                anchors.verticalCenter: parent.verticalCenter
                x: root.theme.gu(2)
                width: parent.width - x - fullscreenToggle.width - root.theme.gu(4)
                Text {
                    textFormat: Text.PlainText
                    width: parent.width
                    text: qsTr("Fullscreen")
                    wrapMode: Text.WordWrap
                    color: root.theme.text
                    font.family: root.theme.fontUi
                    font.pixelSize: 16
                }
                Text {
                    textFormat: Text.PlainText
                    width: parent.width
                    text: qsTr("Hides the top bar. Swipe down from the top edge to bring it back.")
                    wrapMode: Text.WordWrap
                    color: root.theme.secondary
                    font.family: root.theme.fontUi
                    font.pixelSize: 13
                }
            }
            Toggle {
                id: fullscreenToggle
                objectName: "fullscreenSwitch"
                theme: root.theme
                anchors { right: parent.right; rightMargin: root.theme.gu(2); verticalCenter: parent.verticalCenter }
                text: qsTr("Fullscreen")
                checked: root.core.fullscreen
                onToggled: root.core.chooseFullscreen(checked)
                Connections {
                    target: root.core
                    function onFullscreenChanged() { fullscreenToggle.checked = Qt.binding(function() { return root.core.fullscreen }) }
                }
            }
        }
    }
}
