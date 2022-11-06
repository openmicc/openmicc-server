use std::{convert::Infallible, fmt::Display, ops::Deref, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::signup_receipt::SignupReceipt;

pub type SignupIdInner = usize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct SignupId(SignupIdInner);

impl Deref for SignupId {
    type Target = SignupIdInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for SignupId {
    // TODO: What's the right error to use?
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

impl From<SignupIdInner> for SignupId {
    fn from(inner: SignupIdInner) -> Self {
        Self(inner)
    }
}

// impl Deref for SignupId {
//     type Target = SignupIdInner;

//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }

/// Whatever the user wrote down on the list (probably their name)
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SignupListEntryText(String);

impl Display for SignupListEntryText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for SignupListEntryText {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl FromStr for SignupListEntryText {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(s.to_string().into())
    }
}

/// An entry on the signup list.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignupListEntry {
    pub id: SignupId,
    pub text: SignupListEntryText,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryAndReceipt {
    #[serde(flatten)]
    pub entry: SignupListEntry,
    pub receipt: SignupReceipt,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdAndReceipt {
    pub id: SignupId,
    pub receipt: SignupReceipt,
}
