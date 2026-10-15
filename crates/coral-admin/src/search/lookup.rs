use std::net::IpAddr;

use uuid::Uuid;

use super::input::Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    Exact,
    Prefix,
    Contains,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    Account(i64),
    BotLookups(i64),
    MemberNumber(i64),
    Player { uuid: Uuid, dashed: bool },
    PlayerRequests(Uuid),
    Request(Uuid),
    ApiKey(Uuid),
    Guild(String),
    KeyPrefix(String),
    Ip(IpAddr),
    NameHolders { name: String, history: bool },
    NamedRequests(String),
    DiscordName(String, Reach),
    Plugin(String, Reach),
    GuildName(String, Reach),
}

impl Lookup {
    pub fn kinds(&self) -> &'static [Kind] {
        match self {
            Self::Account(_) | Self::BotLookups(_) | Self::MemberNumber(_) => &[Kind::Account],
            Self::DiscordName(..) => &[Kind::Account],
            Self::Player { .. } | Self::PlayerRequests(_) | Self::NameHolders { .. } => {
                &[Kind::Player]
            }
            Self::Request(_) => &[Kind::Request],
            Self::ApiKey(_) | Self::KeyPrefix(_) => &[Kind::Key],
            Self::Guild(_) | Self::GuildName(..) => &[Kind::Guild],
            Self::Ip(_) => &[Kind::Ip],
            Self::NamedRequests(_) => &[Kind::Name],
            Self::Plugin(..) => &[Kind::Plugin],
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Account(_) => "account by Discord id",
            Self::BotLookups(_) => "bot lookups by Discord id",
            Self::MemberNumber(_) => "account by member number",
            Self::Player { .. } => "player by UUID",
            Self::PlayerRequests(_) => "requests naming the UUID",
            Self::Request(_) => "request by id",
            Self::ApiKey(_) => "API key",
            Self::Guild(_) => "guild by id",
            Self::KeyPrefix(_) => "key prefix",
            Self::Ip(_) => "IP address",
            Self::NameHolders { .. } => "players holding the name",
            Self::NamedRequests(_) => "requests naming the name",
            Self::DiscordName(..) => "Discord username",
            Self::Plugin(..) => "plugin",
            Self::GuildName(..) => "guild by name",
        }
    }
}
