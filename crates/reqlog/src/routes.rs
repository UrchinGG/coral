pub const ROUTE_TEMPLATES: &[&str] = &[
    "/",
    "/openapi.json",
    "/llms.txt",
    "/auth/discord",
    "/auth/discord/callback",
    "/auth/me",
    "/auth/logout",
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
const PARAM_OPEN: char = '{';
const PARAM_CLOSE: char = '}';
const PARAM_PATTERN: &str = "[^/]+";
const REGEX_METACHARACTERS: &str = r".^$*+?()[]{}|\";

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

pub fn is_static_template(template: &str) -> bool {
    !template.contains(PARAM_OPEN)
}

pub fn static_templates() -> impl Iterator<Item = &'static str> {
    ROUTE_TEMPLATES
        .iter()
        .copied()
        .filter(|template| is_static_template(template))
}

pub fn template_for_path(path: &str) -> Option<&'static str> {
    static_templates()
        .find(|template| *template == path)
        .or_else(|| {
            ROUTE_TEMPLATES
                .iter()
                .copied()
                .filter(|template| !is_static_template(template))
                .find(|template| path_fits_template(path, template))
        })
}

pub fn template_head(template: &str) -> &str {
    template
        .split_once(PARAM_OPEN)
        .map_or(template, |(head, _)| head)
}

pub fn template_path_pattern(template: &str) -> String {
    let segments: Vec<String> = template.split('/').map(segment_pattern).collect();
    format!("^{}$", segments.join("/"))
}

fn path_fits_template(path: &str, template: &str) -> bool {
    let mut path_segments = path.split('/');
    let mut template_segments = template.split('/');
    loop {
        match (path_segments.next(), template_segments.next()) {
            (None, None) => return true,
            (Some(segment), Some(pattern)) if is_param(pattern) && !segment.is_empty() => {}
            (Some(segment), Some(pattern)) if segment == pattern => {}
            _ => return false,
        }
    }
}

fn is_param(segment: &str) -> bool {
    segment.starts_with(PARAM_OPEN) && segment.ends_with(PARAM_CLOSE)
}

fn segment_pattern(segment: &str) -> String {
    if is_param(segment) {
        return PARAM_PATTERN.to_string();
    }
    let mut escaped = String::with_capacity(segment.len());
    for c in segment.chars() {
        if REGEX_METACHARACTERS.contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_templates_become_anchored_segment_patterns() {
        assert_eq!(
            template_path_pattern("/v3/resolve/{identifier}"),
            "^/v3/resolve/[^/]+$"
        );
        assert_eq!(
            template_path_pattern("/api/v1/starfish/plugins/{slug}/releases/{version}"),
            "^/api/v1/starfish/plugins/[^/]+/releases/[^/]+$"
        );
        assert_eq!(template_path_pattern("/openapi.json"), r"^/openapi\.json$");
    }

    #[test]
    fn a_template_head_is_the_literal_text_before_its_first_parameter() {
        assert_eq!(template_head("/v3/resolve/{identifier}"), "/v3/resolve/");
        assert_eq!(
            template_head("/api/v1/starfish/plugins/{slug}/releases/{version}"),
            "/api/v1/starfish/plugins/"
        );
        assert_eq!(template_head("/v3/players"), "/v3/players");
    }

    #[test]
    fn legacy_paths_map_to_their_templates_statics_first() {
        assert_eq!(template_for_path("/v3/players"), Some("/v3/players"));
        assert_eq!(
            template_for_path("/v3/resolve/oJugg"),
            Some("/v3/resolve/{identifier}")
        );
        assert_eq!(
            template_for_path("/api/v1/starfish/plugins/disabled"),
            Some("/api/v1/starfish/plugins/disabled")
        );
        assert_eq!(
            template_for_path("/api/v1/starfish/plugins/bedwars/releases/1.2.0/yank"),
            Some("/api/v1/starfish/plugins/{slug}/releases/{version}/yank")
        );
        assert_eq!(template_for_path("/v3/resolve/"), None);
        assert_eq!(template_for_path("/v3/resolve/a/b"), None);
        assert_eq!(template_for_path("/wp-login.php"), None);
    }

    #[test]
    fn static_templates_exclude_parameterised_routes() {
        let statics: Vec<&str> = static_templates().collect();
        assert!(statics.contains(&"/v3/players"));
        assert!(statics.contains(&"/api/v1/starfish/plugins/disabled"));
        assert!(!statics.contains(&"/v3/resolve/{identifier}"));
    }

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
