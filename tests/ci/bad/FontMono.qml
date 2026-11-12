// expect: fontMono
import QtQuick 2.15

// Fixture: the monospace font is for the certificate fingerprint only
// (TrustDialog.qml); a label takes Theme.fontLabel.
Text {
    required property var theme
    textFormat: Text.PlainText
    font.family: theme.fontMono
}
