use std::{
    future::{Future, Ready, ready},
    num::NonZeroU32,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use actix_jwt_authc::Authenticated;
use actix_web::{
    HttpMessage, HttpResponse,
    body::EitherBody,
    dev::{HttpServiceFactory, Service, ServiceRequest, ServiceResponse, Transform},
    http::{Method, header},
    web,
};
use apalis_redis::RedisStorage;
use governor::{Quota, RateLimiter, clock::DefaultClock, state::keyed::DefaultKeyedStateStore};
use rmcp::{
    ErrorData, ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    service::{RequestContext, RoleServer},
    tool, tool_handler, tool_router,
    transport::streamable_http_server::session::local::LocalSessionManager,
};
use rmcp_actix_web::transport::StreamableHttpService;
use tokio::sync::RwLock;

use universal_inbox::user::UserId;

use crate::{
    jobs::UniversalInboxJob,
    mcp::tools::{
        ActOnNotificationArgs, BulkActNotificationsArgs, CreateTaskFromNotificationArgs,
        GetNotificationArgs, GetTaskArgs, ListNotificationsArgs, ListTasksArgs, McpServices,
        SearchTasksArgs, SyncNotificationsArgs, SyncTasksArgs, ToolCallError, UpdateTaskArgs,
        act_on_notification_input_schema, bulk_act_notifications_input_schema,
        create_task_from_notification_input_schema, execute_tool, get_notification_input_schema,
        get_task_input_schema, list_notifications_input_schema, list_tasks_input_schema,
        search_tasks_input_schema, sync_notifications_input_schema, sync_tasks_input_schema,
        update_task_input_schema,
    },
    universal_inbox::{notification::service::NotificationService, task::service::TaskService},
    utils::jwt::Claims,
};

pub mod tools;

const SERVER_NAME: &str = "universal-inbox";
const SERVER_TITLE: &str = "Universal Inbox";
const SERVER_INSTRUCTIONS: &str = "Authenticate with a Universal Inbox API key. Universal Inbox aggregates notifications from multiple sources (GitHub, Linear, Slack, Google Mail/Calendar/Drive) and manages tasks synchronized between task management tools (e.g. Todoist, Linear). Tasks accessible here are only those synchronized through Universal Inbox, not all tasks from the underlying providers. Read tools do not trigger synchronization unless trigger_sync is true. Write tools execute immediately.";
const MCP_RATE_LIMIT_PER_MINUTE: u32 = 120;
/// Protocol versions this server can negotiate.
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &["2025-06-18", "2025-03-26", "2024-11-05", "2025-11-25"];

pub type McpRateLimiter = RateLimiter<UserId, DefaultKeyedStateStore<UserId>, DefaultClock>;

/// Build the `StreamableHttpService` once so the `LocalSessionManager` is shared
/// across all Actix-web worker threads.  Call this **before** `HttpServer::new`
/// and clone the returned service into each worker via `scope()`.
pub fn build_http_service(
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    job_storage: RedisStorage<UniversalInboxJob>,
) -> StreamableHttpService<UniversalInboxMcpServer, LocalSessionManager> {
    let services = McpServices {
        notification_service,
        task_service,
        job_storage,
    };

    StreamableHttpService::builder()
        .service_factory(Arc::new(move || {
            Ok::<_, std::io::Error>(UniversalInboxMcpServer::new(services.clone()))
        }))
        .session_manager(Arc::new(LocalSessionManager::default()))
        .stateful_mode(true)
        .on_request_fn(|http_req, extensions| {
            if let Some(authenticated) = http_req.extensions().get::<Authenticated<Claims>>() {
                extensions.insert(authenticated.clone());
            }
        })
        .build()
}

pub fn build_rate_limiter() -> Arc<McpRateLimiter> {
    let quota = Quota::per_minute(
        NonZeroU32::new(MCP_RATE_LIMIT_PER_MINUTE).expect("rate limit must be non-zero"),
    );
    Arc::new(McpRateLimiter::keyed(quota))
}

pub fn scope(
    http_service: StreamableHttpService<UniversalInboxMcpServer, LocalSessionManager>,
    rate_limiter: Arc<McpRateLimiter>,
    resource_url: String,
    extra_allowed_origins: Vec<String>,
) -> impl HttpServiceFactory {
    // Well-known URLs are at the server root, not under the API path
    let server_origin = url::Url::parse(&resource_url)
        .map(|u| u.origin().ascii_serialization())
        .unwrap_or_else(|_| resource_url.clone());
    let resource_metadata_url = format!("{server_origin}/.well-known/oauth-protected-resource");
    let mut allowed_origins = vec![server_origin];
    allowed_origins.extend(extra_allowed_origins);

    web::scope("/mcp")
        .wrap(RequireAuthenticated {
            allowed_origins,
            rate_limiter,
            resource_metadata_url,
            resource_url,
        })
        .service(http_service.scope())
}

struct RequireAuthenticated {
    allowed_origins: Vec<String>,
    rate_limiter: Arc<McpRateLimiter>,
    resource_metadata_url: String,
    resource_url: String,
}

impl<S, B> Transform<S, ServiceRequest> for RequireAuthenticated
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = actix_web::Error;
    type InitError = ();
    type Transform = RequireAuthenticatedMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequireAuthenticatedMiddleware {
            service,
            allowed_origins: self.allowed_origins.clone(),
            rate_limiter: self.rate_limiter.clone(),
            resource_metadata_url: self.resource_metadata_url.clone(),
            resource_url: self.resource_url.clone(),
        }))
    }
}

