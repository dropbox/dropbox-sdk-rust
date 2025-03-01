// DO NOT EDIT
// This file was @generated by Stone

#![allow(
    clippy::too_many_arguments,
    clippy::large_enum_variant,
    clippy::result_large_err,
    clippy::doc_markdown,
)]

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive] // structs may have more fields added in the future.
pub struct DeleteManualContactsArg {
    /// List of manually added contacts to be deleted.
    pub email_addresses: Vec<crate::types::common::EmailAddress>,
}

impl DeleteManualContactsArg {
    pub fn new(email_addresses: Vec<crate::types::common::EmailAddress>) -> Self {
        DeleteManualContactsArg {
            email_addresses,
        }
    }
}

const DELETE_MANUAL_CONTACTS_ARG_FIELDS: &[&str] = &["email_addresses"];
impl DeleteManualContactsArg {
    pub(crate) fn internal_deserialize<'de, V: ::serde::de::MapAccess<'de>>(
        map: V,
    ) -> Result<DeleteManualContactsArg, V::Error> {
        Self::internal_deserialize_opt(map, false).map(Option::unwrap)
    }

    pub(crate) fn internal_deserialize_opt<'de, V: ::serde::de::MapAccess<'de>>(
        mut map: V,
        optional: bool,
    ) -> Result<Option<DeleteManualContactsArg>, V::Error> {
        let mut field_email_addresses = None;
        let mut nothing = true;
        while let Some(key) = map.next_key::<&str>()? {
            nothing = false;
            match key {
                "email_addresses" => {
                    if field_email_addresses.is_some() {
                        return Err(::serde::de::Error::duplicate_field("email_addresses"));
                    }
                    field_email_addresses = Some(map.next_value()?);
                }
                _ => {
                    // unknown field allowed and ignored
                    map.next_value::<::serde_json::Value>()?;
                }
            }
        }
        if optional && nothing {
            return Ok(None);
        }
        let result = DeleteManualContactsArg {
            email_addresses: field_email_addresses.ok_or_else(|| ::serde::de::Error::missing_field("email_addresses"))?,
        };
        Ok(Some(result))
    }

    pub(crate) fn internal_serialize<S: ::serde::ser::Serializer>(
        &self,
        s: &mut S::SerializeStruct,
    ) -> Result<(), S::Error> {
        use serde::ser::SerializeStruct;
        s.serialize_field("email_addresses", &self.email_addresses)?;
        Ok(())
    }
}

impl<'de> ::serde::de::Deserialize<'de> for DeleteManualContactsArg {
    fn deserialize<D: ::serde::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // struct deserializer
        use serde::de::{MapAccess, Visitor};
        struct StructVisitor;
        impl<'de> Visitor<'de> for StructVisitor {
            type Value = DeleteManualContactsArg;
            fn expecting(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str("a DeleteManualContactsArg struct")
            }
            fn visit_map<V: MapAccess<'de>>(self, map: V) -> Result<Self::Value, V::Error> {
                DeleteManualContactsArg::internal_deserialize(map)
            }
        }
        deserializer.deserialize_struct("DeleteManualContactsArg", DELETE_MANUAL_CONTACTS_ARG_FIELDS, StructVisitor)
    }
}

impl ::serde::ser::Serialize for DeleteManualContactsArg {
    fn serialize<S: ::serde::ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // struct serializer
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("DeleteManualContactsArg", 1)?;
        self.internal_serialize::<S>(&mut s)?;
        s.end()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive] // variants may be added in the future
pub enum DeleteManualContactsError {
    /// Can't delete contacts from this list. Make sure the list only has manually added contacts.
    /// The deletion was cancelled.
    ContactsNotFound(Vec<crate::types::common::EmailAddress>),
    /// Catch-all used for unrecognized values returned from the server. Encountering this value
    /// typically indicates that this SDK version is out of date.
    Other,
}

impl<'de> ::serde::de::Deserialize<'de> for DeleteManualContactsError {
    fn deserialize<D: ::serde::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // union deserializer
        use serde::de::{self, MapAccess, Visitor};
        struct EnumVisitor;
        impl<'de> Visitor<'de> for EnumVisitor {
            type Value = DeleteManualContactsError;
            fn expecting(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str("a DeleteManualContactsError structure")
            }
            fn visit_map<V: MapAccess<'de>>(self, mut map: V) -> Result<Self::Value, V::Error> {
                let tag: &str = match map.next_key()? {
                    Some(".tag") => map.next_value()?,
                    _ => return Err(de::Error::missing_field(".tag"))
                };
                let value = match tag {
                    "contacts_not_found" => {
                        match map.next_key()? {
                            Some("contacts_not_found") => DeleteManualContactsError::ContactsNotFound(map.next_value()?),
                            None => return Err(de::Error::missing_field("contacts_not_found")),
                            _ => return Err(de::Error::unknown_field(tag, VARIANTS))
                        }
                    }
                    _ => DeleteManualContactsError::Other,
                };
                crate::eat_json_fields(&mut map)?;
                Ok(value)
            }
        }
        const VARIANTS: &[&str] = &["contacts_not_found",
                                    "other"];
        deserializer.deserialize_struct("DeleteManualContactsError", VARIANTS, EnumVisitor)
    }
}

impl ::serde::ser::Serialize for DeleteManualContactsError {
    fn serialize<S: ::serde::ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // union serializer
        use serde::ser::SerializeStruct;
        match self {
            DeleteManualContactsError::ContactsNotFound(x) => {
                // primitive
                let mut s = serializer.serialize_struct("DeleteManualContactsError", 2)?;
                s.serialize_field(".tag", "contacts_not_found")?;
                s.serialize_field("contacts_not_found", x)?;
                s.end()
            }
            DeleteManualContactsError::Other => Err(::serde::ser::Error::custom("cannot serialize 'Other' variant"))
        }
    }
}

impl ::std::error::Error for DeleteManualContactsError {
}

impl ::std::fmt::Display for DeleteManualContactsError {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match self {
            DeleteManualContactsError::ContactsNotFound(inner) => write!(f, "Can't delete contacts from this list. Make sure the list only has manually added contacts. The deletion was cancelled: {:?}", inner),
            _ => write!(f, "{:?}", *self),
        }
    }
}

