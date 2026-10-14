pub const ROUTE_TEMPLATES: &[&str] = &[
    "/",
    "/openapi.json",
    "/llms.txt",
    "/v3/player/tags",
    "/v3/player/profile",
    "/v3/player/skin",
    "/v3/player/face",
    "/v3/player/body",
    "/v3/player/winstreaks",
    "/v3/player/lock",
    "/v3/player/sessions/daily",
    "/v3/player/sessions/weekly",
    "/v3/player/sessions/monthly",
    "/v3/player/sessions/yearly",
    "/v3/player/sessions/custom",
    "/v3/player/sessions/markers",
    "/v3/player/sessions/markers/{name}",
    "/v3/player/sessions/snapshots",
    "/v3/players",
    "/v3/tags",
    "/v3/guild/sessions/daily",
    "/v3/guild/sessions/weekly",
    "/v3/guild/sessions/monthly",
    "/v3/guild/sessions/yearly",
    "/v3/guild/sessions/custom",
    "/v3/guild/sessions/snapshots",
    "/v3/hypixel/player",
    "/v3/hypixel/guild",
    "/v3/resolve/{identifier}",
    "/v3/verify/codes",
    "/v3/verify/codes/{code}",
    "/v3/cubelify",
    "/api/v1/starfish/auth/device",
    "/api/v1/starfish/auth/poll",
    "/api/v1/starfish/auth/oauth-url",
    "/api/v1/starfish/auth/oauth-callback",
    "/api/v1/starfish/auth/refresh",
    "/api/v1/starfish/auth/logout",
    "/api/v1/starfish/license/validate",
    "/api/v1/starfish/license/check",
    "/api/v1/starfish/heartbeat",
    "/api/v1/starfish/download/info",
    "/api/v1/starfish/download/releases",
    "/api/v1/starfish/download/latest",
    "/api/v1/starfish/users/me",
    "/api/v1/starfish/users/me/link-github",
    "/api/v1/starfish/plugins",
    "/api/v1/starfish/plugins/disabled",
    "/api/v1/starfish/plugins/installed",
    "/api/v1/starfish/plugins/attestation-info",
    "/api/v1/starfish/plugins/mine",
    "/api/v1/starfish/plugins/publish",
    "/api/v1/starfish/plugins/{slug}",
    "/api/v1/starfish/plugins/{slug}/body",
    "/api/v1/starfish/plugins/{slug}/install",
    "/api/v1/starfish/plugins/{slug}/rate",
    "/api/v1/starfish/plugins/{slug}/official",
    "/api/v1/starfish/plugins/{slug}/disable",
    "/api/v1/starfish/plugins/{slug}/releases/{version}",
    "/api/v1/starfish/plugins/{slug}/releases/{version}/yank",
    "/api/v1/starfish/plugins/{slug}/releases/{version}/unyank",
];

const PLAYER_LOOKUP_TEMPLATES: &[&str] = &[
    "/v3/player/tags",
    "/v3/player/profile",
    "/v3/player/skin",
    "/v3/player/face",
    "/v3/player/body",
    "/v3/hypixel/player",
];

const PLAYER_SESSION_TEMPLATES: &[&str] = &[
    "/v3/player/sessions/daily",
    "/v3/player/sessions/weekly",
    "/v3/player/sessions/monthly",
    "/v3/player/sessions/yearly",
    "/v3/player/sessions/custom",
    "/v3/player/sessions/markers",
    "/v3/player/sessions/markers/{name}",
    "/v3/player/sessions/snapshots",
];

const PLAYER_QUERY: &[SubjectSource] = &[
    SubjectSource::Query("player"),
    SubjectSource::Query("uuid"),
    SubjectSource::Query("name"),
];

const PLAYER_PARAM: &[SubjectSource] = &[SubjectSource::Query("player")];

pub const MAX_BATCH_UUIDS: usize = 100;
const GET: &str = "GET";
const HEAD: &str = "HEAD";