struct RequireAuthenticatedMiddleware<S> {
    service: S,
    allowed_origins: Vec<String>,
    rate_limiter: Arc<McpRateLimiter>,
    resource_metadata_url: String,
    resource_url: String,
}

impl<S, B> Service<ServiceRequest> for RequireAuthenticatedMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, mut req: ServiceRequest) -> Self::Future {
        // MCP spec §Transports: "Servers MUST validate the Origin header on all
        // incoming requests to prevent DNS rebinding attacks. If the Origin header
        // is present and does not match the expected origin, servers MUST respond
        // with HTTP 403 Forbidden."
        if let Some(origin_value) = req.headers().get(header::ORIGIN) {
            let origin_str = origin_value.to_str().unwrap_or("");
            if !self.allowed_origins.iter().any(|o| o == origin_str) {
                let response = req
                    .into_response(HttpResponse::Forbidden().finish())
                    .map_into_right_body();
                return Box::pin(async move { Ok(response) });
            }
        }

        // Per the MCP spec, GET without a session ID must return 400 (not 401).
        // The rmcp library incorrectly returns 401 for this case, which causes
        // MCP clients (e.g. Claude Code) to misinterpret it as an auth failure
        // and enter a token-refresh loop instead of proceeding to POST initialize.
        if req.method() == Method::GET && req.headers().get("mcp-session-id").is_none() {
            let response = req
                .into_response(
                    HttpResponse::BadRequest()
                        .body("Bad Request: Mcp-Session-Id header is required for GET requests"),
                )
                .map_into_right_body();
            return Box::pin(async move { Ok(response) });
        }

        // MCP spec §Transports: "the client MUST include the MCP-Protocol-Version
        // header on all subsequent requests [...] If the server receives a request
        // with an invalid or unsupported MCP-Protocol-Version, it MUST respond with
        // 400 Bad Request." The header is not required on the initialize request
        // (which has no session ID yet).
        let unsupported_version = req
            .headers()
            .get("mcp-protocol-version")
            .and_then(|v| v.to_str().ok())
            .filter(|version| !SUPPORTED_PROTOCOL_VERSIONS.contains(version))
            .map(|v| v.to_string());
        if let Some(version) = unsupported_version {
            let response = req
                .into_response(HttpResponse::BadRequest().body(format!(
                    "Bad Request: Unsupported MCP-Protocol-Version: {version}"
                )))
                .map_into_right_body();
            return Box::pin(async move { Ok(response) });
        }

        let auth_result = req.extensions().get::<Authenticated<Claims>>().cloned();
        let has_authorization_header = req.headers().get(header::AUTHORIZATION).is_some();

