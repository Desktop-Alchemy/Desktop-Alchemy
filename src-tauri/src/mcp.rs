//! Local MCP transport. Hardware always goes through the existing Core and SDK owner.
use crate::{core::Core, model::HardwareCommand, storage};
use anyhow::{bail, Context, Result};
use axum::{
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::IntoResponse,
    Router,
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{
    net::TcpListener,
    sync::{Mutex, RwLock},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use crate::management::*;

const OWNER: &str = "@mcp"; // Cannot collide with a valid plugin manifest ID.
const DEFAULT_PORT: u16 = 18718;
const MAX_BODY: usize = 32 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    enabled: bool,
    port: u16,
    token: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_PORT,
            token: String::new(),
        }
    }
}

/// Intentionally excludes credentials, SDK paths, and the full Core snapshot.
#[derive(Clone, Serialize)]
pub struct McpStatus {
    pub enabled: bool,
    pub port: u16,
    pub endpoint: String,
    pub status: String,
    pub error: Option<String>,
}
struct Running {
    access: Arc<RwLock<bool>>,
    stop: CancellationToken,
    task: JoinHandle<std::io::Result<()>>,
}
struct Runtime {
    config: Config,
    running: Option<Running>,
    error: Option<String>,
}
pub struct McpManager {
    core: Arc<Core>,
    services: Arc<Services>,
    operations: Arc<Operations>,
    runtime: Mutex<Runtime>,
}

impl McpManager {
    pub fn new(core: Arc<Core>) -> Arc<Self> {
        Self::with_services(Services::headless(core))
    }
    pub fn with_services(services: Arc<Services>) -> Arc<Self> {
        let core = services.core.clone();
        let operations = Operations::load(&core.root);
        let loaded = (|| -> Result<Config> {
            let path = core.root.join("mcp.json");
            if !path.exists() {
                return Ok(Config::default());
            }
            let c: Config = serde_json::from_slice(&std::fs::read(path)?)?;
            if c.port == 0
                || (!c.token.is_empty()
                    && (c.token.len() != 64 || !c.token.bytes().all(|b| b.is_ascii_hexdigit())))
                || (c.enabled && c.token.is_empty())
            {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b098",
                    "MCP 配置无效，原文件已保留"
                ))
            }
            Ok(c)
        })();
        let (config, error) = match loaded {
            Ok(c) => (c, None),
            Err(_) => (
                Config::default(),
                Some("MCP 配置无法读取，原文件已保留；请重新设置".into()),
            ),
        };
        Arc::new(Self {
            core,
            services,
            operations,
            runtime: Mutex::new(Runtime {
                config,
                running: None,
                error,
            }),
        })
    }
    pub async fn status(&self) -> McpStatus {
        let r = self.runtime.lock().await;
        let error = r.error.clone().or_else(|| {
            r.running
                .as_ref()
                .filter(|s| s.task.is_finished())
                .map(|_| "MCP 服务已意外停止，请重新启用".into())
        });
        McpStatus {
            enabled: r.config.enabled,
            port: r.config.port,
            endpoint: format!("http://127.0.0.1:{}/mcp", r.config.port),
            status: if error.is_some() {
                "error"
            } else if r.running.is_some() {
                "running"
            } else {
                "stopped"
            }
            .into(),
            error,
        }
    }
    pub async fn start_saved(&self) {
        let mut r = self.runtime.lock().await;
        if r.config.enabled {
            if let Err(e) = self.start_locked(&mut r).await {
                r.error = Some(e.to_string());
            }
        }
    }
    async fn start_locked(&self, r: &mut Runtime) -> Result<()> {
        if r.running.is_some() {
            return Ok(());
        }
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, r.config.port))
            .await
            .with_context(|| format!("无法监听 MCP 端口 {}，请检查端口占用", r.config.port))?;
        let access = Arc::new(RwLock::new(true));
        let stop = CancellationToken::new();
        let app = router(
            self.services.clone(),
            self.operations.clone(),
            access.clone(),
            &r.config,
            stop.clone(),
        );
        let shutdown = stop.clone();
        let task = tokio::spawn(serve_http(listener, app, shutdown, self.operations.clone()));
        r.running = Some(Running { access, stop, task });
        r.error = None;
        Ok(())
    }
    async fn stop_locked(&self, r: &mut Runtime) -> Result<()> {
        let Some(mut running) = r.running.take() else {
            return Ok(());
        };
        // Writers wait for active tool calls, and queued/new readers see false.
        *running.access.write().await = false;
        running.stop.cancel();
        let ended = match tokio::time::timeout(Duration::from_secs(3), &mut running.task).await {
            Ok(result) => result
                .context(alchemy_device_api::UiFailure::new(
                    "backend.b099",
                    "MCP 服务任务异常",
                ))
                .and_then(|result| {
                    result.context(alchemy_device_api::UiFailure::new(
                        "backend.b100",
                        "MCP 服务关闭失败",
                    ))
                }),
            Err(_) => {
                running.task.abort();
                let _ = running.task.await;
                Ok(())
            }
        };
        // Includes cancellation of queued queries and restoration of GIF-only overrides.
        let restored = self.core.restore_plugin(OWNER).await;
        ended.and(restored)
    }
    pub async fn configure(&self, enabled: bool, port: u16) -> Result<McpStatus> {
        if port == 0 {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b101",
                "MCP 端口须为 1–65535"
            ));
        }
        let mut r = self.runtime.lock().await;
        self.stop_locked(&mut r).await?;
        let mut config = r.config.clone();
        config.enabled = enabled;
        config.port = port;
        if enabled && config.token.is_empty() {
            config.token = new_token();
        }
        storage::save_json(&self.core.root, "mcp.json", &config)?;
        r.config = config;
        r.error = None;
        if enabled {
            if let Err(e) = self.start_locked(&mut r).await {
                r.error = Some(e.to_string());
            }
        }
        drop(r);
        Ok(self.status().await)
    }
    pub async fn token(&self) -> Result<String> {
        let r = self.runtime.lock().await;
        if r.config.token.is_empty() {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b102",
                "请先启用 MCP 以生成令牌"
            ));
        }
        Ok(r.config.token.clone())
    }
    pub async fn reset_token(&self) -> Result<McpStatus> {
        let mut r = self.runtime.lock().await;
        self.stop_locked(&mut r).await?;
        let mut config = r.config.clone();
        config.token = new_token();
        storage::save_json(&self.core.root, "mcp.json", &config)?;
        r.config = config;
        r.error = None;
        if r.config.enabled {
            if let Err(e) = self.start_locked(&mut r).await {
                r.error = Some(e.to_string());
            }
        }
        drop(r);
        Ok(self.status().await)
    }
    pub async fn shutdown(&self) -> Result<()> {
        self.stop_locked(&mut *self.runtime.lock().await).await
    }
}