pub const ROUTE_SUBJECTS: &[RouteSubjects] = &[
    RouteSubjects {
        methods: Methods::Only(&["GET"]),
        templates: PLAYER_LOOKUP_TEMPLATES,
        sources: PLAYER_QUERY,
    },
    RouteSubjects {
        methods: Methods::Any,
        templates: PLAYER_SESSION_TEMPLATES,
        sources: PLAYER_PARAM,
    },
    RouteSubjects {
        methods: Methods::Only(&["GET"]),
        templates: &["/v3/player/winstreaks", "/v3/hypixel/guild"],
        sources: PLAYER_PARAM,
    },
    RouteSubjects {
        methods: Methods::Only(&["POST", "PATCH", "DELETE"]),
        templates: &["/v3/tags"],
        sources: PLAYER_QUERY,
    },
    RouteSubjects {
        methods: Methods::Only(&["POST", "DELETE"]),
        templates: &["/v3/player/lock"],
        sources: PLAYER_QUERY,
    },
    RouteSubjects {
        methods: Methods::Only(&["GET"]),
        templates: &["/v3/cubelify"],
        sources: &[
            SubjectSource::Query("uuid"),
            SubjectSource::QueryDisplay("name"),
        ],
    },
    RouteSubjects {
        methods: Methods::Only(&["GET"]),
        templates: &["/v3/resolve/{identifier}"],
        sources: &[SubjectSource::PathTail],
    },
    RouteSubjects {
        methods: Methods::Only(&["POST"]),
        templates: &["/v3/players"],
        sources: &[SubjectSource::BodyArray {
            field: "uuids",
            max: MAX_BATCH_UUIDS,
        }],
    },
    RouteSubjects {
        methods: Methods::Only(&["POST"]),
        templates: &["/v3/verify/codes"],
        sources: &[
            SubjectSource::BodyField("uuid"),
            SubjectSource::BodyField("username"),
        ],
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Methods {
    Any,
    Only(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectSource {
    Query(&'static str),
    QueryDisplay(&'static str),
    PathTail,
    BodyArray { field: &'static str, max: usize },
    BodyField(&'static str),
}

#[derive(Debug)]
pub struct RouteSubjects {
    pub methods: Methods,
    pub templates: &'static [&'static str],
    pub sources: &'static [SubjectSource],
}

impl Methods {
    fn admits(self, method: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Only(methods) => {
                methods.contains(&method) || (method == HEAD && methods.contains(&GET))
            }
        }
    }
}

impl SubjectSource {
    pub fn reads_body(self) -> bool {
        matches!(self, Self::BodyArray { .. } | Self::BodyField(_))
    }
}

pub fn route_subjects(method: &str, route: &str) -> Option<&'static RouteSubjects> {
    ROUTE_SUBJECTS
        .iter()
        .find(|entry| entry.methods.admits(method) && entry.templates.contains(&route))
}

pub fn is_mapped_route(route: &str) -> bool {
    ROUTE_SUBJECTS
        .iter()
        .any(|entry| entry.templates.contains(&route))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_subject_template_is_a_known_route() {
        for entry in ROUTE_SUBJECTS {
            for template in entry.templates {
                assert!(ROUTE_TEMPLATES.contains(template), "{template}");
            }
        }
    }

    #[test]
    fn templates_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for template in ROUTE_TEMPLATES {
            assert!(seen.insert(template), "{template}");
        }
    }

    #[test]
    fn subject_routes_respect_methods() {
        assert!(route_subjects("GET", "/v3/player/tags").is_some());
        assert!(route_subjects("POST", "/v3/player/tags").is_none());
        assert!(route_subjects("DELETE", "/v3/player/sessions/markers/{name}").is_some());
        assert!(route_subjects("GET", "/v3/tags").is_none());
        assert!(route_subjects("GET", "/v3/guild/sessions/daily").is_none());
    }

    #[test]
    fn head_requests_follow_the_get_mapping() {
        assert!(route_subjects("HEAD", "/v3/player/tags").is_some());
        assert!(route_subjects("HEAD", "/v3/cubelify").is_some());
        assert!(route_subjects("HEAD", "/v3/tags").is_none());
        assert!(route_subjects("HEAD", "/v3/player/sessions/daily").is_some());
    }

    #[test]
    fn mapped_routes_are_known_under_any_method() {
        assert!(is_mapped_route("/v3/tags"));
        assert!(is_mapped_route("/v3/players"));
        assert!(!is_mapped_route("/v3/guild/sessions/daily"));
    }
}
