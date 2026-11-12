import QtQuick 2.15
import "../../platform"
import "../../components"

// A tapped link, before anything opens it: the real address (the href, never
// the link's text, which can say anything) in parts, and Open, Copy or Cancel.
// The only way a link leaves the reader.
Sheet {
    id: root
    objectName: "linkSheet"
    // Where `linkView` is asked.
    property var core: null
    property string address: ""
    // The address in parts, from the core (`linkView`): the real host apart,
    // the decoration before an @ flagged, invisible characters written out.
    // Display only; Open and Copy use `address` as it came.
    property var parts: ({ "beforeHost": "", "userinfo": "", "host": "", "punycode": "",
                           "afterHost": "", "warning": "", "openable": false })
    signal openRequested(string url)

    title: qsTr("Open this link?")

    function show(url) {
        address = url
        parts = core.linkView(url)
        open()
    }

    // The clipboard has no QML API of its own; a hidden TextEdit copies.
    TextEdit { id: clip; visible: false; textFormat: TextEdit.PlainText }

    Column {
        x: root.theme.gu(2.5)
        width: parent.width - root.theme.gu(5)
        spacing: root.theme.gu(1.5)
        Flow {
            objectName: "linkAddress"
            width: parent.width
            Text {
                objectName: "linkBefore"
                wrapMode: Text.WrapAnywhere
                width: Math.min(implicitWidth, parent.width)
                text: root.parts.beforeHost
                textFormat: Text.PlainText
                color: root.theme.secondary
                font.family: root.theme.fontLabel
                font.pixelSize: 13
            }
            Text {
                objectName: "linkUserinfo"
                wrapMode: Text.WrapAnywhere
                width: Math.min(implicitWidth, parent.width)
                visible: root.parts.userinfo !== ""
                text: root.parts.userinfo + "@"
                textFormat: Text.PlainText
                // Struck through and in the caution colour: it is decoration.
                font.strikeout: true
                color: root.theme.accent
                font.family: root.theme.fontLabel
                font.pixelSize: 13
            }
            Text {
                objectName: "linkHost"
                wrapMode: Text.WrapAnywhere
                width: Math.min(implicitWidth, parent.width)
                text: root.parts.host
                textFormat: Text.PlainText
                color: root.theme.text
                font.family: root.theme.fontLabel
                font.pixelSize: 13
                font.bold: true
            }
            Text {
                objectName: "linkAfter"
                text: root.parts.afterHost
                textFormat: Text.PlainText
                wrapMode: Text.WrapAnywhere
                width: Math.min(implicitWidth, parent.width)
                color: root.theme.secondary
                font.family: root.theme.fontLabel
                font.pixelSize: 13
            }
        }
        Text {
            objectName: "linkWarning"
            visible: root.parts.warning !== ""
            width: parent.width
            text: root.parts.warning
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: root.theme.accent
            font.family: root.theme.fontUi
            font.pixelSize: 14
        }
        AppButton {
            objectName: "linkOpen"
            width: parent.width
            theme: root.theme
            primary: true
            enabled: root.parts.openable
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
