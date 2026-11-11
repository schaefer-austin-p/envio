import QtQuick 2.15

// Fixture: StyledText loads images; without a styled-ok reason it must be
// rejected.
Text {
    textFormat: Text.StyledText
    text: "from a sender"
}
