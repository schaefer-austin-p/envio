import QtQuick 2.15

// Fixture: must pass. Every Text states its format; a comment in a string
// or a name like DateText is not a Text.
Item {
    Text { textFormat: Text.PlainText; text: "sender" }
    Text {
        text: "body"
        textFormat: Text.RichText
        Text { text: "nested"; textFormat: Text.PlainText }
    }
    // styled-ok: our own fixed strings with <b>, never outside data.
    Text { textFormat: Text.StyledText; text: qsTr("<b>Hi</b>") }
    DateText { text: "Text { not a Text }" }
    property string s: "Text { }"
}
