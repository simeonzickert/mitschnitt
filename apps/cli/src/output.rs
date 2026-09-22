use serde::Serialize;

use crate::{Error, Result};
use anlg_agent_access::Pagination;

pub const JSON_SCHEMA_VERSION: &str = "1";

#[derive(Serialize)]
struct JsonResponse<'a, T> {
    schema_version: &'static str,
    command: &'static str,
    data: &'a T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pagination: Option<&'a Pagination>,
}

pub fn json(
    command: &'static str,
    value: &impl Serialize,
    pagination: Option<&Pagination>,
) -> Result<String> {
    serde_json::to_string_pretty(&JsonResponse {
        schema_version: JSON_SCHEMA_VERSION,
        command,
        data: value,
        pagination,
    })
    .map_err(|error| Error::operation("serialize output", error.to_string()))
}

pub fn emit(text: &str) {
    println!("{text}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_response_has_stable_version_and_pagination() {
        let pagination = Pagination {
            offset: 20,
            limit: 10,
            returned: 2,
            total: None,
            next_offset: None,
        };
        let response = json(
            "proposals.list",
            &serde_json::json!([{"id": "meeting-1"}]),
            Some(&pagination),
        )
        .unwrap();
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(response["schema_version"], "1");
        assert_eq!(response["command"], "proposals.list");
        assert_eq!(response["data"][0]["id"], "meeting-1");
        assert_eq!(response["pagination"]["offset"], 20);
        assert!(response["pagination"]["total"].is_null());
    }
}
