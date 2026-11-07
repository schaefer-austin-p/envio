//! Which build is running (envio-core's `BuildInfo`). Constant for the
//! app's life.

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
        #[qproperty(QString, commit, READ, CONSTANT)]
        type BuildInfo = super::BuildInfoRust;
    }
}

use cxx_qt_lib::QString;
use envio_core::build_info::BuildInfo;

pub struct BuildInfoRust {
    version: QString,
    commit: QString,
}

impl Default for BuildInfoRust {
    fn default() -> Self {
        let info =
            BuildInfo::from_manifest(include_str!("../../manifest.json"), env!("ENVIO_COMMIT"))
                .expect("manifest.json is checked by envio-core's tests");
        Self {
            version: QString::from(info.version.as_str()),
            commit: QString::from(info.commit.as_str()),
        }
    }
}
