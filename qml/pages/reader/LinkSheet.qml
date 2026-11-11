import QtQuick 2.15
import "../../platform"
import "../../components"

// A tapped link, before anything opens it: the full real address (the href,
// never the link's text, which can say anything) and Open, Copy or Cancel.
// The only way a link leaves the reader.
Sheet {
    id: root
    objectName: "linkSheet"
    property string address: ""
    // What is shown: the same address with invisible and direction-changing
    // characters (RLO and the like, which can reorder what is displayed)
    // written out as [U+XXXX]. Display only; Open and Copy use `address`.
    readonly property string shownAddress: address.replace(
        /[\u00AD\u061C\u180E\u200B-\u200F\u2028-\u202E\u2060-\u2069\uFEFF]/g,
        function(c) { return "[U+" + c.charCodeAt(0).toString(16).toUpperCase().padStart(4, "0") + "]" })
    signal openRequested(string url)

    title: qsTr("Open this link?")

    function show(url) {
        address = url
        open()
    }

    // The clipboard has no QML API of its own; a hidden TextEdit copies.
    TextEdit { id: clip; visible: false; textFormat: TextEdit.PlainText }

    Column {
        x: root.theme.gu(2.5)
        width: parent.width - root.theme.gu(5)
        spacing: root.theme.gu(1.5)
        Text {
            objectName: "linkAddress"
            width: parent.width
            text: root.shownAddress
            textFormat: Text.PlainText
            wrapMode: Text.WrapAnywhere
            color: root.theme.text
            font.family: root.theme.fontMono
            font.pixelSize: 13
        }
        AppButton {
            objectName: "linkOpen"
            width: parent.width
            theme: root.theme
            primary: true
            text: qsTr("Open")
            onClicked: { root.close(); root.openRequested(root.address) }
        }
        AppButton {
            objectName: "linkCopy"
            width: parent.width
            theme: root.theme
            text: qsTr("Copy")
            onClicked: {
                clip.text = root.address
                clip.selectAll()
                clip.copy()
                root.close()
            }
        }
        AppButton {
            objectName: "linkCancel"
            width: parent.width
            theme: root.theme
            text: qsTr("Cancel")
            onClicked: root.close()
        }
    }
}
