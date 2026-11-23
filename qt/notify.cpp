// org.freedesktop.Notifications over QtDBus for desktops: calls and signal
// hookup, no logic. The Rust side (qt/src/notify/) decides what to send and
// keeps the card id per tag; this puts the typed values it is given into
// Notify, hands the reply (the id, 0 on any failure) back through a
// function pointer, and reports NotificationClosed, ActionInvoked, the
// server's capabilities and a change of the server's owner. Rust registers
// the pointers (envio_notify_init), so this file needs no Rust symbols at
// link time. The core calls from its worker thread, which has no event
// loop: calls are queued to the main thread and made there, asynchronously.

#include <QtCore/QByteArray>
#include <QtCore/QCoreApplication>
#include <QtCore/QDebug>
#include <QtCore/QMetaObject>
#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QVariant>
#include <QtCore/QVariantMap>
#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusMessage>
#include <QtDBus/QDBusPendingCall>
#include <QtDBus/QDBusPendingCallWatcher>
#include <QtDBus/QDBusPendingReply>
#include <QtGui/QGuiApplication>
#include <QtGui/QWindow>

namespace {

const char kService[] = "org.freedesktop.Notifications";
const char kPath[] = "/org/freedesktop/Notifications";
const char kInterface[] = "org.freedesktop.Notifications";
// The one blocking probe (Postal) never waits longer than this.
const int kProbeTimeoutMs = 1000;

// A UTF-8 slice, as in qt/src/notify/mod.rs.
struct Str {
    const char* ptr;
    int len;
};

// Mirrors `Card` in qt/src/notify/mod.rs.
struct Card {
    Str tag;
    Str appName;
    Str icon;
    Str summary;
    Str body;
    Str category;
    Str desktopEntry;
    Str imagePath;
    unsigned int replacesId;
    unsigned int generation;
    unsigned char urgency;
};

QString text(Str s)
{
    return QString::fromUtf8(s.ptr, s.len);
}

using ReplyFn = void (*)(const char* tag, int tag_len, unsigned int id, unsigned int generation);
using CardFn = void (*)(unsigned int id);
using TokenFn = void (*)(unsigned int id, const char* token, int token_len);
using CapsFn = void (*)(int answer, unsigned int generation);
using OwnerFn = void (*)(bool hadOwner);

ReplyFn g_onReply = nullptr;
CardFn g_onClosed = nullptr;
CardFn g_onClicked = nullptr;
TokenFn g_onToken = nullptr;
CapsFn g_onCaps = nullptr;
OwnerFn g_onOwner = nullptr;

// Always answers Rust, so a tag never waits for a reply that won't come.
void reply(const QString& tag, unsigned int id, unsigned int generation)
{
    if (g_onReply == nullptr) {
        return;
    }
    const QByteArray bytes = tag.toUtf8();
    g_onReply(bytes.constData(), static_cast<int>(bytes.size()), id, generation);
}

class Signals : public QObject {
    Q_OBJECT
public Q_SLOTS:
    void onActionInvoked(uint id, const QString& key)
    {
        if (key == QStringLiteral("default") && g_onClicked != nullptr) {
            g_onClicked(id);
        }
    }
    // Sent by the server just before ActionInvoked, for xdg-activation.
    void onActivationToken(uint id, const QString& token)
    {
        if (g_onToken != nullptr) {
            const QByteArray bytes = token.toUtf8();
            g_onToken(id, bytes.constData(), static_cast<int>(bytes.size()));
        }
    }
    void onClosed(uint id, uint)
    {
        if (g_onClosed != nullptr) {
            g_onClosed(id);
        }
    }
    void onNameOwnerChanged(const QString& name, const QString& oldOwner, const QString&)
    {
        if (name == QString::fromLatin1(kService) && g_onOwner != nullptr) {
            g_onOwner(!oldOwner.isEmpty());
        }
    }
};

QDBusMessage notificationsCall(const QString& method)
{
    return QDBusMessage::createMethodCall(QString::fromLatin1(kService),
                                          QString::fromLatin1(kPath),
                                          QString::fromLatin1(kInterface), method);
}

// The main window: the first visible top-level that is a plain window (not
// a popup, tooltip or splash).
QWindow* mainWindow()
{
    const QWindowList windows = QGuiApplication::topLevelWindows();
    for (QWindow* window : windows) {
        if (window->isVisible() && (window->flags() & Qt::WindowType_Mask) == Qt::Window) {
            return window;
        }
    }
    return nullptr;
}

} // namespace

