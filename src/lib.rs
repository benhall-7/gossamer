use std::str::FromStr;
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use bytes::Bytes;
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::body::Incoming;
use hyper::http::request::Parts;
use hyper::http::{Method, StatusCode};
use hyper::{HeaderMap, Request, Response, Uri};
use serde::de;

pub mod openapi;

pub use hyper;

pub type HttpResponse = Response<BoxBody<Bytes, hyper::Error>>;

#[derive(Debug)]
pub struct GossamerResponse(pub HttpResponse);

impl From<HttpResponse> for GossamerResponse {
    fn from(resp: HttpResponse) -> Self {
        Self(resp)
    }
}

impl From<&'static str> for GossamerResponse {
    fn from(s: &'static str) -> Self {
        GossamerResponse(text_response(StatusCode::OK, s))
    }
}

impl From<String> for GossamerResponse {
    fn from(s: String) -> Self {
        GossamerResponse(text_response(StatusCode::OK, s))
    }
}

impl From<(StatusCode, &'static str)> for GossamerResponse {
    fn from((st, s): (StatusCode, &'static str)) -> Self {
        GossamerResponse(text_response(st, s))
    }
}

impl From<(StatusCode, String)> for GossamerResponse {
    fn from((st, s): (StatusCode, String)) -> Self {
        GossamerResponse(text_response(st, s))
    }
}

#[derive(Debug)]
pub struct Error {
    pub status: StatusCode,
    pub message: String,
}

impl Error {
    pub fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: "not found".into(),
        }
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }
}

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

pub type Handler<S> = Arc<dyn Fn(Ctx<S>) -> BoxFuture<Result<HttpResponse, Error>> + Send + Sync>;

#[derive(Clone, Debug)]
pub struct RouteParams {
    inner: HashMap<String, String>,
}

impl RouteParams {
    fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.inner.get(name).map(|s| s.as_str())
    }

    pub fn require(&self, name: &str) -> Result<&str, Error> {
        self.get(name).ok_or_else(|| Error::bad_request(format!("missing path param: {name}")))
    }

    pub fn parse<T: FromStr>(&self, name: &str) -> Result<T, Error> {
        let s = self.require(name)?;
        s.parse::<T>().map_err(|_| {
            Error::bad_request(format!("invalid path param {name}: {s}"))
        })
    }
}

#[derive(Debug, Clone)]
pub struct RouteInfo {
    pub method: Method,
    /// Canonical route pattern as registered, e.g. "/echo/{phrase}"
    pub pattern: String,
    /// Path parameter names extracted from the pattern, in order.
    pub path_params: Vec<String>,
}

impl RouteInfo {
    fn from_compiled(method: Method, compiled: &CompiledRoute) -> Self {
        Self {
            method,
            pattern: compiled.pattern.clone(),
            path_params: compiled.param_names.clone(),
        }
    }

    pub fn has_path_param(&self, name: &str) -> bool {
        self.path_params.iter().any(|p| p == name)
    }
}

#[derive(Debug)]
enum RequestBody {
    Unread(Incoming),
    Buffered(Bytes),
    Taken,
}

#[derive(Debug)]
pub struct Ctx<S> {
    parts: Parts,
    body: RequestBody, // contains Incoming or buffered bytes
    state: Arc<S>,
    params: RouteParams,
}

impl<S> Ctx<S> {
    pub fn state(&self) -> Arc<S> {
        self.state.clone()
    }
    pub fn method(&self) -> &Method {
        &self.parts.method
    }
    pub fn uri(&self) -> &Uri {
        &self.parts.uri
    }
    pub fn headers(&self) -> &HeaderMap {
        &self.parts.headers
    }
    pub fn query_raw(&self) -> Option<&str> {
        self.parts.uri.query()
    }

    pub fn params(&self) -> &RouteParams {
        &self.params
    }

    pub fn path(&self) -> &str {
        self.parts.uri.path()
    }

    pub fn query<T: de::DeserializeOwned>(&self) -> Result<T, Error> {
        let q = self.query_raw().unwrap_or("");
        serde_urlencoded::from_str(q).map_err(|e| Error::bad_request(format!("invalid query: {e}")))
    }

