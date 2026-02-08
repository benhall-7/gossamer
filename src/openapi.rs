use crate::{MetaBuilder, RouteInfo};

use hyper::http::Method;
use serde::Serialize;
use std::collections::BTreeMap;

pub use serde_json;

#[derive(Debug)]
pub struct OpenApiAcc {
    pub doc: OpenApiDoc,
}

impl OpenApiAcc {
    pub fn new(title: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            doc: OpenApiDoc {
                openapi: "3.0.3".to_string(),
                info: Info {
                    title: title.into(),
                    version: version.into(),
                },
                paths: BTreeMap::new(),
            },
        }
    }

    pub fn to_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.doc)
    }
}

impl MetaBuilder for OpenApiAcc {
    type Finish = OpenApiDoc;

    fn on_route(&mut self, info: &RouteInfo) {
        let path = info.pattern.clone(); // already canonical: "/echo/{phrase}"

        let method = match info.method {
            Method::GET => HttpMethod::Get,
            Method::POST => HttpMethod::Post,
            Method::PUT => HttpMethod::Put,
            Method::PATCH => HttpMethod::Patch,
            Method::DELETE => HttpMethod::Delete,
            Method::OPTIONS => HttpMethod::Options,
            Method::HEAD => HttpMethod::Head,
            _ => return, // or map extension methods
        };

        let mut op = Operation::default();

        // Add path params as required string params (MVP default)
        for p in &info.path_params {
            op.parameters.push(Parameter {
                name: p.clone(),
                location: ParamLocation::Path,
                required: true,
                description: None,
                schema: Schema::String,
            });
        }

        // Add a default 200 response so the spec is valid
        op.responses.insert(
            "200".to_string(),
            Response {
                description: "OK".to_string(),
                content: BTreeMap::new(),
            },
        );

        self.doc
            .paths
            .entry(path)
            .or_default()
            .set_operation(method, op);
    }

    fn on_finish(self) -> OpenApiDoc {
        self.doc
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenApiDoc {
    // "3.0.3" or "3.1.0"
    pub openapi: String,
    pub info: Info,
    pub paths: BTreeMap<String, PathItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Info {
    pub title: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PathItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<Operation>,
}

impl PathItem {
    pub fn set_operation(&mut self, method: HttpMethod, op: Operation) {
        match method {
            HttpMethod::Get => self.get = Some(op),
            HttpMethod::Post => self.post = Some(op),
            HttpMethod::Put => self.put = Some(op),
            HttpMethod::Patch => self.patch = Some(op),
            HttpMethod::Delete => self.delete = Some(op),
            HttpMethod::Options => self.options = Some(op),
            HttpMethod::Head => self.head = Some(op),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum HttpMethod {
    #[serde(rename = "get")]
    Get,
    #[serde(rename = "post")]
    Post,
    #[serde(rename = "put")]
    Put,
    #[serde(rename = "patch")]
    Patch,
    #[serde(rename = "delete")]
    Delete,
    #[serde(rename = "options")]
    Options,
    #[serde(rename = "head")]
    Head,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Operation {
    pub operation_id: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub parameters: Vec<Parameter>,
    pub request_body: Option<RequestBody>,
    // "200", "default", etc.
    pub responses: BTreeMap<String, Response>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Parameter {
    pub name: String,
    // path/query
    pub location: ParamLocation,
    pub required: bool,
    pub description: Option<String>,
    pub schema: Schema,
}

#[derive(Debug, Clone, Serialize)]
pub enum ParamLocation {
    Path,
    Query,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestBody {
    pub required: bool,
    // "application/json"
    pub content: BTreeMap<String, MediaType>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub description: String,
    pub content: BTreeMap<String, MediaType>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaType {
    pub schema: Schema,
}

#[derive(Debug, Clone, Serialize)]
pub enum Schema {
    // MVP: primitives + “unknown”
    Any,
    String,
    Integer { format: Option<IntegerFormat> },
    Number { format: Option<NumberFormat> },
    Boolean,
    Array(Box<Schema>),
    // placeholder; later becomes properties map / refs
    Object,
}

#[derive(Debug, Clone, Serialize)]
pub enum IntegerFormat {
    Int32,
    Int64,
}

#[derive(Debug, Clone, Serialize)]
pub enum NumberFormat {
    Float,
    Double,
}
