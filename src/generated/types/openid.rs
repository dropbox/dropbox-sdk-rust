// DO NOT EDIT
// This file was @generated by Stone

#![allow(
    clippy::too_many_arguments,
    clippy::large_enum_variant,
    clippy::doc_markdown,
)]

// for compatibility with old module structure
if_feature! {
    "sync",
    #[allow(unused_imports)]
    pub use crate::generated::routes::openid::*;
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive] // variants may be added in the future
pub enum OpenIdError {
    /// Missing openid claims for the associated access token.
    IncorrectOpenidScopes,
    /// Catch-all used for unrecognized values returned from the server. Encountering this value
    /// typically indicates that this SDK version is out of date.
    Other,
}

impl<'de> ::serde::de::Deserialize<'de> for OpenIdError {
    fn deserialize<D: ::serde::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // union deserializer
        use serde::de::{self, MapAccess, Visitor};
        struct EnumVisitor;
        impl<'de> Visitor<'de> for EnumVisitor {
            type Value = OpenIdError;
            fn expecting(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str("a OpenIdError structure")
            }
            fn visit_map<V: MapAccess<'de>>(self, mut map: V) -> Result<Self::Value, V::Error> {
                let tag: &str = match map.next_key()? {
                    Some(".tag") => map.next_value()?,
                    _ => return Err(de::Error::missing_field(".tag"))
                };
                let value = match tag {
                    "incorrect_openid_scopes" => OpenIdError::IncorrectOpenidScopes,
                    _ => OpenIdError::Other,
                };
                crate::eat_json_fields(&mut map)?;
                Ok(value)
            }
        }
        const VARIANTS: &[&str] = &["incorrect_openid_scopes",
                                    "other"];
        deserializer.deserialize_struct("OpenIdError", VARIANTS, EnumVisitor)
    }
}

impl ::serde::ser::Serialize for OpenIdError {
    fn serialize<S: ::serde::ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // union serializer
        use serde::ser::SerializeStruct;
        match *self {
            OpenIdError::IncorrectOpenidScopes => {
                // unit
                let mut s = serializer.serialize_struct("OpenIdError", 1)?;
                s.serialize_field(".tag", "incorrect_openid_scopes")?;
                s.end()
            }
            OpenIdError::Other => Err(::serde::ser::Error::custom("cannot serialize 'Other' variant"))
        }
    }
}

impl ::std::error::Error for OpenIdError {
}

impl ::std::fmt::Display for OpenIdError {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match self {
            OpenIdError::IncorrectOpenidScopes => f.write_str("Missing openid claims for the associated access token."),
            _ => write!(f, "{:?}", *self),
        }
    }
}

/// No Parameters
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive] // structs may have more fields added in the future.
pub struct UserInfoArgs {
}

const USER_INFO_ARGS_FIELDS: &[&str] = &[];
impl UserInfoArgs {
    // no _opt deserializer
    pub(crate) fn internal_deserialize<'de, V: ::serde::de::MapAccess<'de>>(
        mut map: V,
    ) -> Result<UserInfoArgs, V::Error> {
        // ignore any fields found; none are presently recognized
        crate::eat_json_fields(&mut map)?;
        Ok(UserInfoArgs {})
    }
}

impl<'de> ::serde::de::Deserialize<'de> for UserInfoArgs {
    fn deserialize<D: ::serde::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // struct deserializer
        use serde::de::{MapAccess, Visitor};
        struct StructVisitor;
        impl<'de> Visitor<'de> for StructVisitor {
            type Value = UserInfoArgs;
            fn expecting(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str("a UserInfoArgs struct")
            }
            fn visit_map<V: MapAccess<'de>>(self, map: V) -> Result<Self::Value, V::Error> {
                UserInfoArgs::internal_deserialize(map)
            }
        }
        deserializer.deserialize_struct("UserInfoArgs", USER_INFO_ARGS_FIELDS, StructVisitor)
    }
}

