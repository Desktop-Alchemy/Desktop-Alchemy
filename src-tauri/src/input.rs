//! Normalized input and bindings shared by device modules.
use crate::{core::Core, model::*};
use anyhow::Result;
use std::sync::Arc;

pub use alchemy_device_api::RawInputSpec;
impl Core {
    pub fn dispatch_input(
        self: &Arc<Self>,
        token: &alchemy_device_api::Connection,
        input: alchemy_device_api::InputValue,
    ) {
        use alchemy_device_api::{InputKind, InputTrigger, InputValue};
        let id = match &input {
            InputValue::Button { control_id, .. }
            | InputValue::Rotary { control_id, .. }
            | InputValue::Analog { control_id, .. } => control_id,
        };
        let definition = self.definition().normalized();
        let Some(control) = definition.inputs.iter().find(|c| &c.id == id) else {
            return;
        };
        let valid = match (&control.kind, &input) {
            (InputKind::Button, InputValue::Button { .. }) => true,
            (InputKind::Rotary, InputValue::Rotary { delta, .. }) => *delta != 0,
            (InputKind::Analog { min, max, .. }, InputValue::Analog { value, .. }) => {
                value.is_finite()
                    && min.is_finite()
                    && max.is_finite()
                    && max > min
                    && value >= min
                    && value <= max
            }
            _ => false,
        };
        if !valid {
            return;
        }
        let actions = {
            let mut inner = self.inner.lock().unwrap();
            if inner.connection_epoch != token.epoch
                || inner.paths.get(&token.device_id) != Some(&token.path)
                || *self.stop.borrow()
            {
                return;
            }
            if matches!(&input,InputValue::Button{pressed,..} if inner.snapshot.input_values.get(id).is_some_and(|old|matches!(old,InputValue::Button{pressed:p,..} if p==pressed)))
            {
                return;
            }
            if let InputValue::Button { pressed, .. } = &input {
                if let Ok(key) = id.parse::<u8>() {
                    inner.snapshot.pressed.retain(|k| *k != key);
                    if *pressed {
                        inner.snapshot.pressed.push(key);
                        inner.snapshot.last_key_press = Some(now());
                    }
                }
            }
            let first = !inner.snapshot.input_values.contains_key(id);
            inner
                .snapshot
                .input_values
                .insert(id.clone(), input.clone());
            let bindings = if self.supports_pages {
                inner
                    .snapshot
                    .config
                    .pages
                    .iter()
                    .find(|p| p.id == inner.snapshot.active_page)
                    .map(|p| p.bindings.clone())
                    .unwrap_or_default()
            } else {
                inner.snapshot.config.bindings.clone()
            };
            let mut actions = vec![];
            for (index, binding) in bindings
                .iter()
                .enumerate()
                .filter(|(_, b)| b.enabled && b.control_id == *id)
            {
                let fire = match (&binding.trigger, &input) {
                    (InputTrigger::Press, InputValue::Button { pressed, .. }) => *pressed,
                    (InputTrigger::Clockwise, InputValue::Rotary { delta, .. }) => *delta > 0,
                    (InputTrigger::Counterclockwise, InputValue::Rotary { delta, .. }) => {
                        *delta < 0
                    }
                    (trigger, InputValue::Analog { value, .. }) => {
                        let InputKind::Analog { min, max, .. } = &control.kind else {
                            continue;
                        };
                        let value = (value - min) / (max - min);
                        let key = format!("{}:{index}:{id}", inner.snapshot.active_page);
                        let (threshold, hysteresis, above) = match trigger {
                            InputTrigger::Above {
                                threshold,
                                hysteresis,
                            } => (*threshold, *hysteresis, true),
                            InputTrigger::Below {
                                threshold,
                                hysteresis,
                            } => (*threshold, *hysteresis, false),
                            _ => continue,
                        };
                        let reset = if above {
                            if hysteresis == 0.0 {
                                value < threshold
                            } else {
                                value <= threshold - hysteresis
                            }
                        } else if hysteresis == 0.0 {
                            value > threshold
                        } else {
                            value >= threshold + hysteresis
                        };
                        let crossed = if above {
                            value >= threshold
                        } else {
                            value <= threshold
                        };
                        let armed = inner.analog_armed.entry(key).or_insert(reset);
                        if first {
                            *armed = reset;
                            false
                        } else if reset {
                            *armed = true;
                            false
                        } else if crossed && *armed {
                            *armed = false;
                            true
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                if fire {
                    actions.push(binding.action.clone());
                }
            }
            actions
        };
        if let InputValue::Button { pressed, .. } = &input {
            self.event(DeviceEvent {
                input: None,
                at: now(),
                kind: if *pressed { "button.down" } else { "button.up" }.into(),
                device_id: token.device_id.clone(),
                key: id.parse().ok(),
                pressed: Some(*pressed),
                page_id: None,
            });
        }
        self.event(DeviceEvent {
            input: Some(input.clone()),
            at: now(),
            kind: "input".into(),
            device_id: token.device_id.clone(),
            key: None,
            pressed: None,
            page_id: None,
        });
        self.emit();
        for action in actions {
            self.spawn_input_action(token.clone(), action);
        }
    }
    fn spawn_input_action(
        self: &Arc<Self>,
        token: alchemy_device_api::Connection,
        action: KeyAction,
    ) {
        if matches!(action, KeyAction::None) {
            return;
        }
        let Ok(permit) = self.action_slots.clone().try_acquire_owned() else {
            self.log("warning", "输入动作队列已满");
            return;
        };
        let core = self.clone();
        // Raw Input and adapter callbacks may originate on native threads.
        tauri::async_runtime::spawn(async move {
            let _permit = permit;
            {
                let inner = core.inner.lock().unwrap();
                if inner.connection_epoch != token.epoch
                    || inner.paths.get(&token.device_id) != Some(&token.path)
                    || *core.stop.borrow()
                {
                    return;
                }
            }
            let result = if let Some(target) = crate::launch_service::action_target(&action) {
                let current = core.clone();
                tokio::task::spawn_blocking(move || {
                    {
                        let inner = current.inner.lock().unwrap();
                        if inner.connection_epoch != token.epoch
                            || inner.paths.get(&token.device_id) != Some(&token.path)
                            || *current.stop.borrow()
                        {
                            return Ok(());
                        }
                    }
                    crate::launch_service::execute(&target)
                })
                .await
                .map_err(anyhow::Error::from)
                .and_then(|value| value)
            } else {
                core.execute_action(action).await
            };
            if let Err(e) = result {
                core.log("error", e.to_string());
                let _ = core
                    .action_errors
                    .send(crate::model::UiError::from_error(e));
            }
        });
    }
    pub fn attach_input_device(&self, device: DeviceDescriptor, path: String) -> bool {
        let id = device.id.clone();
        let mut i = self.inner.lock().unwrap();
        if i.paths.get(&id) == Some(&path) {
            return false;
        }
        i.connection_epoch += 1;
        i.paths.clear();
        i.paths.insert(id.clone(), path);
        let mut device = device;
        if let Some(saved) = i
            .snapshot
            .config
            .verification
            .get(&format!("{}|{}", device.id, device.firmware))
        {
            for (id, capability) in &mut device.capabilities {
                if let Some(observation) = saved.get(id) {
                    capability.verification = observation.verification.clone();
                    capability.reason = observation.reason.clone();
                }
            }
        }
        i.snapshot.devices = vec![device];
        i.snapshot.selected_device = Some(id.clone());
        i.snapshot.connection = "ready".into();
        i.snapshot.error = None;
        i.snapshot.pressed.clear();
        i.snapshot.input_values.clear();
        i.analog_armed.clear();
        i.snapshot.last_key_press = None;
        drop(i);
        self.event(DeviceEvent {
            input: None,
            at: now(),
            kind: "device.connected".into(),
            device_id: id,
            key: None,
            pressed: None,
            page_id: None,
        });
        true
    }
    pub fn input_event(
        self: &Arc<Self>,
        device_id: &str,
        path: &str,
        connection_epoch: u64,
        key: u8,
        pressed: bool,
    ) {
        self.dispatch_input(
            &alchemy_device_api::Connection {
                device_id: device_id.into(),
                path: path.into(),
                epoch: connection_epoch,
            },
            alchemy_device_api::InputValue::Button {
                control_id: key.to_string(),
                pressed,
            },
        );
    }
    pub fn save_control_bindings(
        &self,
        bindings: Vec<alchemy_device_api::ControlBinding>,
    ) -> Result<()> {
        self.save_control_bindings_checked(bindings, None)
    }
    pub fn save_control_bindings_checked(
        &self,
        bindings: Vec<alchemy_device_api::ControlBinding>,
        expected: Option<&str>,
    ) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        crate::revisions::check(expected, &inner.snapshot.config.bindings)?;
        let mut next = inner.snapshot.config.clone();
        next.bindings = bindings;
        self.persist(&next)?;
        inner.snapshot.config = next;
        inner.analog_armed.clear();
        drop(inner);
        self.emit();
        Ok(())
    }
    pub fn definition(&self) -> DeviceDefinition {
        self.device_definition.clone()
    }
}
