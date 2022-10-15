// Redis keys
pub const SIGNUP_LIST: &str = "signup_list";
pub const SIGNUP_COUNTER: &str = "signup_counter";

// Redis PubSub topics (a.k.a. channels)
pub const SIGNUP_TOPIC: &str = "new_signups";
pub const REMOVAL_TOPIC: &str = "signup_removals";