    pub fn take_body(&mut self) -> Result<Incoming, Error> {
        match std::mem::replace(&mut self.body, RequestBody::Taken) {
            RequestBody::Unread(b) => Ok(b),
            RequestBody::Buffered(_) => Err(Error::bad_request("body already buffered")),
            RequestBody::Taken => Err(Error::bad_request("body already taken")),
        }
    }

    pub async fn bytes(&mut self) -> Result<Bytes, Error> {
        match &self.body {
            RequestBody::Buffered(b) => return Ok(b.clone()),
            RequestBody::Taken => return Err(Error::bad_request("body already taken")),
            RequestBody::Unread(_) => {}
        }

        let incoming = match std::mem::replace(&mut self.body, RequestBody::Taken) {
            RequestBody::Unread(b) => b,
            _ => unreachable!(),
        };

        let collected = incoming
            .collect()
            .await
            .map_err(|e| Error::bad_request(format!("failed to read body: {e:?}")))?;
        let b = collected.to_bytes();

        self.body = RequestBody::Buffered(b.clone());
        Ok(b)
    }

    pub async fn json<T: de::DeserializeOwned>(&mut self) -> Result<T, Error> {
        let b: Vec<u8> = self.bytes().await?.into_iter().collect();
        serde_json::from_slice(&b).map_err(|e| Error::bad_request(format!("invalid json: {e}")))
    }
}

#[derive(Clone, Debug)]
enum Segment {
    Lit(String),
    Param { name: String },
}

#[derive(Clone, Debug)]
struct CompiledRoute {
    pattern: String,
    segments: Vec<Segment>,
    param_names: Vec<String>,
    is_static: bool,
}

impl CompiledRoute {
    fn compile(pattern: &str) -> Result<Self, Error> {
        if !pattern.starts_with('/') {
            return Err(Error::bad_request("route patterns must start with '/'"));
        }

        let split_segments = pattern.split('/').filter(|s| !s.is_empty());
        let mut segments = Vec::new();
        let mut param_names = Vec::new();
        let mut is_static = true;

        for seg in split_segments {
            if let Some(name) = seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                if name.is_empty() {
                    return Err(Error::bad_request("empty param name in route pattern"));
                }
                is_static = false;
                param_names.push(name.to_string());
                segments.push(Segment::Param {
                    name: name.to_string(),
                });
            } else {
                segments.push(Segment::Lit(seg.to_string()));
            }
        }

        Ok(Self {
            pattern: pattern.to_string(),
            segments,
            param_names,
            is_static,
        })
    }

    fn try_match(&self, path: &str) -> Option<RouteParams> {
        let split_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if split_segments.len() != self.segments.len() {
            return None;
        }

        let mut params = RouteParams::new();

        for (seg, actual) in self.segments.iter().zip(split_segments) {
            match seg {
                Segment::Lit(expected) => {
                    if expected != actual {
                        return None;
                    }
                }
                Segment::Param { name } => {
                    params.inner.insert(name.clone(), actual.to_string());
                }
            }
        }

        Some(params)
    }
}

pub trait RouteMeta {
    fn on_route(&mut self, info: &RouteInfo);
}

// Default no-op implementation
impl RouteMeta for () {
    fn on_route(&mut self, _: &RouteInfo) {}
}


struct Route<S> {
    method: Method,
    compiled: CompiledRoute,
    handler: Handler<S>,
}

pub struct RouterBuilder<S, M = ()> {
    state: Arc<S>,
    meta: M,

    static_routes: HashMap<(Method, String), Handler<S>>,
    param_routes: Vec<Route<S>>,
}

impl<S> RouterBuilder<S, ()> {
    pub fn new(state: S) -> Self {
        Self {
            state: Arc::new(state),
            meta: (),
            static_routes: HashMap::new(),
            param_routes: Vec::new(),
        }
    }
}

impl<S, M> RouterBuilder<S, M> {
    pub fn with_meta<N>(self, meta: N) -> RouterBuilder<S, N> {
        RouterBuilder {
            state: self.state,
            meta,
            static_routes: self.static_routes,
            param_routes: self.param_routes,
        }
    }

    pub fn finish(self) -> (Router<S>, M) {
        let router = Router {
            state: self.state,
            static_routes: self.static_routes,
            param_routes: self.param_routes,
        };
        (router, self.meta)
    }
}

