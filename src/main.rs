mod qrc;

#[macro_use]
extern crate cstr;

#[macro_use]
extern crate qmetaobject;

use std::env;
use std::path::PathBuf;

use cpp::cpp;
use gettextrs::{bindtextdomain, textdomain};
use qmetaobject::{QQuickStyle, QString, QVariantMap, QmlEngine};

const THEME: &str = "Suru";

/// Empty shell for now.
fn main() {
    init_gettext();
    cpp! {{
        #include <QtCore/QCoreApplication>
        #include <QtCore/QString>
    }}
    unsafe {
        cpp! {[]{
            QCoreApplication::setApplicationName(QStringLiteral("envio.aps"));
            // Confined apps can only write to ~/.config/<app_identifier>
            QCoreApplication::setOrganizationName(QStringLiteral("envio.aps"));
        }}
    }

    QQuickStyle::set_style(THEME);
    qrc::load();

    let mut engine = QmlEngine::new();
    engine.set_property("buildInfo".into(), build_info().into());
    engine.load_file("qrc:/qml/Main.qml".into());
    engine.exec();
}

fn build_info() -> QVariantMap {
    QVariantMap::from_iter(
        [
            ("version", QString::from(env!("CARGO_PKG_VERSION"))),
            ("commit", QString::from(env!("ENVIO_COMMIT"))),
            ("authors", QString::from(env!("CARGO_PKG_AUTHORS"))),
            ("repository", QString::from(env!("CARGO_PKG_REPOSITORY"))),
        ]
        .into_iter(),
    )
}

fn init_gettext() {
    let domain = "envio.aps";
    textdomain(domain).expect("Failed to set gettext domain");

    let mut app_dir_path = env::current_dir().expect("Failed to get the app working directory");
    if !app_dir_path.is_absolute() {
        app_dir_path = PathBuf::from("/usr");
    }

    let path = app_dir_path.join("share/locale");

    bindtextdomain(domain, path.to_str().unwrap()).expect("Failed to bind gettext domain");
}
