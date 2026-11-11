//! JSON values as QVariants, for the maps QML receives (`messageOpened`,
//! `composeReady`). A QVariantMap and QVariantList are what QML turns into
//! plain JS objects and arrays.

use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};
use serde_json::Value;

pub fn variant_of(value: &Value) -> QVariant {
    match value {
        Value::Null => QVariant::default(),
        Value::Bool(b) => QVariant::from(b),
        Value::Number(n) => n.as_i64().map_or_else(
            || QVariant::from(&n.as_f64().unwrap_or(0.0)),
            |i| QVariant::from(&i),
        ),
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
