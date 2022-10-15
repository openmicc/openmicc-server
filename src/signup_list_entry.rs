use std::{convert::Infallible, fmt::Display, str::FromStr};

use redis::{FromRedisValue, ToRedisArgs};
use serde::{Deserialize, Serialize};

use crate::signup_receipt::SignupReceipt;

pub type SignupIdInner = usize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct SignupId(SignupIdInner);

impl From<SignupIdInner> for SignupId {
    fn from(inner: SignupIdInner) -> Self {
        Self(inner)
    }
}

impl FromRedisValue for SignupId {
    fn from_redis_value(v: &redis::Value) -> redis::RedisResult<Self> {
        FromRedisValue::from_redis_value(v).map(Self)
    }
}

impl ToRedisArgs for SignupId {
    fn write_redis_args<W>(&self, out: &mut W)
    where
        W: ?Sized + redis::RedisWrite,
    {
        self.0.write_redis_args(out)
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
pub struct SignupListEntry {
    pub id: SignupId,
    pub text: SignupListEntryText,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EntryAndReceipt {
    #[serde(flatten)]
    pub entry: SignupListEntry,
    pub receipt: SignupReceipt,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IdAndReceipt {
    pub id: SignupId,
    pub receipt: SignupReceipt,
}
