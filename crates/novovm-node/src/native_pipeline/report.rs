//! Diagnostic snapshots carry owned fields; creating one does not build a JSON map.

macro_rules! record {
    ($($field:ident: $value:expr),+ $(,)?) => {{
        #[allow(non_camel_case_types)]
        struct Record<$($field),+> {
            $($field: $field),+
        }
        #[allow(non_camel_case_types)]
        impl<$($field: serde::Serialize),+> serde::Serialize for Record<$($field),+> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeStruct as _;
                let mut out = serializer.serialize_struct("DiagnosticRecord", [$(stringify!($field)),+].len())?;
                $(out.serialize_field(stringify!($field), &self.$field)?;)+
                out.end()
            }
        }
        Record { $($field: $value),+ }
    }};
}

pub(crate) use record;

/// Defer an existing diagnostic formatter until the owned snapshot is encoded.
pub(crate) struct Deferred<F>(pub F);

impl<F: Fn() -> serde_json::Value> serde::Serialize for Deferred<F> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&(self.0)(), serializer)
    }
}