impl ::serde::ser::Serialize for UserInfoArgs {
    fn serialize<S: ::serde::ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // struct serializer
        use serde::ser::SerializeStruct;
        serializer.serialize_struct("UserInfoArgs", 0)?.end()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive] // variants may be added in the future
pub enum UserInfoError {
    OpenidError(OpenIdError),
    /// Catch-all used for unrecognized values returned from the server. Encountering this value
    /// typically indicates that this SDK version is out of date.
    Other,
}

impl<'de> ::serde::de::Deserialize<'de> for UserInfoError {
    fn deserialize<D: ::serde::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // union deserializer
        use serde::de::{self, MapAccess, Visitor};
        struct EnumVisitor;
        impl<'de> Visitor<'de> for EnumVisitor {
            type Value = UserInfoError;
            fn expecting(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str("a UserInfoError structure")
            }
            fn visit_map<V: MapAccess<'de>>(self, mut map: V) -> Result<Self::Value, V::Error> {
                let tag: &str = match map.next_key()? {
                    Some(".tag") => map.next_value()?,
                    _ => return Err(de::Error::missing_field(".tag"))
                };
                let value = match tag {
                    "openid_error" => {
                        match map.next_key()? {
                            Some("openid_error") => UserInfoError::OpenidError(map.next_value()?),
                            None => return Err(de::Error::missing_field("openid_error")),
                            _ => return Err(de::Error::unknown_field(tag, VARIANTS))
                        }
                    }
                    _ => UserInfoError::Other,
                };
                crate::eat_json_fields(&mut map)?;
                Ok(value)
            }
        }
        const VARIANTS: &[&str] = &["openid_error",
                                    "other"];
        deserializer.deserialize_struct("UserInfoError", VARIANTS, EnumVisitor)
    }
}

impl ::serde::ser::Serialize for UserInfoError {
    fn serialize<S: ::serde::ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // union serializer
        use serde::ser::SerializeStruct;
        match *self {
            UserInfoError::OpenidError(ref x) => {
                // union or polymporphic struct
                let mut s = serializer.serialize_struct("UserInfoError", 2)?;
                s.serialize_field(".tag", "openid_error")?;
                s.serialize_field("openid_error", x)?;
                s.end()
            }
            UserInfoError::Other => Err(::serde::ser::Error::custom("cannot serialize 'Other' variant"))
        }
    }
}

impl ::std::error::Error for UserInfoError {
    fn source(&self) -> Option<&(dyn ::std::error::Error + 'static)> {
        match self {
            UserInfoError::OpenidError(inner) => Some(inner),
            _ => None,
        }
    }
}

impl ::std::fmt::Display for UserInfoError {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match self {
            UserInfoError::OpenidError(inner) => write!(f, "{}", inner),
            _ => write!(f, "{:?}", *self),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive] // structs may have more fields added in the future.
pub struct UserInfoResult {
    /// Last name of user.
    pub family_name: Option<String>,
    /// First name of user.
    pub given_name: Option<String>,
    /// Email address of user.
    pub email: Option<String>,
    /// If user is email verified.
    pub email_verified: Option<bool>,
    /// Issuer of token (in this case Dropbox).
    pub iss: String,
    /// An identifier for the user. This is the Dropbox account_id, a string value such as
    /// dbid:AAH4f99T0taONIb-OurWxbNQ6ywGRopQngc.
    pub sub: String,
}

impl UserInfoResult {
    pub fn with_family_name(mut self, value: String) -> Self {
        self.family_name = Some(value);
        self
    }

    pub fn with_given_name(mut self, value: String) -> Self {
        self.given_name = Some(value);
        self
    }

    pub fn with_email(mut self, value: String) -> Self {
        self.email = Some(value);
        self
    }

    pub fn with_email_verified(mut self, value: bool) -> Self {
        self.email_verified = Some(value);
        self
    }

    pub fn with_iss(mut self, value: String) -> Self {
        self.iss = value;
        self
    }

