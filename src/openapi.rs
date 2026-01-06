use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct OpenApiDoc {
    // "3.0.3" or "3.1.0"
    pub openapi: String,
    pub info: Info,
    pub paths: BTreeMap<String, PathItem>,
}

#[derive(Debug, Clone)]
pub struct Info {
    pub title: String,
    pub version: String,
}

#[derive(Debug, Clone, Default)]
pub struct PathItem {
    pub operations: BTreeMap<HttpMethod, Operation>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Head,
}

#[derive(Debug, Clone, Default)]
pub struct Operation {
    pub operation_id: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub parameters: Vec<Parameter>,
    pub request_body: Option<RequestBody>,
    // "200", "default", etc.
    pub responses: BTreeMap<String, Response>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    // path/query
    pub location: ParamLocation,
    pub required: bool,
    pub description: Option<String>,
    pub schema: Schema,
}

#[derive(Debug, Clone)]
pub enum ParamLocation {
    Path,
    Query,
}

#[derive(Debug, Clone)]
pub struct RequestBody {
    pub required: bool,
    // "application/json"
    pub content: BTreeMap<String, MediaType>,
}

#[derive(Debug, Clone)]
pub struct Response {
    pub description: String,
    pub content: BTreeMap<String, MediaType>,
}

#[derive(Debug, Clone)]
pub struct MediaType {
    pub schema: Schema,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub enum IntegerFormat {
    Int32,
    Int64,
}

#[derive(Debug, Clone)]
pub enum NumberFormat {
    Float,
    Double,
}
