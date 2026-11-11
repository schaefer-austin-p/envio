import QtQuick 2.15

// Fixture: qsTranslate with a count (4 arguments); must be rejected.
Text {
    text: qsTranslate("ctx", "selected", "", 3)
}
