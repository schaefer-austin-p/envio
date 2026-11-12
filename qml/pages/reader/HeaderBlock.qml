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
            { key: "from", label: qsTr("From"), value: view.from },
            { key: "to", label: qsTr("To"), value: view.to.join(", ") },
            { key: "cc", label: qsTr("Cc"), value: view.cc.join(", ") },
            { key: "date", label: qsTr("Date"), value: when },
            { key: "account", label: qsTr("Account"), value: accountName !== "" ? accountName : view.account }
        ]
        // Airmail's stamp row carries the sender, so the table drops From.
        return rows.filter(function(r) { return r.value !== "" && !(theme.airmail && r.key === "from") })
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
    // Airmail: the sender's stamp, name and address under the title (board:
    // gap 14). Invisible in Carbon, so the Column adds no spacing there.
    Item {
        visible: root.theme.airmail
        width: parent.width
        height: root.theme.stampHeight
        Loader {
            objectName: "headerStamp"
            anchors.fill: parent
            active: root.theme.airmail
            sourceComponent: Item {
                Avatar {
                    theme: root.theme
                    initials: StampTint.initials(root.view.from)
                    blue: StampTint.isBlue(StampTint.addressOf(
                              root.view.fromAddress ? root.view.fromAddress : root.view.from))
                }
                Column {
                    x: root.theme.stampWidth + 14
                    width: parent.width - x
                    anchors.verticalCenter: parent.verticalCenter
                    Text {
                        textFormat: Text.PlainText
                        objectName: "headerSender"
                        width: parent.width
                        text: root.view.from
                        elide: Text.ElideRight
                        color: root.theme.heading
                        // A bare address (no display name) is mono, so look-alikes stay distinct.
                        font.family: /^[^<]*@[^<]*$/.test(String(root.view.from))
                            ? root.theme.fontAddress : root.theme.fontLabel
                        font.pixelSize: 16
                        font.weight: root.theme.unreadWeight
                    }
                    Text {
                        textFormat: Text.PlainText
                        objectName: "headerAddress"
                        visible: text !== ""
                        width: parent.width
                        text: root.view.fromAddress !== undefined ? root.view.fromAddress : ""
                        // The middle goes, so the @domain end stays visible.
                        elide: Text.ElideMiddle
                        color: root.theme.secondary
                        font.family: root.theme.fontAddress
                        font.pixelSize: 13
                    }
                }
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
                        // A value with an address is mono, so look-alikes (rn/m) stay distinct.
                        font.family: line.modelData.value.indexOf("@") >= 0
                            ? root.theme.fontAddress : root.theme.fontLabel
                        font.pixelSize: 13
                        lineHeight: 1.4
                    }
                }
            }
        }
        Rectangle { width: parent.width; height: 1; color: root.theme.hairline }
    }
}
