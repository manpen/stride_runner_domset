use paste::paste;
use serde::{Serialize, Serializer};
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

// Use strong type for instance id (IId) and data id (DId)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct IId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DId(pub u32);

macro_rules! impl_id {
    ($name:ident, $lower_case_name:ident) => {
        paste! {
            impl FromStr for $name {
                type Err = <u32 as FromStr>::Err;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    u32::from_str(s).map($name)
                }
            }

            impl Serialize for $name {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: Serializer,
                {
                    serializer.serialize_u32(self.0)
                }
            }

            impl Hash for $name {
                fn hash<H: Hasher>(&self, state: &mut H) {
                    self.0.hash(state);
                }
            }

            impl Debug for $name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, concat!(stringify!($name), "({})"), self.0)
                }
            }

            impl $name {
                pub const fn new(id: u32) -> Self {
                    Self(id)
                }

                pub const fn [< $lower_case_name _to_u32 >](self) -> u32 {
                    self.0
                }
            }

            #[cfg(test)]
            mod [< test_ $lower_case_name >] {
                use super::*;

                #[test]
                fn from_str() {
                    assert_eq!($name::from_str("42").unwrap().[< $lower_case_name _to_u32 >](), 42);
                }

                #[test]
                fn serialize() {
                    assert_eq!(
                        serde_json::to_string(&$name(1337)).unwrap(),
                        "1337".to_string()
                    );
                }

                #[test]
                fn debug() {
                    assert_eq!(format!("{:?}", $name(42)), concat!(stringify!($name), "(42)"));
                }

                #[test]
                fn to_u32() {
                    assert_eq!($name(42).[< $lower_case_name _to_u32 >](), 42);
                }
            }
        }
    };
}

impl_id!(IId, iid);
impl_id!(DId, did);
