use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 request message.
///
/// Used for all client → server MCP messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 response message.
///
/// Used for all server → client MCP messages. Contains either a `result`
/// or an `error`, never both.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Standard JSON-RPC 2.0 error codes.
impl JsonRpcError {
    /// Invalid JSON was received by the server.
    pub const PARSE_ERROR: i32 = -32700;

    /// The JSON sent is not a valid Request object.
    pub const INVALID_REQUEST: i32 = -32600;

    /// The method does not exist / is not available.
    pub const METHOD_NOT_FOUND: i32 = -32601;

    /// Invalid method parameter(s).
    pub const INVALID_PARAMS: i32 = -32602;

    /// Internal JSON-RPC error.
    pub const INTERNAL_ERROR: i32 = -32603;
}

/// MCP server capabilities advertised during initialization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
}

/// Capability flags for the `tools` feature.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PromptsCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Result returned from a `tools/call` invocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// A single content item inside a [`ToolResult`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// Definition of a resource exposed by the MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceDefinition {
    pub uri: String,
    pub name: String,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Definition of a prompt exposed by the MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

/// A single argument definition for a [`PromptDefinition`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptArgument {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ------------------------------------------------------------------
    // JSON-RPC request parsing
    // ------------------------------------------------------------------

    #[test]
    fn parse_valid_initialize_request() {
        let raw = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "sampling": {},
                    "roots": { "listChanged": true }
                },
                "clientInfo": { "name": "hermes", "version": "0.8.0" }
            }
        }"#;

        let req: JsonRpcRequest = serde_json::from_str(raw).expect("valid JSON-RPC request");
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.id, Some(json!(1)));
        assert_eq!(req.method, "initialize");
        assert!(req.params.is_some());
    }

    #[test]
    fn parse_tools_call_request() {
        let raw = r#"{
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "brain_get",
                "arguments": {
                    "slug": "wiki/index.md",
                    "vault": "forge"
                }
            }
        }"#;

        let req: JsonRpcRequest = serde_json::from_str(raw).expect("valid tools/call request");
        assert_eq!(req.method, "tools/call");
        assert_eq!(req.id, Some(json!(2)));

        let params = req.params.unwrap();
        assert_eq!(params["name"], "brain_get");
        assert_eq!(params["arguments"]["slug"], "wiki/index.md");
    }

    #[test]
    fn parse_notification_without_id() {
        // Notifications have no `id` field.
        let raw = r#"{
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }"#;

        let req: JsonRpcRequest = serde_json::from_str(raw).expect("valid notification");
        assert_eq!(req.id, None);
        assert_eq!(req.method, "notifications/initialized");
        assert_eq!(req.params, None);
    }

    #[test]
    fn parse_request_with_string_id() {
        let raw = r#"{
            "jsonrpc": "2.0",
            "id": "abc-123",
            "method": "ping"
        }"#;

        let req: JsonRpcRequest = serde_json::from_str(raw).expect("valid request with string id");
        assert_eq!(req.id, Some(json!("abc-123")));
    }

    #[test]
    fn malformed_json_fails_to_parse() {
        let raw = r#"{ "jsonrpc": "2.0", "id": 1, "method": }"#;
        let result: Result<JsonRpcRequest, _> = serde_json::from_str(raw);
        assert!(result.is_err());
    }

    #[test]
    fn missing_method_field_fails() {
        let raw = r#"{ "jsonrpc": "2.0", "id": 1 }"#;
        let result: Result<JsonRpcRequest, _> = serde_json::from_str(raw);
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // JSON-RPC response parsing
    // ------------------------------------------------------------------

    #[test]
    fn parse_initialize_response() {
        let raw = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "listChanged": true, "subscribe": false }
                },
                "serverInfo": { "name": "wikiops", "version": "0.1.0" }
            }
        }"#;

        let resp: JsonRpcResponse = serde_json::from_str(raw).expect("valid response");
        assert_eq!(resp.jsonrpc, "2.0");
        assert_eq!(resp.id, Some(json!(1)));
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());

        let result = resp.result.unwrap();
        assert_eq!(result["serverInfo"]["name"], "wikiops");
    }

    #[test]
    fn parse_tool_result_response() {
        let raw = r#"{
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "content": [
                    {
                        "type": "text",
                        "text": "{\"slug\":\"wiki/index.md\"}"
                    }
                ],
                "isError": false
            }
        }"#;

        let resp: JsonRpcResponse = serde_json::from_str(raw).expect("valid tool response");
        let result = resp.result.unwrap();
        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["isError"], false);
    }

    #[test]
    fn parse_error_response() {
        let raw = r#"{
            "jsonrpc": "2.0",
            "id": 2,
            "error": {
                "code": -32602,
                "message": "Invalid params: missing 'slug'",
                "data": { "field": "slug" }
            }
        }"#;

        let resp: JsonRpcResponse = serde_json::from_str(raw).expect("valid error response");
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());

        let err = resp.error.unwrap();
        assert_eq!(err.code, -32602);
        assert_eq!(err.message, "Invalid params: missing 'slug'");
        assert_eq!(err.data, Some(json!({ "field": "slug" })));
    }

    #[test]
    fn serialize_error_response_omits_none_fields() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some(json!(99)),
            result: None,
            error: Some(JsonRpcError {
                code: JsonRpcError::METHOD_NOT_FOUND,
                message: "Method not found".into(),
                data: None,
            }),
        };

        let json = serde_json::to_string(&resp).expect("serializable");
        assert!(json.contains("\"error\""));
        assert!(!json.contains("\"result\""));
        // `data` should be omitted because it is None.
        assert!(!json.contains("\"data\""));
    }

    // ------------------------------------------------------------------
    // Error code constants
    // ------------------------------------------------------------------

    #[test]
    fn error_codes_are_standard() {
        assert_eq!(JsonRpcError::PARSE_ERROR, -32700);
        assert_eq!(JsonRpcError::INVALID_REQUEST, -32600);
        assert_eq!(JsonRpcError::METHOD_NOT_FOUND, -32601);
        assert_eq!(JsonRpcError::INVALID_PARAMS, -32602);
        assert_eq!(JsonRpcError::INTERNAL_ERROR, -32603);
    }

    // ------------------------------------------------------------------
    // MCP capability types
    // ------------------------------------------------------------------

    #[test]
    fn parse_capabilities() {
        let raw = r#"{
            "tools": { "listChanged": false },
            "resources": { "listChanged": true, "subscribe": false }
        }"#;

        let caps: McpCapabilities = serde_json::from_str(raw).expect("valid capabilities");
        assert_eq!(caps.tools.unwrap().list_changed, Some(false));
        assert_eq!(caps.resources.unwrap().list_changed, Some(true));
        assert!(caps.prompts.is_none());
    }

    #[test]
    fn serialize_capabilities_omits_none() {
        let caps = McpCapabilities {
            tools: Some(ToolsCapability {
                list_changed: Some(true),
            }),
            resources: None,
            prompts: None,
        };

        let json = serde_json::to_string(&caps).expect("serializable");
        assert!(json.contains("\"tools\""));
        assert!(!json.contains("\"resources\""));
        assert!(!json.contains("\"prompts\""));
    }

    // ------------------------------------------------------------------
    // Tool definition / result
    // ------------------------------------------------------------------

    #[test]
    fn parse_tool_definition() {
        let raw = r#"{
            "name": "brain_get",
            "description": "Retrieve a brain document by slug",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": { "type": "string" }
                },
                "required": ["slug"]
            }
        }"#;

        let def: ToolDefinition = serde_json::from_str(raw).expect("valid tool definition");
        assert_eq!(def.name, "brain_get");
        assert_eq!(def.description, "Retrieve a brain document by slug");
        assert_eq!(def.input_schema["type"], "object");
    }

    #[test]
    fn parse_tool_result() {
        let raw = r#"{
            "content": [
                { "type": "text", "text": "hello world" }
            ],
            "isError": false
        }"#;

        let tr: ToolResult = serde_json::from_str(raw).expect("valid tool result");
        assert_eq!(tr.content.len(), 1);
        assert_eq!(tr.content[0].content_type, "text");
        assert_eq!(tr.content[0].text, "hello world");
        assert_eq!(tr.is_error, Some(false));
    }

    #[test]
    fn serialize_tool_result_renames_is_error() {
        let tr = ToolResult {
            content: vec![ToolContent {
                content_type: "text".into(),
                text: "ok".into(),
            }],
            is_error: Some(true),
        };

        let json = serde_json::to_string(&tr).expect("serializable");
        assert!(json.contains("\"isError\""));
        assert!(!json.contains("\"is_error\""));
    }

    // ------------------------------------------------------------------
    // Resource / Prompt definitions
    // ------------------------------------------------------------------

    #[test]
    fn parse_resource_definition() {
        let raw = r#"{
            "uri": "brain://forge/wiki/index.md",
            "name": "Wiki Index",
            "mimeType": "text/markdown",
            "description": "Main wiki index"
        }"#;

        let res: ResourceDefinition = serde_json::from_str(raw).expect("valid resource");
        assert_eq!(res.uri, "brain://forge/wiki/index.md");
        assert_eq!(res.name, "Wiki Index");
        assert_eq!(res.mime_type, Some("text/markdown".into()));
        assert_eq!(res.description, Some("Main wiki index".into()));
    }

    #[test]
    fn parse_prompt_definition() {
        let raw = r#"{
            "name": "summarize",
            "description": "Summarize a document",
            "arguments": [
                { "name": "doc", "description": "Document to summarize", "required": true }
            ]
        }"#;

        let prompt: PromptDefinition = serde_json::from_str(raw).expect("valid prompt");
        assert_eq!(prompt.name, "summarize");
        assert_eq!(prompt.description, Some("Summarize a document".into()));

        let args = prompt.arguments.unwrap();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].name, "doc");
        assert_eq!(args[0].required, Some(true));
    }

    #[test]
    fn roundtrip_all_types() {
        // Ensure every public type can be serialized and deserialized without loss.
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(42)),
            method: "tools/list".into(),
            params: Some(json!({ "cursor": "abc" })),
        };
        let json = serde_json::to_string(&req).unwrap();
        let req2: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, req2);

        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some(json!(42)),
            result: Some(json!({ "tools": [] })),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let resp2: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, resp2);

        let err = JsonRpcError {
            code: JsonRpcError::INTERNAL_ERROR,
            message: "boom".into(),
            data: Some(json!({ "trace": "here" })),
        };
        let json = serde_json::to_string(&err).unwrap();
        let err2: JsonRpcError = serde_json::from_str(&json).unwrap();
        assert_eq!(err, err2);

        let caps = McpCapabilities {
            tools: Some(ToolsCapability {
                list_changed: Some(false),
            }),
            resources: Some(ResourcesCapability {
                list_changed: Some(true),
                subscribe: Some(false),
            }),
            prompts: Some(PromptsCapability {
                list_changed: Some(true),
            }),
        };
        let json = serde_json::to_string(&caps).unwrap();
        let caps2: McpCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(caps, caps2);
    }
}
