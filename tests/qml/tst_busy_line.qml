import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"

// BusyLine: the spinner is the accent role, the text the secondary role, in
// every look; it is hidden and still unless it is running.
Item {
    id: root
    width: 300
    height: 100

    Theme { id: thm }
    BusyLine { id: line; theme: thm; text: "Working…"; width: 300 }

    TestCase {
        name: "BusyLine"
        when: windowShown

        function child(name) {
            for (let i = 0; i < line.children.length; i++)
                if (line.children[i].objectName === name) return line.children[i]
            return null
        }

        function test_colours_come_from_the_theme() {
            line.running = true
            for (const look of ["carbon", "airmail"]) {
                for (const mode of ["light", "dark"]) {
                    thm.name = look
                    thm.darkMode = mode
                    const label = look + " " + mode
                    compare(child("busySpinner").stroke, thm.accent, label)
                    compare(child("busyText").color, thm.secondary, label)
                }
            }
        }

        function test_hidden_and_still_unless_running() {
            line.running = false
            verify(!line.visible)
            verify(!child("busySpinner").visible)
            line.running = true
            verify(line.visible)
            compare(child("busyText").text, "Working…")
        }
    }
}
