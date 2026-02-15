use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub type FieldFlags = BTreeSet<FieldFlag>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify_next::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "snake_case")]
pub enum FieldFlag {
    #[serde(rename = "pk")]
    PrimaryKey,
    #[serde(rename = "nn")]
    NonNull,
    #[serde(rename = "uq")]
    Unique,
}
