//! Which build is running (envio-core's `BuildInfo`), for the About text
//! and the start-up log. Constant for the app's life.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, version, READ, CONSTANT)]
        // False in an official release: then the rest is empty.
        #[qproperty(bool, dev, READ, CONSTANT)]
        #[qproperty(QString, source, READ, CONSTANT)]
        // Milliseconds since 1970 for a QML Date, 0 if not known.
        #[qproperty(f64, built, READ, CONSTANT)]
        #[qproperty(f64, installed, READ, CONSTANT)]
        type BuildInfo = super::BuildInfoRust;
    }
}

use cxx_qt_lib::QString;
use envio_core::build_info::BuildInfo;

pub struct BuildInfoRust {
    version: QString,
    dev: bool,
    source: QString,
    built: f64,
    installed: f64,
}

impl Default for BuildInfoRust {
    fn default() -> Self {
        let mut info = BuildInfo::from_manifest(
            include_str!("../../manifest.json"),
            envio_stamp::SOURCE,
            envio_stamp::BUILT,
        )
        .expect("manifest.json is checked by envio-core's tests");
        if let Some(dir) = std::env::current_exe()
            .ok()
            .as_deref()
            .and_then(|e| e.parent())
        {
            info = info.installed_from(dir);
        }
        let ms = |s: Option<u64>| s.map_or(0.0, |s| s as f64 * 1000.0);
        let dev = info.dev.as_ref();
        Self {
            version: QString::from(info.version.as_str()),
            dev: dev.is_some(),
            source: QString::from(dev.map_or("", |d| d.source.as_str())),
            built: ms(dev.and_then(|d| d.built)),
            installed: ms(dev.and_then(|d| d.installed)),
        }
    }
}