    pub fn with_sub(mut self, value: String) -> Self {
        self.sub = value;
        self
    }
}

const USER_INFO_RESULT_FIELDS: &[&str] = &["family_name",
                                           "given_name",
                                           "email",
                                           "email_verified",
                                           "iss",
                                           "sub"];
impl UserInfoResult {
    // no _opt deserializer
    pub(crate) fn internal_deserialize<'de, V: ::serde::de::MapAccess<'de>>(
        mut map: V,
    ) -> Result<UserInfoResult, V::Error> {
        let mut field_family_name = None;
        let mut field_given_name = None;
        let mut field_email = None;
        let mut field_email_verified = None;
        let mut field_iss = None;
        let mut field_sub = None;
        while let Some(key) = map.next_key::<&str>()? {
            match key {
                "family_name" => {
                    if field_family_name.is_some() {
                        return Err(::serde::de::Error::duplicate_field("family_name"));
                    }
                    field_family_name = Some(map.next_value()?);
                }
                "given_name" => {
                    if field_given_name.is_some() {
                        return Err(::serde::de::Error::duplicate_field("given_name"));
                    }
                    field_given_name = Some(map.next_value()?);
                }
                "email" => {
                    if field_email.is_some() {
                        return Err(::serde::de::Error::duplicate_field("email"));
                    }
                    field_email = Some(map.next_value()?);
                }
                "email_verified" => {
                    if field_email_verified.is_some() {
                        return Err(::serde::de::Error::duplicate_field("email_verified"));
                    }
                    field_email_verified = Some(map.next_value()?);
                }
                "iss" => {
                    if field_iss.is_some() {
                        return Err(::serde::de::Error::duplicate_field("iss"));
                    }
                    field_iss = Some(map.next_value()?);
                }
                "sub" => {
                    if field_sub.is_some() {
                        return Err(::serde::de::Error::duplicate_field("sub"));
                    }
                    field_sub = Some(map.next_value()?);
                }
                _ => {
                    // unknown field allowed and ignored
                    map.next_value::<::serde_json::Value>()?;
                }
            }
        }
        let result = UserInfoResult {
            family_name: field_family_name.and_then(Option::flatten),
            given_name: field_given_name.and_then(Option::flatten),
            email: field_email.and_then(Option::flatten),
            email_verified: field_email_verified.and_then(Option::flatten),
            iss: field_iss.unwrap_or_default(),
            sub: field_sub.unwrap_or_default(),
        };
        Ok(result)
    }

    pub(crate) fn internal_serialize<S: ::serde::ser::Serializer>(
        &self,
        s: &mut S::SerializeStruct,
    ) -> Result<(), S::Error> {
        use serde::ser::SerializeStruct;
        if let Some(val) = &self.family_name {
            s.serialize_field("family_name", val)?;
        }
        if let Some(val) = &self.given_name {
            s.serialize_field("given_name", val)?;
        }
        if let Some(val) = &self.email {
            s.serialize_field("email", val)?;
        }
        if let Some(val) = &self.email_verified {
            s.serialize_field("email_verified", val)?;
        }
        if !self.iss.is_empty() {
            s.serialize_field("iss", &self.iss)?;
        }
        if !self.sub.is_empty() {
            s.serialize_field("sub", &self.sub)?;
        }
        Ok(())
    }
}

impl<'de> ::serde::de::Deserialize<'de> for UserInfoResult {
    fn deserialize<D: ::serde::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // struct deserializer
        use serde::de::{MapAccess, Visitor};
        struct StructVisitor;
        impl<'de> Visitor<'de> for StructVisitor {
            type Value = UserInfoResult;
            fn expecting(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str("a UserInfoResult struct")
            }
            fn visit_map<V: MapAccess<'de>>(self, map: V) -> Result<Self::Value, V::Error> {
                UserInfoResult::internal_deserialize(map)
            }
        }
        deserializer.deserialize_struct("UserInfoResult", USER_INFO_RESULT_FIELDS, StructVisitor)
    }
}

impl ::serde::ser::Serialize for UserInfoResult {
    fn serialize<S: ::serde::ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // struct serializer
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("UserInfoResult", 6)?;
        self.internal_serialize::<S>(&mut s)?;
        s.end()
    }
}