/// Gracefully close short HTTP connections before releasing the Windows socket.
/// Immediate drop after shutdown could reset the connection before the client read
/// its response. Axum's convenience server does not expose this final peer drain.
async fn serve_http(
    listener: TcpListener,
    app: Router,
    stop: CancellationToken,
    operations: Arc<Operations>,
) -> std::io::Result<()> {
    use hyper_util::{
        rt::{TokioIo, TokioTimer},
        service::TowerToHyperService,
    };
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            biased;
            _ = stop.cancelled() => break,
            incoming = listener.accept() => {
                let (socket, _) = incoming?;
                socket.set_nodelay(true)?;
                let operations = operations.clone();
                let delivered = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let app = app.clone().layer(axum::Extension(ResponseDelivery(delivered.clone())));
                let cancel = stop.clone();
                connections.spawn(async move {
                    let mut builder = hyper::server::conn::http1::Builder::new();
                    builder.half_close(true).timer(TokioTimer::new()).header_read_timeout(Duration::from_secs(30));
                    let mut connection = builder.serve_connection(TokioIo::new(socket), TowerToHyperService::new(app));
                    let result = tokio::select! {
                        result = std::future::poll_fn(|cx| connection.poll_without_shutdown(cx)) => result,
                        _ = cancel.cancelled() => {
                            std::pin::Pin::new(&mut connection).graceful_shutdown();
                            std::future::poll_fn(|cx| connection.poll_without_shutdown(cx)).await
                        }
                    };
                    if result.is_ok() {
                        use tokio::io::{AsyncReadExt, AsyncWriteExt};
                        let mut socket = connection.into_parts().io.into_inner();
                        let _ = socket.shutdown().await;
                        if delivered.load(std::sync::atomic::Ordering::SeqCst) { operations.response_finished(); }
                        // Let pending Windows overlapped sends complete before a peer
                        // that already half-closed can make the read return EOF immediately.
                        #[cfg(windows)]
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        let _ = tokio::time::timeout(Duration::from_secs(2), async {
                            let mut buffer = [0u8; 1024];
                            while socket.read(&mut buffer).await? != 0 {}
                            Ok::<_, std::io::Error>(())
                        }).await;
                    }
                });
            },
            _ = connections.join_next(), if !connections.is_empty() => {}
        }
    }
    drop(listener);
    while connections.join_next().await.is_some() {}
    Ok(())
}
fn new_token() -> String {
    // Two independently generated UUIDs provide 244 random bits, encoded as 64 hex digits.
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

fn router(
    services: Arc<Services>,
    operations: Arc<Operations>,
    access: Arc<RwLock<bool>>,
    config: &Config,
    stop: CancellationToken,
) -> Router {
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager,
        tower::{StreamableHttpServerConfig, StreamableHttpService},
    };
    let transport_config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_cancellation_token(stop)
        .with_allowed_hosts([format!("127.0.0.1:{}", config.port)])
        .with_allowed_origins([format!("http://127.0.0.1:{}", config.port)])
        .with_max_request_body_bytes(MAX_BODY);
    let response_operations = operations.clone();
    let service = StreamableHttpService::new(
        move || {
            Ok(AlchemyServer::with_services(
                services.clone(),
                operations.clone(),
                access.clone(),
            ))
        },
        Arc::new(LocalSessionManager::default()),
        transport_config,
    );
    let token = Arc::new(config.token.clone());
    Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn(move |request: Request, next: Next| {
            let token = token.clone();
            let operations = response_operations.clone();
            let delivery = request.extensions().get::<ResponseDelivery>().cloned();
            async move {
                let authorized = request.headers().get_all("authorization").iter().count() == 1
                    && request
                        .headers()
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.strip_prefix("Bearer "))
                        .is_some_and(|s| constant_time_eq(s, &token));
                if !authorized {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                match tokio::time::timeout(Duration::from_secs(30), next.run(request)).await {
                    Ok(mut response) => {
                        if response.status().is_success() && operations.restart_pending() {
                            response
                                .headers_mut()
                                .insert("connection", "close".parse().unwrap());
                            if let Some(delivery) = delivery {
                                delivery.0.store(true, std::sync::atomic::Ordering::SeqCst);
                            }
                        }
                        response
                    }
                    Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
                }
            }
        }))
}

