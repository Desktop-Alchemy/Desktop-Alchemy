use alchemy_device_api::*;
use std::sync::Arc;
pub const ID: &str = "example.simulator";
pub struct Adapter;
impl DeviceAdapter for Adapter {
    fn operations(&self) -> &'static [&'static str] {
        &["input"]
    }
    fn id(&self) -> &'static str {
        ID
    }
    fn layout(&self) -> DeviceLayout {
        DeviceLayout {
            input_keys: (1..=32).collect(),
            display_keys: (1..=32).collect(),
            columns: 8,
            key_width: 80,
            key_height: 80,
            ..Default::default()
        }
    }
    fn definition(&self) -> DeviceDefinition {
        DeviceDefinition {
            inputs: (1..=32)
                .map(|k| InputControl {
                    id: k.to_string(),
                    name: format!("按键 {k}"),
                    kind: InputKind::Button,
                })
                .chain([
                    InputControl {
                        id: "knob".into(),
                        name: "旋钮".into(),
                        kind: InputKind::Rotary,
                    },
                    InputControl {
                        id: "slider".into(),
                        name: "滑杆".into(),
                        kind: InputKind::Analog {
                            min: 0.0,
                            max: 100.0,
                            unit: "%".into(),
                        },
                    },
                ])
                .collect(),
            displays: vec![],
            adapter_id: ID.into(),
            name: "Platform Simulator".into(),
            supports_pages: true,
            input_names: (1..=32).map(|k| (k, format!("按键 {k}"))).collect(),
            layout: self.layout(),
        }
    }
    fn default_config(&self) -> AppConfig {
        let page = default_page("主页", &self.definition());
        AppConfig {
            active_page: page.id.clone(),
            pages: vec![page],
            ..AppConfig::empty()
        }
    }
    fn run(self: Arc<Self>, host: AdapterHost) -> AdapterFuture {
        Box::pin(async move {
            let descriptor = DeviceDescriptor {
                adapter_id: ID.into(),
                id: "simulator".into(),
                layout: Box::new(self.layout()),
                model: "Platform Simulator".into(),
                serial: "simulator".into(),
                firmware: "1".into(),
                vendor_id: 0,
                product_id: 0,
                capabilities: ["key_input", "key_images", "key_gif", "device_control"]
                    .into_iter()
                    .map(|id| {
                        (
                            id.into(),
                            Capability {
                                available: true,
                                verification: Verification::Unverified,
                                reason: "模拟设备".into(),
                            },
                        )
                    })
                    .collect(),
            };
            host.api.discover(vec![descriptor.clone()])?;
            host.api.connected(descriptor, "simulator".into())?;
            let mut cursor = 0;
            loop {
                while let Some(batch) = host.api.next_batch(&mut cursor) {
                    if host.api.current(&batch.id) {
                        host.api.complete(&batch.id, Ok(()));
                    } else {
                        host.api
                            .complete(&batch.id, Err(anyhow::anyhow!(alchemy_device_api::UiFailure::new("backend.b311", "操作已过期"))));
                    }
                }
                tokio::select! {_=host.cancel.cancelled()=>return Ok(()),_=tokio::time::sleep(std::time::Duration::from_millis(10))=>{}}
            }
        })
    }
    fn request<'a>(
        &'a self,
        host: AdapterHost,
        connection: Connection,
        operation: String,
        payload: serde_json::Value,
    ) -> ServiceFuture<'a> {
        Box::pin(async move {
            anyhow::ensure!(operation == "input", alchemy_device_api::UiFailure::new("backend.e016", "模块操作不支持"));
            anyhow::ensure!(host.api.connection_current(&connection), alchemy_device_api::UiFailure::new("backend.e019", "设备已离线"));
            let input = alchemy_device_api::strict::<InputValue>(payload)?;
            host.api.input(&connection, input);
            Ok(serde_json::json!({"accepted":true}))
        })
    }
}
