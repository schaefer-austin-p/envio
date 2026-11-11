import QtQuick 2.15
import QtQuick.Controls 2.15

// Fixture: swipe actions on a row; must be rejected.
SwipeDelegate {
    swipe.right: Item {}
}