#[derive(Clone)]
struct ResponseDelivery(Arc<std::sync::atomic::AtomicBool>);
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |diff, (x, y)| diff | (x ^ y))
        == 0
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ImportAsset {
    /// Absolute local file path; choose exactly one of path or data. No URLs or network/device paths.
    path: Option<String>,
    /// Base64 image bytes or a data URL. PNG/JPEG/GIF, at most 20 MiB after decoding.
    data: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReceiptRequest {
    device_id: String,
    receipt_id: String,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CommandRequest {
    device_id: String,
    command: DeviceCommand,
}

use alchemy_device_api::DeviceCommand;

fn import_local(root: &Path, input: ImportAsset) -> Result<String> {
    match (input.path, input.data) {
        (Some(path), None) => {
            if path.starts_with(['\\', '/'])
                || path.contains("://")
                || !Path::new(&path).is_absolute()
            {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b103",
                    "图片须为本机绝对文件路径，不接受 URL、网络或设备路径"
                ));
            }
            #[cfg(windows)]
            if path.get(2..).is_some_and(|s| s.contains(':')) {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b104",
                    "不接受文件流或设备路径"
                ));
            }
            let path =
                Path::new(&path)
                    .canonicalize()
                    .context(alchemy_device_api::UiFailure::new(
                        "backend.b105",
                        "图片路径不可用",
                    ))?;
            #[cfg(windows)]
            {
                let text = path.to_string_lossy();
                let normal = text.strip_prefix(r"\\?\").unwrap_or(&text);
                if normal.starts_with("UNC\\") || normal.starts_with(r"\\") {
                    bail!(alchemy_device_api::UiFailure::new(
                        "backend.b106",
                        "不接受网络图片路径"
                    ));
                }
                use std::os::windows::ffi::OsStrExt;
                let drive: Vec<u16> = std::ffi::OsStr::new(&format!("{}\\", &normal[..2]))
                    .encode_wide()
                    .chain(Some(0))
                    .collect();
                if unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(drive.as_ptr()) }
                    == 4
                {
                    bail!(alchemy_device_api::UiFailure::new(
                        "backend.b107",
                        "不接受网络磁盘路径"
                    ));
                }
            }
            if !path.metadata()?.is_file() {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b108",
                    "图片路径须指向普通文件"
                ));
            }
            storage::import(root, &path)
        }
        (None, Some(data)) => storage::put_asset(root, &storage::decode_data(&data)?),
        _ => bail!(alchemy_device_api::UiFailure::new(
            "backend.b109",
            "path 与 data 必须且只能提供一个"
        )),
    }
}