        let resource_metadata_url = self.resource_metadata_url.clone();
        let authenticated = match auth_result {
            None => {
                let response = req
                    .into_response(
                        HttpResponse::Unauthorized()
                            .insert_header((
                                header::WWW_AUTHENTICATE,
                                format!("Bearer resource_metadata=\"{resource_metadata_url}\""),
                            ))
                            .finish(),
                    )
                    .map_into_right_body();
                return Box::pin(async move { Ok(response) });
            }
            Some(a) => a,
        };

        // Reject session-cookie authentication: MCP endpoints only accept Bearer
        // tokens. If Authenticated<Claims> came from a session cookie (no
        // Authorization header), reject to prevent CSRF — this makes the
        // permissive CORS origin policy safe.
        if !has_authorization_header {
            let response = req
                .into_response(
                    HttpResponse::Unauthorized()
                        .insert_header((
                            header::WWW_AUTHENTICATE,
                            format!("Bearer resource_metadata=\"{resource_metadata_url}\""),
                        ))
                        .finish(),
                )
                .map_into_right_body();
            return Box::pin(async move { Ok(response) });
        }

        // Validate audience for OAuth2 tokens (API key tokens without aud are still allowed)
        if let Some(ref aud) = authenticated.claims.aud {
            let expected_aud = &self.resource_url;
            if aud != expected_aud {
                let response = req
                    .into_response(HttpResponse::Forbidden().finish())
                    .map_into_right_body();
                return Box::pin(async move { Ok(response) });
            }
        }

        let user_id = authenticated.claims.sub.parse::<UserId>().ok();

        if let Some(uid) = user_id
            && self.rate_limiter.check_key(&uid).is_err()
        {
            let response = req
                .into_response(HttpResponse::TooManyRequests().finish())
                .map_into_right_body();
            return Box::pin(async move { Ok(response) });
        }

        req.headers_mut().remove(header::AUTHORIZATION);
        let future = self.service.call(req);
        Box::pin(async move { future.await.map(ServiceResponse::map_into_left_body) })
    }
}

#[derive(Clone)]
pub struct UniversalInboxMcpServer {
    services: McpServices,
    tool_router: ToolRouter<Self>,
}

impl UniversalInboxMcpServer {
    fn new(services: McpServices) -> Self {
        Self {
            services,
            tool_router: Self::tool_router(),
        }
    }

    fn user_id_from_context(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<UserId, ErrorData> {
        let authenticated = context
            .extensions
            .get::<Authenticated<Claims>>()
            .ok_or_else(|| ErrorData::invalid_request("Missing authenticated user", None))?;

        authenticated
            .claims
            .sub
            .parse::<UserId>()
            .map_err(|_| ErrorData::invalid_request("Invalid authenticated user", None))
    }

    async fn call_structured_tool<T: serde::Serialize>(
        &self,
        tool_name: &str,
        args: T,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let user_id = self.user_id_from_context(&context)?;
        let arguments = serde_json::to_value(args).map(Some).map_err(|err| {
            ErrorData::invalid_params(format!("Failed to serialize tool arguments: {err}"), None)
        })?;

        match execute_tool(tool_name, arguments, &self.services, user_id).await {
            Ok(result) => Ok(CallToolResult::structured(result)),
            Err(ToolCallError::InvalidArguments(err)) => {
                Err(ErrorData::invalid_params(err.to_string(), None))
            }
            Err(ToolCallError::Execution(err)) => {
                Ok(CallToolResult::error(vec![Content::text(err.to_string())]))
            }
            Err(ToolCallError::UnknownTool(tool_name)) => Err(ErrorData::invalid_params(
                format!("Unknown tool: {tool_name}"),
                None,
            )),
        }
    }
}

#[tool_router]
impl UniversalInboxMcpServer {
    #[tool(
        name = "list_notifications",
        title = "List notifications",
        description = "List Universal Inbox notifications without implicitly triggering synchronization.",
        input_schema = list_notifications_input_schema(),
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn list_notifications(
        &self,
        Parameters(args): Parameters<ListNotificationsArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call_structured_tool("list_notifications", args, context)
            .await
    }

    #[tool(
        name = "get_notification",
        title = "Get notification",
        description = "Fetch a single Universal Inbox notification.",
        input_schema = get_notification_input_schema(),
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_notification(
        &self,
        Parameters(args): Parameters<GetNotificationArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call_structured_tool("get_notification", args, context)
            .await
    }

