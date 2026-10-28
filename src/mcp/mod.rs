//! Optional, local stdio diagnostic entry point. No GUI or USB startup work.
mod backend;
mod dto;
mod mock;
pub(crate) mod trace;
use rmcp::{model::*, service::RequestContext, ErrorData, RoleServer, ServerHandler, ServiceExt};
use serde_json::Value;

pub fn run(raw_args: &[std::ffi::OsString]) -> anyhow::Result<()> {
    let args = raw_args
        .iter()
        .map(|a| {
            a.to_str()
                .map(str::to_owned)
                .ok_or_else(|| anyhow::anyhow!("MCP arguments must be UTF-8"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    anyhow::ensure!(
        args.iter().filter(|a| a.as_str() == "--mcp-stdio").count() == 1,
        "MCP requires exactly one --mcp-stdio mode flag"
    );
    let mut mock = false;
    for arg in &args {
        match arg.as_str() {
            "--mcp-stdio" | "--mcp-access=read-only" | "--mcp-backend=hid" => {}
            "--mcp-backend=mock" if cfg!(feature = "mcp-mock") => mock = true,
            _ => anyhow::bail!("Unsupported MCP startup argument"),
        }
    }
    if args
        .iter()
        .filter(|a| a.starts_with("--mcp-backend="))
        .count()
        > 1
    {
        anyhow::bail!("Only one MCP backend may be selected");
    }
    anyhow::ensure!(
        args.iter()
            .filter(|a| a.starts_with("--mcp-access="))
            .count()
            <= 1,
        "Only one MCP access policy may be selected"
    );
    crate::hid::enforce_read_only_hid();
    let backend = backend::Backend::new(mock);
    let shutdown = backend.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async move {
        let (input, output) = rmcp::transport::stdio_pipes();
        let transport =
            rmcp::transport::async_rw::AsyncRwTransport::<RoleServer, _, _>::new(input, output)
                .with_max_line_length(65536);
        let server = Adapter { backend }.serve(transport).await?;
        server.waiting().await?;
        Ok::<(), anyhow::Error>(())
    });
    shutdown.shutdown();
    result
}
#[derive(Clone)]
struct Adapter {
    backend: backend::Backend,
}
fn definition<T: schemars::JsonSchema>(
    name: &'static str,
    description: &'static str,
    read_only: bool,
) -> Tool {
    let schema = serde_json::to_value(schemars::schema_for!(T)).expect("static schema");
    let mut tool = Tool::new(
        name,
        description,
        schema.as_object().expect("schema object").clone(),
    )
    .with_annotations(
        ToolAnnotations::new()
            .read_only(read_only)
            .destructive(false),
    );
    let output = serde_json::to_value(schemars::schema_for!(dto::ResultEnvelope))
        .expect("static output schema");
    tool.output_schema = Some(std::sync::Arc::new(
        output.as_object().expect("schema object").clone(),
    ));
    tool
}
fn tools() -> Vec<Tool> {
    vec![
        definition::<dto::Devices>(
            "ak820_devices",
            "Enumerate USB candidates without recognizing firmware.",
            true,
        ),
        definition::<dto::Session>(
            "ak820_session",
            "Explicit diagnostic owner lifecycle; no settings load or unlock.",
            false,
        ),
        definition::<dto::Read>(
            "ak820_read",
            "Read fixed diagnostic observations for an exact recognized image.",
            true,
        ),
        definition::<dto::Tests>(
            "ak820_tests",
            "List or run bounded registered read-only protocol checks.",
            true,
        ),
        definition::<dto::Trace>(
            "ak820_trace",
            "Start, inspect or stop bounded metadata-only HID capture.",
            false,
        ),
        definition::<dto::Operation>(
            "ak820_operation",
            "Inspect or cooperatively cancel this process's operation.",
            false,
        ),
    ]
}
impl ServerHandler for Adapter {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_resources().build())
            .with_instructions("Read-only diagnostic preview. Mock evidence is synthetic host-model evidence. Physical behavior is unqualified. No setters, unlock, reset, loader or flash operations are exposed.")
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: tools(),
            ..Default::default()
        })
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        tools().into_iter().find(|t| t.name == name)
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = Value::Object(request.arguments.unwrap_or_default());
        let result = tokio::select! {
            result = self.backend.call(&request.name, args) => result,
            _ = context.ct.cancelled() => self.backend.cancelled_request(),
        };
        let failed = result["error"].is_object();
        Ok(if failed {
            CallToolResult::structured_error(result)
        } else {
            CallToolResult::structured(result)
        }
        .into())
    }
    async fn list_resources(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult {
            resources: self
                .backend
                .resources()
                .into_iter()
                .map(|(uri, name)| Resource::new(uri, name))
                .collect(),
            ..Default::default()
        })
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let value = self
            .backend
            .resource(&request.uri)
            .ok_or_else(|| ErrorData::invalid_params("Unknown process-local resource", None))?;
        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            serde_json::to_string(&value).expect("JSON"),
            request.uri,
        )])
        .into())
    }
}