impl<S, M> RouterBuilder<S, M>
where
    S: Send + Sync + 'static,
    M: RouteMeta,
{
    pub fn get<H, Fut, R>(self, pattern: &str, handler: H) -> Self
    where
        H: Fn(Ctx<S>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<R, Error>> + Send + 'static,
        R: Into<GossamerResponse> + 'static,
    {
        self.route(Method::GET, pattern, handler)
    }

    pub fn post<H, Fut, R>(self, pattern: &str, handler: H) -> Self
    where
        H: Fn(Ctx<S>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<R, Error>> + Send + 'static,
        R: Into<GossamerResponse> + 'static,
    {
        self.route(Method::POST, pattern, handler)
    }

    pub fn put<H, Fut, R>(self, pattern: &str, handler: H) -> Self
    where
        H: Fn(Ctx<S>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<R, Error>> + Send + 'static,
        R: Into<GossamerResponse> + 'static,
    {
        self.route(Method::PUT, pattern, handler)
    }

    pub fn delete<H, Fut, R>(self, pattern: &str, handler: H) -> Self
    where
        H: Fn(Ctx<S>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<R, Error>> + Send + 'static,
        R: Into<GossamerResponse> + 'static,
    {
        self.route(Method::DELETE, pattern, handler)
    }

    pub fn route<H, Fut, R>(mut self, method: Method, pattern: &str, handler: H) -> Self
    where
        H: Fn(Ctx<S>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<R, Error>> + Send + 'static,
        R: Into<GossamerResponse> + 'static,
    {
        let compiled = CompiledRoute::compile(pattern)
            .expect("invalid route pattern");

        // Build RouteInfo once, derived from compiled route.
        let info = RouteInfo::from_compiled(method.clone(), &compiled);

        // Fold meta
        self.meta.on_route(&info);

        // Erase handler to your stored Handler<S>
        let erased: Handler<S> = Arc::new(move |ctx: Ctx<S>| {
            let fut = handler(ctx);
            Box::pin(async move {
                let out: R = fut.await?;
                Ok(out.into().0) // R -> GossamerResponse -> HttpResponse
            })
        });

        if compiled.is_static {
            self.static_routes.insert((method, compiled.pattern.clone()), erased);
        } else {
            self.param_routes.push(Route { method, compiled, handler: erased });
        }

        self
    }
}

pub struct Router<S> {
    state: Arc<S>,
    static_routes: HashMap<(Method, String), Handler<S>>,
    param_routes: Vec<Route<S>>,
}

impl<S: Send + Sync + 'static> Router<S> {
    pub async fn handle(&self, req: Request<Incoming>) -> HttpResponse {
        let method = req.method().clone();
        let path = req.uri().path().to_string();

        if let Some(h) = self.static_routes.get(&(method.clone(), path.clone())) {
            let (parts, body) = req.into_parts();
            let ctx = Ctx {
                parts,
                state: self.state.clone(),
                params: RouteParams::new(),
                body: RequestBody::Unread(body),
            };
            return match (h)(ctx).await {
                Ok(r) => r,
                Err(e) => error_response(e),
            };
        }

        for r in &self.param_routes {
            if r.method != method {
                continue;
            }
            if let Some(params) = r.compiled.try_match(&path) {
                let (parts, body) = req.into_parts();
                let ctx = Ctx {
                    parts,
                    state: self.state.clone(),
                    params,
                    body: RequestBody::Unread(body),
                };
                return match (r.handler)(ctx).await {
                    Ok(resp) => resp,
                    Err(e) => error_response(e),
                };
            }
        }

        error_response(Error::not_found())
    }
}

// ---- response helpers ----

fn boxed_full(bytes: Bytes) -> BoxBody<Bytes, hyper::Error> {
    Full::new(bytes).map_err(|never| match never {}).boxed()
}

pub fn text_response(status: StatusCode, s: impl Into<String>) -> HttpResponse {
    let mut r = Response::new(boxed_full(Bytes::from(s.into())));
    *r.status_mut() = status;
    r
}

fn error_response(e: Error) -> HttpResponse {
    text_response(e.status, e.message)
}