    #[tool(
        name = "act_on_notification",
        title = "Act on notification",
        description = "Apply a single notification action. Write operations execute immediately.",
        input_schema = act_on_notification_input_schema(),
        annotations(destructive_hint = true)
    )]
    async fn act_on_notification(
        &self,
        Parameters(args): Parameters<ActOnNotificationArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call_structured_tool("act_on_notification", args, context)
            .await
    }

    #[tool(
        name = "bulk_act_notifications",
        title = "Bulk act on notifications",
        description = "Apply the same action to all matching notifications. Empty status/source filters match all notifications.",
        input_schema = bulk_act_notifications_input_schema(),
        annotations(destructive_hint = true)
    )]
    async fn bulk_act_notifications(
        &self,
        Parameters(args): Parameters<BulkActNotificationsArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call_structured_tool("bulk_act_notifications", args, context)
            .await
    }

    #[tool(
        name = "create_task_from_notification",
        title = "Create task from notification",
        description = "Create a task from a notification and link the two together.",
        input_schema = create_task_from_notification_input_schema(),
        annotations(destructive_hint = true)
    )]
    async fn create_task_from_notification(
        &self,
        Parameters(args): Parameters<CreateTaskFromNotificationArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call_structured_tool("create_task_from_notification", args, context)
            .await
    }

    #[tool(
        name = "sync_notifications",
        title = "Synchronize notifications",
        description = "Synchronize notification sources immediately and return the resulting notifications.",
        input_schema = sync_notifications_input_schema(),
        annotations(destructive_hint = true)
    )]
    async fn sync_notifications(
        &self,
        Parameters(args): Parameters<SyncNotificationsArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call_structured_tool("sync_notifications", args, context)
            .await
    }

    #[tool(
        name = "list_tasks",
        title = "List tasks",
        description = "List tasks synchronized through Universal Inbox (not all tasks from underlying providers like Todoist). Does not trigger synchronization unless trigger_sync is true.",
        input_schema = list_tasks_input_schema(),
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn list_tasks(
        &self,
        Parameters(args): Parameters<ListTasksArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call_structured_tool("list_tasks", args, context).await
    }

    #[tool(
        name = "get_task",
        title = "Get task",
        description = "Fetch a single task synchronized through Universal Inbox.",
        input_schema = get_task_input_schema(),
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_task(
        &self,
        Parameters(args): Parameters<GetTaskArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call_structured_tool("get_task", args, context).await
    }

    #[tool(
        name = "search_tasks",
        title = "Search tasks",
        description = "Search tasks synchronized through Universal Inbox by text.",
        input_schema = search_tasks_input_schema(),
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn search_tasks(
        &self,
        Parameters(args): Parameters<SearchTasksArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call_structured_tool("search_tasks", args, context)
            .await
    }

    #[tool(
        name = "update_task",
        title = "Update task",
        description = "Patch an existing task synchronized through Universal Inbox. Write operations execute immediately.",
        input_schema = update_task_input_schema(),
        annotations(destructive_hint = true)
    )]
    async fn update_task(
        &self,
        Parameters(args): Parameters<UpdateTaskArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call_structured_tool("update_task", args, context)
            .await
    }

    #[tool(
        name = "sync_tasks",
        title = "Synchronize tasks",
        description = "Synchronize task sources immediately and return the resulting tasks. Only synchronizes tasks tracked by Universal Inbox, not all tasks from the provider.",
        input_schema = sync_tasks_input_schema(),
        annotations(destructive_hint = true)
    )]
    async fn sync_tasks(
        &self,
        Parameters(args): Parameters<SyncTasksArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call_structured_tool("sync_tasks", args, context).await
    }
}

#[tool_handler]
impl ServerHandler for UniversalInboxMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2025_06_18)
            .with_server_info(
                Implementation::new(SERVER_NAME, env!("CARGO_PKG_VERSION"))
                    .with_title(SERVER_TITLE.to_string())
                    .with_description(SERVER_INSTRUCTIONS.to_string()),
            )
            .with_instructions(SERVER_INSTRUCTIONS)
    }
}