#[derive(Clone)]
struct AlchemyServer {
    core: Arc<Core>,
    services: Arc<Services>,
    operations: Arc<Operations>,
    access: Arc<RwLock<bool>>,
    tool_router: ToolRouter<Self>,
}
impl AlchemyServer {
    #[cfg(test)]
    fn new(core: Arc<Core>, access: Arc<RwLock<bool>>) -> Self {
        let operations = Operations::load(&core.root);
        Self::with_services(Services::headless(core), operations, access)
    }
    fn with_services(
        services: Arc<Services>,
        operations: Arc<Operations>,
        access: Arc<RwLock<bool>>,
    ) -> Self {
        Self {
            core: services.core.clone(),
            services,
            operations,
            access,
            tool_router: Self::tool_router(),
        }
    }
    async fn managed(
        &self,
        future: impl std::future::Future<Output = Result<Value>>,
    ) -> CallToolResult {
        let active = self.access.read().await;
        if !*active {
            return tool_result(Err(alchemy_device_api::ApiError::new(
                alchemy_device_api::ErrorCode::Offline,
                "MCP has stopped",
            )
            .into()));
        }
        tool_result(future.await)
    }
}
fn tool_result(result: Result<Value>) -> CallToolResult {
    match result {
        Ok(value) => CallToolResult::structured(value),
        Err(e) => CallToolResult::structured_error(
            json!({"error":alchemy_device_api::ApiError::from_error(&e)}),
        ),
    }
}
fn preview_result(mut result: CallToolResult) -> CallToolResult {
    if let Some(data) = result
        .structured_content
        .as_ref()
        .and_then(|v| v["data_url"].as_str())
        .and_then(|s| s.strip_prefix("data:image/png;base64,"))
    {
        result
            .content
            .push(rmcp::model::ContentBlock::image(data, "image/png"));
    }
    result
}
#[tool_router]
impl AlchemyServer {
    #[tool(
        description = "Read all device connections, capabilities, pressed keys and cached firmware/GIF status. Cached information is not a fresh hardware readback.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_get_status(&self) -> CallToolResult {
        let active = self.access.read().await;
        if !*active {
            return tool_result(Err(anyhow::anyhow!(alchemy_device_api::UiFailure::new(
                "backend.b110",
                "MCP 已停止"
            ))));
        }
        let devices = if let Some(manager) = self.core.device_manager() {
            match manager.snapshot() {
                Ok(snapshot) => snapshot
                    .devices
                    .iter()
                    .map(alchemy_device_api::DeviceDto::from_state)
                    .collect::<Vec<_>>(),
                Err(e) => return tool_result(Err(e)),
            }
        } else {
            let state = self.core.snapshot();
            vec![alchemy_device_api::DeviceDto {
                device_id: state.selected_device.clone(),
                adapter_id: self.core.adapter_id.clone(),
                name: self.core.device_definition.name.clone(),
                connection: state.connection.clone(),
                error: state.error.clone(),
                definition: self.core.definition(),
                capabilities: state
                    .devices
                    .first()
                    .map(|d| d.capabilities.clone())
                    .unwrap_or_default(),
                inputs: state.input_values.clone(),
                firmware: state.devices.first().map(|d| d.firmware.clone()),
                gif_status: state.gif_status.clone(),
            }]
        };
        tool_result(Ok(json!({"api_version":1,"devices":devices})))
    }
    #[tool(
        description = "Import a local PNG/JPEG/GIF from exactly one of path or Base64 data; returns asset_id. Max 20 MiB, edge 8192, 32 million pixels. No URL/network/device paths."
    )]
    async fn alchemy_import_asset(
        &self,
        Parameters(input): Parameters<ImportAsset>,
    ) -> CallToolResult {
        let active = self.access.clone().read_owned().await;
        if !*active {
            return tool_result(Err(anyhow::anyhow!(alchemy_device_api::UiFailure::new(
                "backend.b110",
                "MCP 已停止"
            ))));
        }
        let root = self.core.root.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _active = active;
            import_local(&root, input).map_err(|e| {
                alchemy_device_api::ApiError::new(
                    alchemy_device_api::ErrorCode::InvalidArgument,
                    e.to_string(),
                )
                .into()
            })
        })
        .await;
        tool_result(
            result
                .context(alchemy_device_api::UiFailure::new(
                    "backend.b111",
                    "素材导入任务失败",
                ))
                .and_then(|r| r)
                .map(|id| json!({"asset_id":id})),
        )
    }
    #[tool(
        description = "Queue a temporary command for the explicit device_id, validated against that device capability and layout. Returns receipt_id: queued is NOT hardware success; poll alchemy_get_command_receipt. acknowledged is an SDK reply, NOT visual verification. Firmware/GIF queries update cached status. No persistent startup writes or scene editing."
    )]
    async fn alchemy_device_command(
        &self,
        Parameters(input): Parameters<CommandRequest>,
    ) -> CallToolResult {
        let active = self.access.read().await;
        if !*active {
            return tool_result(Err(anyhow::anyhow!(alchemy_device_api::UiFailure::new(
                "backend.b110",
                "MCP 已停止"
            ))));
        }
        let core = match crate::devices::resolve_device(&self.core, &input.device_id, true) {
            Ok(core) => core,
            Err(e) => return tool_result(Err(e)),
        };
        let command: HardwareCommand = input.command.into();
        let epoch = command.visual().then(|| core.inner.lock().unwrap().epoch);
        tool_result(
            core.plugin_command(OWNER, command, epoch)
                .map(|id| json!({"receipt_id":id,"status":"queued","verification":"not_observed"})),
        )
    }
    #[tool(
        description = "Get a retained command receipt by receipt_id. queued/sent/acknowledged are distinct; an SDK acknowledgment does not confirm physical output. Missing/expired receipts return an error.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_get_command_receipt(
        &self,
        Parameters(input): Parameters<ReceiptRequest>,
    ) -> CallToolResult {
        let active = self.access.read().await;
        if !*active {
            return tool_result(Err(anyhow::anyhow!(alchemy_device_api::UiFailure::new(
                "backend.b110",
                "MCP 已停止"
            ))));
        }
        let core = match crate::devices::resolve_device(&self.core, &input.device_id, false) {
            Ok(core) => core,
            Err(e) => return tool_result(Err(e)),
        };
        tool_result(
            core.snapshot()
                .receipts
                .into_iter()
                .find(|r| r.id == input.receipt_id)
                .ok_or_else(|| {
                    alchemy_device_api::ApiError::localized(
                        alchemy_device_api::ErrorCode::NotFound,
                        "backend.e022",
                        "未找到回执，可能已超出保留范围",
                    )
                    .into()
                })
                .and_then(|r| Ok(serde_json::to_value(r)?)),
        )
    }
    #[tool(
        description = "Discover saved profiles, management permissions and unsupported operations. Use profile_id for persistent editing and device_id for hardware.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_get_workspace(&self) -> CallToolResult {
        self.managed(async { self.services.workspace() }).await
    }
    #[tool(
        description = "Read saved pages, key visuals/actions/widgets, bindings, scenes and device settings with per-resource revisions. Offline editing is supported. Runtime state is separate.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_get_profile(
        &self,
        Parameters(Strict(input)): Parameters<Strict<ProfileTarget>>,
    ) -> CallToolResult {
        self.managed(async { self.services.profile_snapshot(&input.profile_id) })
            .await
    }
    #[tool(
        description = "Read/search personal assets and read-only plugin icons. The revision covers the complete personal index; use it for edits/imports.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_get_library(
        &self,
        Parameters(Strict(input)): Parameters<Strict<Search>>,
    ) -> CallToolResult {
        self.managed(async {
            let personal = self.core.library.snapshot()?;
            let revision = crate::revisions::revision(&personal);
            let mut catalog = self.core.library_catalog()?;
            if let Some(query) = input.query {
                let query = query.to_lowercase();
                catalog
                    .items
                    .retain(|i| i.name.to_lowercase().contains(&query));
            }
            Ok(json!({"revision":revision,"data":catalog}))
        })
        .await
    }
    #[tool(
        description = "Persist folder creation, rename, move or metadata deletion. Requires the last personal-library revision; original image bytes and saved page references are retained. Plugin folders are read-only."
    )]
    async fn alchemy_edit_library(
        &self,
        Parameters(Strict(input)): Parameters<Strict<LibraryMutation>>,
    ) -> CallToolResult {
        self.managed(async {
            let core = self.core.clone();
            let result = tokio::task::spawn_blocking(move || {
                core.library
                    .edit_checked(input.edit, Some(&input.expected_revision))
            })
            .await??;
            self.services.resource_changed("library");
            Ok(versioned(result))
        })
        .await
    }
    #[tool(
        description = "Import a batch into the personal library from exactly one local path or Base64 data per item. Optional replace_item_id replaces that library reference only. Returns per-item results and revision; partial success is explicit."
    )]
    async fn alchemy_import_library(
        &self,
        Parameters(Strict(input)): Parameters<Strict<LibraryUpload>>,
    ) -> CallToolResult {
        self.managed(async { anyhow::ensure!(!input.items.is_empty() && input.items.len()<=1000,"Provide 1..1000 items");
        let core=self.core.clone();
        let result=tokio::task::spawn_blocking(move || -> Result<Value> {
            crate::revisions::check(Some(&input.expected_revision),&core.library.snapshot()?)?;
            let mut revision=input.expected_revision; let mut results=Vec::new();
            for item in input.items {
                let result=(|| -> Result<Value> {
                    let id=import_local(&core.root,ImportAsset{path:item.path,data:item.data})?;
                    let library=core.library.attach(id.clone(),input.folder_id.clone(),item.name,item.replace_item_id,&revision)?;
                    revision=crate::revisions::revision(&library); Ok(json!({"asset_id":id,"saved":true}))
                })();
                results.push(match result {Ok(value)=>value,Err(e)=>json!({"saved":false,"error":alchemy_device_api::ApiError::from_error(&e)})});
            }
            Ok(json!({"revision":revision,"results":results}))
        }).await??; self.services.resource_changed("library"); Ok(result) }).await
    }
    #[tool(
        description = "Get a 160-pixel PNG preview of a personal image or plugin icon. No hardware output.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_preview_image(
        &self,
        Parameters(Strict(input)): Parameters<Strict<Preview>>,
    ) -> CallToolResult {
        preview_result(self.managed(async { Ok(json!({"data_url":crate::management::image_preview(self.core.clone(),input.image).await?})) }).await)
    }
    #[tool(
        description = "Render a proposed key visual, including plugin icons and current widget samples, using the device compositor. Does not save or upload.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_preview_key(
        &self,
        Parameters(Strict(input)): Parameters<Strict<KeyPreview>>,
    ) -> CallToolResult {
        preview_result(self.managed(async { Ok(json!({"data_url":crate::management::key_preview(self.services.profile(&input.profile_id)?,input.visual).await?})) }).await)
    }
    #[tool(
        description = "Read/search saved launch presets with a revision. Editing presets does not rewrite actions already copied to keys.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_get_applications(
        &self,
        Parameters(Strict(input)): Parameters<Strict<Search>>,
    ) -> CallToolResult {
        self.managed(async {
            let core = self.core.clone();
            let applications =
                tokio::task::spawn_blocking(move || core.applications.snapshot()).await??;
            let revision = crate::revisions::revision(&applications);
            let mut data = applications;
            if let Some(query) = input.query {
                let query = query.to_lowercase();
                data.items.retain(|i| {
                    i.name.to_lowercase().contains(&query)
                        || serde_json::to_string(&i.target)
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&query)
                });
            }
            Ok(json!({"revision":revision,"data":data}))
        })
        .await
    }
    #[tool(
        description = "Create/update/delete a typed Program, RegisteredApplication or Uri preset, or refresh its cached icon. Requires the application-library revision; atomic persistence precedes success."
    )]
    async fn alchemy_edit_applications(
        &self,
        Parameters(Strict(input)): Parameters<Strict<ApplicationMutation>>,
    ) -> CallToolResult {
        self.managed(async {
            let core = self.core.clone();
            let result = tokio::task::spawn_blocking(move || {
                core.applications
                    .edit_checked(input.edit, Some(&input.expected_revision))
            })
            .await??;
            self.services.resource_changed("applications");
            Ok(versioned(result))
        })
        .await
    }
    #[tool(
        description = "Enumerate Windows AppsFolder entries preserving exact identities, including duplicate display names. Does not launch programs.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_get_registered_applications(&self) -> CallToolResult {
        self.managed(async {
            Ok(serde_json::to_value(
                tokio::task::spawn_blocking(crate::application_catalog::ApplicationCatalog::list)
                    .await??,
            )?)
        })
        .await
    }
    #[tool(
        description = "Read a local .lnk target, arguments and working directory without launching it.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_resolve_shortcut(
        &self,
        Parameters(Strict(input)): Parameters<Strict<Shortcut>>,
    ) -> CallToolResult {
        self.managed(async {
            Ok(serde_json::to_value(
                tokio::task::spawn_blocking(move || crate::launcher::resolve_shortcut(&input.path))
                    .await??,
            )?)
        })
        .await
    }
    #[tool(
        description = "Persist create/duplicate/rename/reorder/delete/update pages, batch key visuals/actions/widgets, or apply an application preset. Requires profile_id and the pages revision. Only an affected active page is refreshed."
    )]
    async fn alchemy_edit_pages(
        &self,
        Parameters(Strict(input)): Parameters<Strict<PagesEdit>>,
    ) -> CallToolResult {
        self.managed(async { self.services.edit_pages(input).await })
            .await
    }
    #[tool(
        description = "Persist pageless device input bindings, or copy an application preset to an input. Requires profile_id and bindings revision. New mappings remain disabled until explicitly enabled."
    )]
    async fn alchemy_edit_bindings(
        &self,
        Parameters(Strict(input)): Parameters<Strict<BindingsEdit>>,
    ) -> CallToolResult {
        self.managed(async { self.services.edit_bindings(input) })
            .await
    }
    #[tool(
        description = "Explicitly activate an already saved page on device_id through its Core. Editing a page alone does not activate it."
    )]
    async fn alchemy_activate_page(
        &self,
        Parameters(Strict(input)): Parameters<Strict<PageActivation>>,
    ) -> CallToolResult {
        self.managed(async {
            let core = crate::devices::resolve_device(&self.core, &input.device_id, true)?;
            core.activate(&input.page_id).await?;
            Ok(json!({"queued":true,"verification":"not_observed"}))
        })
        .await
    }
    #[tool(
        description = "Persist scene rules, priority, fallback and pause state using profile_id and scenes revision. Unsupported on devices without pages."
    )]
    async fn alchemy_edit_scenes(
        &self,
        Parameters(Strict(input)): Parameters<Strict<ScenesEdit>>,
    ) -> CallToolResult {
        self.managed(async {
            let core = self.services.profile(&input.profile_id)?;
            let revision = crate::revisions::revision(&input.settings);
            core.save_scenes_checked(input.settings, Some(&input.expected_revision))?;
            Ok(json!({"saved":true,"revision":revision}))
        })
        .await
    }
    #[tool(
        description = "Evaluate a scene condition using the same collectors as the application; unknown is distinct from false.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_check_scene_condition(
        &self,
        Parameters(Strict(input)): Parameters<Strict<ConditionCheck>>,
    ) -> CallToolResult {
        self.managed(async {
            let manager = self
                .core
                .device_manager()
                .context("Device manager unavailable")?;
            Ok(serde_json::to_value(
                manager
                    .scene_manager(&input.profile_id)?
                    .check(input.condition)
                    .await?,
            )?)
        })
        .await
    }
    #[tool(
        description = "Read running programs for building scene conditions.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_get_running_programs(
        &self,
        Parameters(Strict(input)): Parameters<Strict<ProfileTarget>>,
    ) -> CallToolResult {
        self.managed(async {
            let manager = self
                .core
                .device_manager()
                .context("Device manager unavailable")?;
            Ok(serde_json::to_value(
                manager.scene_manager(&input.profile_id)?.programs().await?,
            )?)
        })
        .await
    }
    #[tool(description = "Retry scene evaluation for a saved profile.")]
    async fn alchemy_retry_scenes(
        &self,
        Parameters(Strict(input)): Parameters<Strict<ProfileTarget>>,
    ) -> CallToolResult {
        self.managed(async {
            self.core
                .device_manager()
                .context("Device manager unavailable")?
                .scene_manager(&input.profile_id)?
                .retry();
            Ok(json!({"requested":true}))
        })
        .await
    }
    #[tool(
        description = "Read Audio Dock settings from hardware through its module queue. M18 returns saved configuration, revision and cached status, explicitly without hardware settings readback.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_get_device_settings(
        &self,
        Parameters(Strict(input)): Parameters<Strict<DeviceTarget>>,
    ) -> CallToolResult {
        self.managed(async { self.services.device_settings(input).await })
            .await
    }
    #[tool(
        description = "Persist and apply M18 settings with their revision, or update/read back typed Audio Dock audio/EQ settings. Hardware acceptance and saved state are separate. No raw HID or boot-image writes."
    )]
    async fn alchemy_update_device_settings(
        &self,
        Parameters(Strict(input)): Parameters<Strict<DeviceSettingsEdit>>,
    ) -> CallToolResult {
        self.managed(async { self.services.update_device_settings(input).await })
            .await
    }
    #[tool(
        description = "Retry connection, bind an offline profile explicitly to a device, or enable/disable a declared capability. Does not manufacture hardware verification."
    )]
    async fn alchemy_manage_device(
        &self,
        Parameters(Strict(input)): Parameters<Strict<DeviceManagement>>,
    ) -> CallToolResult {
        self.managed(async {
            let manager = self
                .core
                .device_manager()
                .context("Device manager unavailable")?;
            match input.edit {
                DeviceEdit::Retry { device_id } => {
                    manager.retry(&device_id)?;
                    Ok(json!({"requested":true,"connection_verified":false}))
                }
                DeviceEdit::Bind {
                    profile_id,
                    device_id,
                } => {
                    manager.bind(&profile_id, &device_id)?;
                    Ok(json!({"saved":true}))
                }
                DeviceEdit::Capability {
                    device_id,
                    feature,
                    enabled,
                } => {
                    manager
                        .device_profile(&device_id)?
                        .set_capability_enabled(&feature, enabled)?;
                    Ok(json!({"saved":true}))
                }
            }
        })
        .await
    }
    #[tool(
        description = "Read installed plugin definitions, permissions, enabled/running state and current widget samples. Does not start processes.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_get_plugins(&self) -> CallToolResult {
        self.managed(async {
            let snapshot = self.core.snapshot();
            Ok(json!({"plugins":snapshot.plugins,"samples":self.core.widget_samples()}))
        })
        .await
    }
    #[tool(
        description = "Discover widget presentations/data sources with an explicit lease; release=true ends that discovery lease. Discovery may start an enabled plugin process."
    )]
    async fn alchemy_discover_widgets(
        &self,
        Parameters(Strict(input)): Parameters<Strict<WidgetRequest>>,
    ) -> CallToolResult {
        self.managed(async {
            if input.release {
                self.services
                    .plugins
                    .release_discovery(&input.plugin_id, &input.lease)
                    .await;
                Ok(json!({"released":true}))
            } else {
                Ok(serde_json::to_value(
                    self.services
                        .plugins
                        .discover_widgets(&input.plugin_id, &input.lease)
                        .await?,
                )?)
            }
        })
        .await
    }
    #[tool(
        description = "Read saved language preference, close behavior and per-profile model appearance with revision, plus theme. No credentials.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_get_app_settings(&self) -> CallToolResult {
        self.managed(async {
            let mut result = versioned(self.services.preferences.settings()?);
            let theme = storage::load_workspace(&self.core.root)?.global.theme;
            result["theme"] = json!(theme);
            result["theme_revision"] = json!(crate::revisions::revision(&theme));
            Ok(result)
        })
        .await
    }
    #[tool(
        description = "Persist the global system/light/dark theme using theme_revision from alchemy_get_app_settings."
    )]
    async fn alchemy_update_theme(
        &self,
        Parameters(Strict(input)): Parameters<Strict<ThemeEdit>>,
    ) -> CallToolResult {
        self.managed(async {
            self.core
                .set_theme_checked(input.theme.clone(), Some(&input.expected_revision))?;
            Ok(versioned(input.theme))
        })
        .await
    }
    #[tool(
        description = "Persist application preferences using their revision and publish changes to the live desktop/tray. Model labels only affect software previews."
    )]
    async fn alchemy_update_app_settings(
        &self,
        Parameters(Strict(input)): Parameters<Strict<PreferencesEdit>>,
    ) -> CallToolResult {
        self.managed(async { self.services.update_preferences(input) })
            .await
    }
    #[tool(
        description = "Read bounded command receipts and connection errors for one device. Does not expose serial numbers, SDK paths or credentials and does not claim physical verification.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_get_diagnostics(
        &self,
        Parameters(Strict(input)): Parameters<Strict<DeviceTarget>>,
    ) -> CallToolResult {
        self.managed(async { let core=crate::devices::resolve_device(&self.core,&input.device_id,false)?;let snapshot=core.snapshot();Ok(json!({"connection":snapshot.connection,"error":snapshot.error,"receipts":snapshot.receipts})) }).await
    }
    #[tool(
        description = "Read the last startup backup restore notice; this is separate from a restore request having been staged.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_get_restore_result(&self) -> CallToolResult {
        self.managed(async { Ok(json!({"notice":self.services.restore_notice})) })
            .await
    }
    #[tool(
        description = "Poll a retained application operation. A completed restore operation means staging completed; inspect startup restore result after reconnecting. Interrupted jobs are never automatically replayed.",
        annotations(read_only_hint = true)
    )]
    async fn alchemy_get_operation(
        &self,
        Parameters(Strict(input)): Parameters<Strict<OperationTarget>>,
    ) -> CallToolResult {
        self.managed(async { self.operations.get(&input.operation_id) })
            .await
    }
    #[tool(
        description = "Explicitly forget terminal operations to free space in the bounded 1024-entry history. Active jobs cannot be forgotten. Never reuse their request_ids: deduplication ends when a record is removed."
    )]
    async fn alchemy_forget_operations(
        &self,
        Parameters(Strict(input)): Parameters<Strict<ForgetOperations>>,
    ) -> CallToolResult {
        self.managed(async { self.operations.forget(&input.operation_ids) })
            .await
    }
    #[tool(
        description = "Start an explicit application launch, plugin inspect/install/lifecycle, sensor enablement or backup export/inspect/restore operation. Supply unique request_id; retries with the same payload reuse the operation. Installation/restoration require a fingerprint from a prior inspection. Poll alchemy_get_operation. No automatic desktop confirmation. Backup restore replaces saved configuration and discards unsaved desktop drafts after restarting."
    )]
    async fn alchemy_start_operation(
        &self,
        Parameters(Strict(input)): Parameters<Strict<JobRequest>>,
    ) -> CallToolResult {
        let access = self.access.clone().read_owned().await;
        if !*access {
            return tool_result(Err(alchemy_device_api::ApiError::new(
                alchemy_device_api::ErrorCode::Offline,
                "MCP has stopped",
            )
            .into()));
        }
        tool_result(self.operations.submit(self.services.clone(), input, access))
    }
}
#[tool_handler(router = self.tool_router)]
impl ServerHandler for AlchemyServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("desktop-alchemy", env!("CARGO_PKG_VERSION")))
            .with_instructions("Manage Desktop Alchemy assets, application presets, saved profiles/pages/actions/scenes, device settings, plugins and backups. Read before editing and supply the returned revision. The four original tools remain temporary device control; disabling MCP restores their overrides but keeps persistent management changes. Use explicit device_id for hardware and profile_id for saved configuration. Launch/install/backup operations require unique request_id and operation polling. No raw SDK/HID, credential access, boot-image writes or hardware image readback. SDK receipts are not physical verification.")
    }
}

#[cfg(test)]
mod tests;
