// Starts Qt and loads the QML. Everything else is in Rust (qt/src,
// crates/) or QML; BuildInfo registers itself in the "Envio" QML module
// that cxx-qt-build generates.

#include <libintl.h>

#include <QByteArray>
#include <QGuiApplication>
#include <QQmlContext>
#include <QQmlApplicationEngine>
#include <QString>
#include <QTranslator>
#include <QUrl>

namespace {

// qsTr() in QML asks the installed translators; this one answers from the
// gettext catalogue (domain ENVIO_APP_ID), the same one the Rust core uses.
// The disambiguation comment is gettext's msgctxt. A null QString means
// "none", and Qt shows the source text.
class GettextTranslator : public QTranslator
{
public:
    using QTranslator::QTranslator;

    [[nodiscard]] QString translate(
        const char * /*context*/, const char *sourceText, const char *disambiguation, int /*n*/) const override
    {
        if (sourceText == nullptr) {
            return {};
        }
        QByteArray key(sourceText);
        if (disambiguation != nullptr) {
            key = QByteArray(disambiguation) + '\004' + key;
        }
        const char *text = dgettext(ENVIO_APP_ID, key.constData());
        // gettext hands back its argument when there is no translation
        return text == key.constData() ? QString() : QString::fromUtf8(text);
    }

    [[nodiscard]] bool isEmpty() const override { return false; }
};

// Lomiri sizes everything in grid units of GRID_UNIT_PX pixels, 8 at scale
// 1. A Controls 2 app without the Lomiri toolkit gets no scaling of its
// own, so turn the grid unit into Qt's scale factor. Must run before the
// QGuiApplication exists; an explicit QT_SCALE_FACTOR wins.
void scaleFromGridUnit()
{
    bool ok = false;
    const int gridUnit = qEnvironmentVariableIntValue("GRID_UNIT_PX", &ok);
    if (!ok || gridUnit <= 0 || qEnvironmentVariableIsSet("QT_SCALE_FACTOR")) {
        return;
    }
    qputenv("QT_SCALE_FACTOR", QByteArray::number(gridUnit / 8.0));
}

} // namespace

int main(int argc, char *argv[])
{
    scaleFromGridUnit();
    const QGuiApplication app(argc, argv);
    // Must equal the click name: the confined XDG folders are named after it.
    QGuiApplication::setApplicationName(QStringLiteral(ENVIO_APP_ID));
    QGuiApplication::setOrganizationName(QStringLiteral(ENVIO_APP_ID));

    // The click installs share/locale next to the binary.
    const QByteArray locale = (QGuiApplication::applicationDirPath() + QStringLiteral("/share/locale")).toUtf8();
    bindtextdomain(ENVIO_APP_ID, locale.constData());
    bind_textdomain_codeset(ENVIO_APP_ID, "UTF-8");
    textdomain(ENVIO_APP_ID);
    GettextTranslator translator;
    QGuiApplication::installTranslator(&translator);

    QQmlApplicationEngine engine;
    // Lomiri sets GRID_UNIT_PX for its apps and nothing else does: the one
    // sign of the phone session the QML can use (platform/WindowMode.qml).
    engine.rootContext()->setContextProperty(QStringLiteral("lomiriSession"), qEnvironmentVariableIsSet("GRID_UNIT_PX"));
    engine.load(QUrl(QStringLiteral("qrc:/qml/Main.qml")));
    if (engine.rootObjects().isEmpty()) {
        return 1;
    }
    return QGuiApplication::exec();
}
