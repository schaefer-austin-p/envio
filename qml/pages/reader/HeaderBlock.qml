import QtQuick 2.15
import "../../components"
import "../../platform"
import "../../platform/stampTint.js" as StampTint

// The subject and the From, To, Cc, Date and Account lines of the message
// being read (boards 2 and 3). `view` is the `messageOpened` map.
Column {
    id: root
    required property Theme theme
    required property var view
    property string accountName: ""

    readonly property var lines: {
        const when = view.date > 0 ? new Date(view.date * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat) : ""
        const rows = [
            { label: qsTr("From"), value: view.from },
            { label: qsTr("To"), value: view.to.join(", ") },
            { label: qsTr("Cc"), value: view.cc.join(", ") },
            { label: qsTr("Date"), value: when },
            { label: qsTr("Account"), value: accountName !== "" ? accountName : view.account }
        ]
        return rows.filter(function(r) { return r.value !== "" })
    }

    spacing: theme.gu(1.5)

    Text {
        textFormat: Text.PlainText
        objectName: "headerSubject"
        width: parent.width
        text: root.view.subject
        wrapMode: Text.Wrap
        color: root.theme.heading
        font.family: root.theme.fontTitle
        font.pixelSize: 22
        font.weight: Font.DemiBold
        lineHeight: 1.2
    }
    // Airmail: the sender's stamp beside the name (board: gap 14).
    Loader {
        objectName: "headerStamp"
        active: root.theme.airmail
        width: parent.width
        height: item !== null ? item.height : 0
        sourceComponent: Row {
            spacing: 14
            Avatar {
                theme: root.theme
                initials: StampTint.initials(root.view.from)
                blue: StampTint.isBlue(StampTint.addressOf(
                          root.view.fromAddress !== undefined ? root.view.fromAddress : root.view.from))
            }
            Text {
                textFormat: Text.PlainText
                objectName: "headerSender"
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - root.theme.stampWidth - parent.spacing
                text: root.view.from
                elide: Text.ElideRight
                color: root.theme.heading
                font.family: root.theme.fontLabel
                font.pixelSize: 16
                font.weight: root.theme.unreadWeight
            }
        }
    }
    Column {
        width: parent.width
        Rectangle { width: parent.width; height: 1; color: root.theme.hairline }
        Column {
            width: parent.width
            topPadding: root.theme.gu(1)
            bottomPadding: root.theme.gu(1)
            Repeater {
                model: root.lines
                delegate: Row {
                    id: line
                    required property var modelData
                    width: parent.width
                    spacing: root.theme.gu(1.5)
                    Text {
                        textFormat: Text.PlainText
                        id: label
                        width: root.theme.gu(8)
                        text: line.modelData.label
                        color: root.theme.secondary
                        font.family: root.theme.fontLabel
                        font.pixelSize: 13
                    }
                    Text {
                        textFormat: Text.PlainText
                        objectName: "header-" + line.modelData.label
                        width: parent.width - label.width - parent.spacing
                        text: line.modelData.value
                        wrapMode: Text.Wrap
                        color: root.theme.text
                        font.family: root.theme.fontLabel
                        font.pixelSize: 13
                        lineHeight: 1.4
                    }
                }
            }
        }
        Rectangle { width: parent.width; height: 1; color: root.theme.hairline }
    }
}