extern "C" {

// Registered or activatable on the session bus. Any failure: false. One
// short blocking probe, only for Postal and only when APP_ID does not
// already say Ubuntu Touch.
bool envio_bus_has_service(const char* name, int len)
{
    const QString service = QString::fromUtf8(name, len);
    if (!QDBusConnection::sessionBus().isConnected()) {
        return false;
    }
    auto ask = [](const QString& method, const QVariantList& args) {
        QDBusMessage message = QDBusMessage::createMethodCall(
            QStringLiteral("org.freedesktop.DBus"), QStringLiteral("/org/freedesktop/DBus"),
            QStringLiteral("org.freedesktop.DBus"), method);
        message.setArguments(args);
        return QDBusConnection::sessionBus().call(message, QDBus::Block, kProbeTimeoutMs);
    };
    const QDBusMessage owner = ask(QStringLiteral("NameHasOwner"), {service});
    if (owner.type() == QDBusMessage::ReplyMessage && !owner.arguments().isEmpty()
        && owner.arguments().first().toBool()) {
        return true;
    }
    const QDBusMessage activatable = ask(QStringLiteral("ListActivatableNames"), {});
    return activatable.type() == QDBusMessage::ReplyMessage && !activatable.arguments().isEmpty()
        && activatable.arguments().first().toStringList().contains(service);
}

// Once, on the main thread: the callbacks, and the signals. The service
// name is given for the notification signals, so Qt follows the server
// across a restart; NameOwnerChanged tells Rust when that happens.
void envio_notify_init(ReplyFn onReply, CardFn onClosed, CardFn onClicked, TokenFn onToken,
                       CapsFn onCaps, OwnerFn onOwner)
{
    g_onReply = onReply;
    g_onClosed = onClosed;
    g_onClicked = onClicked;
    g_onToken = onToken;
    g_onCaps = onCaps;
    g_onOwner = onOwner;
    static Signals signalsObject;
    QDBusConnection bus = QDBusConnection::sessionBus();
    const QString service = QString::fromLatin1(kService);
    const QString path = QString::fromLatin1(kPath);
    const QString interface = QString::fromLatin1(kInterface);
    bus.connect(service, path, interface, QStringLiteral("ActionInvoked"), &signalsObject,
                SLOT(onActionInvoked(uint, QString)));
    bus.connect(service, path, interface, QStringLiteral("ActivationToken"), &signalsObject,
                SLOT(onActivationToken(uint, QString)));
    bus.connect(service, path, interface, QStringLiteral("NotificationClosed"), &signalsObject,
                SLOT(onClosed(uint, uint)));
    bus.connect(QStringLiteral("org.freedesktop.DBus"), QStringLiteral("/org/freedesktop/DBus"),
                QStringLiteral("org.freedesktop.DBus"), QStringLiteral("NameOwnerChanged"),
                &signalsObject, SLOT(onNameOwnerChanged(QString, QString, QString)));
}

// GetCapabilities, asynchronous. The answer goes to Rust: 0 failed, 1 no
// `body-markup`, 2 `body-markup`.
void envio_notify_query_caps(unsigned int generation)
{
    QCoreApplication* app = QCoreApplication::instance();
    if (app == nullptr) {
        if (g_onCaps != nullptr) {
            g_onCaps(0, generation);
        }
        return;
    }
    QMetaObject::invokeMethod(
        app,
        [generation]() {
            const QDBusPendingCall call =
                QDBusConnection::sessionBus().asyncCall(notificationsCall(QStringLiteral("GetCapabilities")));
            auto* watcher = new QDBusPendingCallWatcher(call);
            QObject::connect(watcher, &QDBusPendingCallWatcher::finished, watcher, [watcher, generation]() {
                const QDBusPendingReply<QStringList> answer = *watcher;
                if (g_onCaps != nullptr) {
                    if (answer.isError()) {
                        g_onCaps(0, generation);
                    } else {
                        g_onCaps(answer.value().contains(QStringLiteral("body-markup")) ? 2 : 1,
                                 generation);
                    }
                }
                watcher->deleteLater();
            });
        },
        Qt::QueuedConnection);
}

// Notify(app_name, replaces_id, icon, summary, body, actions, hints,
// timeout) = susssasa{sv}i; the reply is the card's id (u).
void envio_notify_send(const Card* card)
{
    const QString tagText = text(card->tag);
    const unsigned int generation = card->generation;
    QCoreApplication* app = QCoreApplication::instance();
    if (app == nullptr) {
        // No loop to queue to: answer directly, so the tag is not stuck.
        reply(tagText, 0, generation);
        return;
    }
    QVariantMap hints{
        {QStringLiteral("category"), text(card->category)},
        {QStringLiteral("desktop-entry"), text(card->desktopEntry)},
        {QStringLiteral("urgency"), QVariant::fromValue<uchar>(card->urgency)},
    };
    if (card->imagePath.len > 0) {
        hints.insert(QStringLiteral("image-path"), text(card->imagePath));
    }
    const QVariantList args{
        text(card->appName),
        QVariant::fromValue<quint32>(card->replacesId),
        text(card->icon),
        text(card->summary),
        text(card->body),
        QStringList{QStringLiteral("default"), QString()},
        hints,
        QVariant::fromValue<qint32>(-1),
    };
    QMetaObject::invokeMethod(
        app,
        [tagText, generation, args]() {
            QDBusConnection bus = QDBusConnection::sessionBus();
            if (!bus.isConnected()) {
                reply(tagText, 0, generation);
                return;
            }
            QDBusMessage message = notificationsCall(QStringLiteral("Notify"));
            message.setArguments(args);
            const QDBusPendingCall call = bus.asyncCall(message);
            if (call.isError()) {
                reply(tagText, 0, generation);
                return;
            }
            auto* watcher = new QDBusPendingCallWatcher(call);
            QObject::connect(watcher, &QDBusPendingCallWatcher::finished, watcher,
                             [watcher, tagText, generation]() {
                                 const QDBusPendingReply<quint32> answer = *watcher;
                                 if (answer.isError()) {
                                     qWarning().noquote()
                                         << "envio: Notify failed:" << answer.error().message();
                                     reply(tagText, 0, generation);
                                 } else {
                                     reply(tagText, answer.value(), generation);
                                 }
                                 watcher->deleteLater();
                             });
        },
        Qt::QueuedConnection);
}

// Best effort: Wayland may refuse to raise a window without a token. The
// server's xdg-activation token (empty: none) goes to the toolkit the way
// launchers hand it over, in the environment, before the request. Only a
// minimised window is restored; a maximised or fullscreen one stays as it is.
void envio_activate_window(const char* token, int tokenLen)
{
    if (tokenLen > 0) {
        const QByteArray value(token, tokenLen);
        qputenv("XDG_ACTIVATION_TOKEN", value);
        qputenv("DESKTOP_STARTUP_ID", value);
    }
    QWindow* window = mainWindow();
    if (window == nullptr) {
        return;
    }
    if (window->windowStates().testFlag(Qt::WindowMinimized)) {
        window->setWindowStates(window->windowStates() & ~Qt::WindowMinimized);
        window->show();
    }
    window->raise();
    window->requestActivate();
}

} // extern "C"

#include "notify.moc"
