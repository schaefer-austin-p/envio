//! JSON values as QVariants, for the maps QML receives (`messageOpened`,
//! `composeReady`). A QVariantMap and QVariantList are what QML turns into
//! plain JS objects and arrays.

use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};
use serde_json::Value;

pub fn variant_of(value: &Value) -> QVariant {
    match value {
        Value::Null => QVariant::default(),
        Value::Bool(b) => QVariant::from(b),
        // A double: QML has no conversion for a QVariant of `long`, which
        // is what `std::int64_t` is on LP64 Linux.
        Value::Number(n) => QVariant::from(&n.as_f64().unwrap_or(0.0)),
        Value::String(s) => QVariant::from(&QString::from(s)),
        Value::Array(items) => {
            let mut list = QList::<QVariant>::default();
            for item in items {
                list.append(variant_of(item));
            }
            QVariant::from(&list)
        }
        Value::Object(_) => QVariant::from(&map_of(value)),
    }
}

/// An i64 as the number QML gets. Qt 5's QML engine converts a `long`
/// (`std::int64_t` on LP64) neither in a QVariant nor as a signal argument or
/// return value; it arrives as an opaque object. Ids and Unix seconds stay
/// far below 2^53, so the double is exact.
#[allow(clippy::cast_precision_loss)]
pub fn js_num(i: i64) -> f64 {
    i as f64
}

/// An object as a QVariantMap (anything else gives an empty map).
pub fn map_of(value: &Value) -> QMap<QMapPair_QString_QVariant> {
    let mut map = QMap::<QMapPair_QString_QVariant>::default();
    if let Value::Object(fields) = value {
        for (key, field) in fields {
            map.insert(QString::from(key), variant_of(field));
        }
    }
    map
}
