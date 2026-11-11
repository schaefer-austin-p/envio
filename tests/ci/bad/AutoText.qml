import QtQuick 2.15

// Fixture: AutoText is the guess that lets markup in; must be rejected.
Text {
    textFormat: Text.AutoText
    text: "from a sender"
}
