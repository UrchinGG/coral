use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::extension::FlagLevel;
use crate::serde_ts;

#[derive(Debug, Clone, Serialize)]
pub struct ExtensionView {
    pub title: String,
    pub subtitle: Option<String>,
    pub sections: Vec<ViewSection>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewSection {
    Notice {
        level: FlagLevel,
        text: String,
        actions: Vec<ViewAction>,
    },
    Stats {
        items: Vec<ViewStat>,
    },
    Table {
        id: String,
        title: String,
        columns: Vec<ViewColumn>,
        rows: Vec<ViewRow>,
    },
    Form {
        id: String,
        title: String,
        fields: Vec<ViewField>,
        submit: ViewAction,
    },
    Series {
        id: String,
        title: String,
        unit: Option<String>,
        lines: Vec<SeriesLine>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct ViewStat {
    pub label: String,
    pub value: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ViewColumn {
    pub key: String,
    pub label: String,
    pub numeric: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ViewRow {
    pub key: String,
    pub cells: BTreeMap<String, ViewCell>,
    pub actions: Vec<ViewAction>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViewCell {
    Text {
        text: String,
        tone: ViewTone,
    },
    Mono {
        text: String,
    },
    Number {
        value: f64,
        suffix: Option<String>,
        tone: ViewTone,
    },
    Pill {
        text: String,
        tone: ViewTone,
    },
    Meter {
        used: f64,
        limit: f64,
    },
    Time {
        #[serde(serialize_with = "serde_ts::micros")]
        at: DateTime<Utc>,
    },
    Secret {
        value: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewTone {
    Neutral,
    Subtle,
    Ok,
    Info,
    Warning,
    Limited,
    Danger,
}

#[derive(Debug, Clone, Serialize)]
pub struct ViewField {
    pub name: String,
    pub label: String,
    pub required: bool,
    #[serde(flatten)]
    pub input: FieldInput,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldInput {
    Text {
        placeholder: Option<String>,
        value: Option<String>,
    },
    Secret {
        placeholder: Option<String>,
    },
    Select {
        options: Vec<FieldOption>,
        value: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ViewAction {
    pub action: String,
    pub label: String,
    pub danger: bool,
    pub input: Map<String, Value>,
    pub fields: Vec<ViewField>,
    pub confirm: Option<ViewConfirm>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ViewConfirm {
    pub title: String,
    pub text: String,
    pub phrase: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeriesLine {
    pub label: String,
    pub tone: ViewTone,
    pub points: Vec<SeriesPoint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeriesPoint {
    #[serde(serialize_with = "serde_ts::micros")]
    pub t: DateTime<Utc>,
    pub value: f64,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const SAMPLE_INSTANT: &str = "2026-10-14T12:30:00.25Z";
    const SERIALIZED_INSTANT: &str = "2026-10-14T12:30:00.250000Z";

    fn instant() -> DateTime<Utc> {
        SAMPLE_INSTANT.parse().unwrap()
    }

    fn plain_action(action: &str, label: &str) -> ViewAction {
        ViewAction {
            action: action.into(),
            label: label.into(),
            danger: false,
            input: Map::new(),
            fields: Vec::new(),
            confirm: None,
        }
    }

    fn select_field() -> ViewField {
        ViewField {
            name: "group".into(),
            label: "Group".into(),
            required: true,
            input: FieldInput::Select {
                options: vec![FieldOption {
                    value: "a".into(),
                    label: "Group A".into(),
                }],
                value: Some("a".into()),
            },
        }
    }

    fn guarded_action() -> ViewAction {
        ViewAction {
            danger: true,
            input: Map::from_iter([("id".to_string(), json!("item-1"))]),
            fields: vec![select_field()],
            confirm: Some(ViewConfirm {
                title: "Remove item-1?".into(),
                text: "This cannot be undone.".into(),
                phrase: Some("item-1".into()),
            }),
            ..plain_action("remove", "Remove")
        }
    }

    fn sample_row() -> ViewRow {
        let cells = [
            (
                "name",
                ViewCell::Mono {
                    text: "item-1".into(),
                },
            ),
            (
                "note",
                ViewCell::Text {
                    text: "none".into(),
                    tone: ViewTone::Subtle,
                },
            ),
            (
                "count",
                ViewCell::Number {
                    value: 12.0,
                    suffix: Some(" ms".into()),
                    tone: ViewTone::Limited,
                },
            ),
            (
                "state",
                ViewCell::Pill {
                    text: "active".into(),
                    tone: ViewTone::Ok,
                },
            ),
            (
                "usage",
                ViewCell::Meter {
                    used: 30.0,
                    limit: 120.0,
                },
            ),
            ("seen", ViewCell::Time { at: instant() }),
            (
                "secret",
                ViewCell::Secret {
                    value: "s3cret".into(),
                },
            ),
        ];
        ViewRow {
            key: "item-1".into(),
            cells: cells
                .into_iter()
                .map(|(key, cell)| (key.to_string(), cell))
                .collect(),
            actions: vec![guarded_action()],
        }
    }

    fn sample_view() -> ExtensionView {
        ExtensionView {
            title: "Sample".into(),
            subtitle: None,
            sections: vec![
                ViewSection::Notice {
                    level: FlagLevel::Warning,
                    text: "Check this".into(),
                    actions: vec![plain_action("check", "Check")],
                },
                ViewSection::Stats {
                    items: vec![ViewStat {
                        label: "Used".into(),
                        value: "30 / 120".into(),
                        detail: Some("per 5 min".into()),
                    }],
                },
                ViewSection::Table {
                    id: "items".into(),
                    title: "Items".into(),
                    columns: vec![ViewColumn {
                        key: "count".into(),
                        label: "Count".into(),
                        numeric: true,
                    }],
                    rows: vec![sample_row()],
                },
                ViewSection::Form {
                    id: "add".into(),
                    title: "Add item".into(),
                    fields: vec![
                        ViewField {
                            name: "secret".into(),
                            label: "Secret".into(),
                            required: true,
                            input: FieldInput::Secret { placeholder: None },
                        },
                        ViewField {
                            name: "note".into(),
                            label: "Note".into(),
                            required: false,
                            input: FieldInput::Text {
                                placeholder: Some("Optional".into()),
                                value: None,
                            },
                        },
                    ],
                    submit: plain_action("add", "Add"),
                },
                ViewSection::Series {
                    id: "load".into(),
                    title: "Load".into(),
                    unit: Some("req/min".into()),
                    lines: vec![SeriesLine {
                        label: "Requests".into(),
                        tone: ViewTone::Info,
                        points: vec![SeriesPoint {
                            t: instant(),
                            value: 4.5,
                        }],
                    }],
                },
            ],
        }
    }

    fn expected_plain_action(action: &str, label: &str) -> Value {
        json!({ "action": action, "label": label, "danger": false, "input": {}, "fields": [], "confirm": null })
    }

    fn expected_row() -> Value {
        json!({
            "key": "item-1",
            "cells": {
                "count": { "kind": "number", "value": 12.0, "suffix": " ms", "tone": "limited" },
                "name": { "kind": "mono", "text": "item-1" },
                "note": { "kind": "text", "text": "none", "tone": "subtle" },
                "secret": { "kind": "secret", "value": "s3cret" },
                "seen": { "kind": "time", "at": SERIALIZED_INSTANT },
                "state": { "kind": "pill", "text": "active", "tone": "ok" },
                "usage": { "kind": "meter", "used": 30.0, "limit": 120.0 }
            },
            "actions": [{
                "action": "remove",
                "label": "Remove",
                "danger": true,
                "input": { "id": "item-1" },
                "fields": [{
                    "name": "group",
                    "label": "Group",
                    "required": true,
                    "kind": "select",
                    "options": [{ "value": "a", "label": "Group A" }],
                    "value": "a"
                }],
                "confirm": { "title": "Remove item-1?", "text": "This cannot be undone.", "phrase": "item-1" }
            }]
        })
    }

    #[test]
    fn view_serializes_to_the_documented_wire_format() {
        let expected = json!({
            "title": "Sample",
            "subtitle": null,
            "sections": [
                { "type": "notice", "level": "warning", "text": "Check this", "actions": [expected_plain_action("check", "Check")] },
                { "type": "stats", "items": [{ "label": "Used", "value": "30 / 120", "detail": "per 5 min" }] },
                {
                    "type": "table",
                    "id": "items",
                    "title": "Items",
                    "columns": [{ "key": "count", "label": "Count", "numeric": true }],
                    "rows": [expected_row()]
                },
                {
                    "type": "form",
                    "id": "add",
                    "title": "Add item",
                    "fields": [
                        { "name": "secret", "label": "Secret", "required": true, "kind": "secret", "placeholder": null },
                        { "name": "note", "label": "Note", "required": false, "kind": "text", "placeholder": "Optional", "value": null }
                    ],
                    "submit": expected_plain_action("add", "Add")
                },
                {
                    "type": "series",
                    "id": "load",
                    "title": "Load",
                    "unit": "req/min",
                    "lines": [{ "label": "Requests", "tone": "info", "points": [{ "t": SERIALIZED_INSTANT, "value": 4.5 }] }]
                }
            ]
        });
        assert_eq!(serde_json::to_value(sample_view()).unwrap(), expected);
    }
}
