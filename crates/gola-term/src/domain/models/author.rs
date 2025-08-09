use serde::Deserialize;
use serde::Serialize;
use strum_macros::Display;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Display, Default)]
pub enum Author {
    User,
    Gola,
    #[default]
    Model,
}
