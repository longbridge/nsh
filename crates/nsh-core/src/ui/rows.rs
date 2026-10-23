//! [`Rows`]: the rows of data a component takes from JS (`data: T[]`), read
//! natively as columns of numbers by key.

use std::sync::Arc;

use super::{
    JsValue, PropError, SharedString, TsTypes,
    js::{JsHost, JsIn, JsKind, JsOwned},
};

/// Rows of data a component takes, `T[]` in TypeScript (its props generic
/// over `T`: `#[props(generic)]`): JS objects whose number fields are read
/// once, when JS sets the prop, into a column per key, so the component
/// reads its values by key ([`Rows::column`]) without calling into JS
/// again. A field that is not a number on some row (missing, `null`, a
/// string) is `NaN` there: a gap.
#[derive(Debug, Clone, Default)]
pub struct Rows {
    len: usize,
    columns: Vec<(SharedString, Arc<[f64]>)>,
}

impl Rows {
    /// `len` rows with the number fields `columns` (each `len` long;
    /// shorter ones end in gaps).
    pub fn from_columns(
        len: usize,
        columns: impl IntoIterator<Item = (SharedString, Vec<f64>)>,
    ) -> Self {
        let columns = columns
            .into_iter()
            .map(|(key, mut values)| {
                values.resize(len, f64::NAN);
                (key, values.into())
            })
            .collect();
        Self { len, columns }
    }

    /// How many rows there are.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The values of field `key`, one per row (`NaN` where a row has no
    /// number there); `None` when no row has a number there.
    pub fn column(&self, key: &str) -> Option<&[f64]> {
        self.columns
            .iter()
            .find(|(k, _)| &**k == key)
            .map(|(_, values)| &**values)
    }

    /// The keys with numbers, in the order they first appear.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.columns.iter().map(|(k, _)| &**k)
    }
}

/// Equal rows hold the same numbers (gaps included, which `NaN`'s own
/// comparison would tell apart): a prop set to equal rows changes nothing.
impl PartialEq for Rows {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len
            && self.columns.len() == other.columns.len()
            && self.columns.iter().zip(&other.columns).all(|(a, b)| {
                a.0 == b.0
                    && a.1.len() == b.1.len()
                    && a.1
                        .iter()
                        .zip(b.1.iter())
                        .all(|(x, y)| x.to_bits() == y.to_bits())
            })
    }
}

impl JsValue for Rows {
    fn ts_type(_types: &mut TsTypes) -> String {
        "T[]".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        let len = value
            .array_len()?
            .ok_or_else(|| value.expected("an array of objects"))?;
        let mut columns: Vec<(SharedString, Vec<f64>)> = Vec::new();
        for i in 0..len {
            let row = value.index(i)?;
            let row = row.get();
            if row.kind() != JsKind::Object {
                continue;
            }
            for key in row.keys()? {
                let Some(n) = row.field(&key)?.get().as_number() else {
                    continue;
                };
                let at = match columns.iter().position(|(k, _)| **k == *key) {
                    Some(at) => at,
                    None => {
                        columns.push((SharedString::from(key), vec![f64::NAN; len]));
                        columns.len() - 1
                    }
                };
                columns[at].1[i] = n;
            }
        }
        Ok(Rows::from_columns(len, columns))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        // The numbers it read, one object per row.
        let mut rows = Vec::with_capacity(self.len);
        for i in 0..self.len {
            let row = JsOwned::new(host, host.new_object()?);
            for (key, values) in &self.columns {
                let n = values[i];
                if !n.is_nan() {
                    host.set_field(row.get().raw(), key, host.new_number(n))?;
                }
            }
            rows.push(row);
        }
        let raws = rows.into_iter().map(JsOwned::into_raw).collect();
        Ok(JsOwned::new(host, host.new_array(raws)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_by_key_with_gaps() {
        let rows = Rows::from_columns(
            3,
            [
                ("price".into(), vec![1.0, 2.0, 3.0]),
                ("avg".into(), vec![1.5]),
            ],
        );
        assert_eq!(rows.len(), 3);
        assert_eq!(rows.column("price"), Some(&[1.0, 2.0, 3.0][..]));
        let avg = rows.column("avg").unwrap();
        assert_eq!(avg[0], 1.5);
        assert!(avg[1].is_nan() && avg[2].is_nan());
        assert_eq!(rows.column("time"), None);
        assert_eq!(rows.keys().collect::<Vec<_>>(), ["price", "avg"]);
        // Gaps compare equal.
        assert_eq!(rows, rows.clone());
    }
}
