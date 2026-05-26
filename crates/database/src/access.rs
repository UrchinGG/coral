#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccessRank {
    Default = 0,
    Trusted = 1,
    Helper = 2,
    Moderator = 3,
    Admin = 4,
    Owner = 5,
}

impl AccessRank {
    pub fn from_level(level: i16) -> Self {
        match level {
            5.. => Self::Owner,
            4 => Self::Admin,
            3 => Self::Moderator,
            2 => Self::Helper,
            1 => Self::Trusted,
            _ => Self::Default,
        }
    }

    pub fn to_level(self) -> i16 {
        self as i16
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Owner => "Owner",
            Self::Admin => "Admin",
            Self::Moderator => "Moderator",
            Self::Helper => "Helper",
            Self::Trusted => "Trusted",
            Self::Default => "Default",
        }
    }
}
