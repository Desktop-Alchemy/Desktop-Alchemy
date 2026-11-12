//! One owner for the official C SDK. Pages are prepared before any USB submission,
//! then uploaded without per-key refreshes or sleeps; the page's Refresh commits it.
use crate::sdk::{self, SdkEvent, SdkPaths};
use alchemy_device_api::{assets as storage, *};
use anyhow::{bail, Context, Result};
use image::{AnimationDecoder, DynamicImage};
use libloading::Library;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{c_char, c_void, CStr},
    io::BufReader,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

type Handle = *mut c_void;
type Simple = unsafe extern "C" fn(Handle) -> u32;
type ImageFn = unsafe extern "C" fn(Handle, *const u8, usize, u8) -> u32;
#[repr(C)]
struct HidInfo {
    path: *const c_char,
    vendor: u16,
    product: u16,
    serial: *const u16,
    release: u16,
    manufacturer: *const u16,
    product_name: *const u16,
    usage_page: u16,
    usage: u16,
    interface: i32,
    next: *mut HidInfo,
}
struct Api {
    enumerate: unsafe extern "C" fn(u16, u16) -> *mut HidInfo,
    free: unsafe extern "C" fn(*mut HidInfo),
    create: unsafe extern "C" fn(*const HidInfo, *mut Handle) -> u32,
    destroy: Simple,
    size: unsafe extern "C" fn(Handle, u16, u16, u16) -> u32,
    read: unsafe extern "C" fn(Handle, *mut u8, *mut usize, i32) -> u32,
    firmware: unsafe extern "C" fn(Handle, *mut u8, usize) -> u32,
    image: ImageFn,
    background: unsafe extern "C" fn(Handle, *const u8, usize, u32) -> u32,
    frame: unsafe extern "C" fn(Handle, *const u8, usize, u16, u16, u16, u16, u8) -> u32,
    clear_key: unsafe extern "C" fn(Handle, u8) -> u32,
    clear_all: Simple,
    refresh: Simple,
    wake: Simple,
    heartbeat: Simple,
    brightness: unsafe extern "C" fn(Handle, u8) -> u32,
    rgb: unsafe extern "C" fn(Handle, u16, u8, u8, u8) -> u32,
    colors: unsafe extern "C" fn(Handle, u16, *const u8) -> u32,
    reset_led: Simple,
    _library: Library,
}
fn check(result: u32) -> Result<()> {
    if result != 0 {
        bail!("官方 C SDK 错误 0x{result:08X}")
    }
    Ok(())
}
impl Api {
    fn load(path: &Path) -> Result<Self> {
        // Resolve dependent runtime DLLs next to the verified transport, without PATH lookup.
        let library: Library =
            unsafe { libloading::os::windows::Library::load_with_flags(path, 0x1100)? }.into();
        macro_rules! symbol {
            ($name:literal) => {
                unsafe { *library.get(concat!("transport_", $name, "\0").as_bytes())? }
            };
        }
        Ok(Self {
            enumerate: symbol!("hid_enumerate"),
            free: symbol!("hid_free_enumeration"),
            create: symbol!("create"),
            destroy: symbol!("destroy"),
            size: symbol!("set_reportSize"),
            read: symbol!("read"),
            firmware: symbol!("get_firmware_version"),
            image: symbol!("set_key_image_stream"),
            background: symbol!("set_background_image_stream"),
            frame: symbol!("set_background_frame_stream"),
            clear_key: symbol!("clear_key"),
            clear_all: symbol!("clear_all_keys"),
            refresh: symbol!("refresh"),
            wake: symbol!("wakeup_screen"),
            heartbeat: symbol!("heartbeat"),
            brightness: symbol!("set_key_brightness"),
            rgb: symbol!("set_led_color"),
            colors: symbol!("set_single_led_color"),
            reset_led: symbol!("reset_led_color"),
            _library: library,
        })
    }
    fn discover<'a>(
        &'a self,
        existing: &BTreeSet<String>,
        tx: &mpsc::Sender<SdkEvent>,
    ) -> Result<(BTreeSet<String>, Vec<Device<'a>>)> {
        let head = unsafe { (self.enumerate)(0, 0) };
        let mut ptr = head;
        let mut present = BTreeSet::new();
        let result = {
            let mut added = Vec::new();
            while !ptr.is_null() {
                let info = unsafe { &*ptr };
                if matches!(
                    (info.vendor, info.product),
                    (0x6603, 0x1009) | (0x6603, 0x1012) | (0x5548, 0x1000) | (0x5548, 0x1012)
                ) && info.usage_page == 0xffa0
                    && info.usage == 1
                    && !info.path.is_null()
                {
                    let path = unsafe { CStr::from_ptr(info.path) }
                        .to_string_lossy()
                        .into_owned();
                    present.insert(path.clone());
                    if !existing.contains(&path) {
                        let opened: Result<Device<'a>> = (|| {
                            let mut raw = std::ptr::null_mut();
                            check(unsafe { (self.create)(info, &mut raw) })?;
                            if raw.is_null() {
                                bail!(alchemy_device_api::UiFailure::new("backend.b312", "C SDK 返回空设备句柄"))
                            }
                            let handle = Transport { api: self, raw };
                            check(unsafe { (self.size)(raw, 513, 1025, 0) })?;
                            let serial = wide_string(info.serial);
                            let id = format!(
                                "{:04x}:{:04x}:{}",
                                info.vendor,
                                info.product,
                                if serial.is_empty() { &path } else { &serial }
                            );
                            check(unsafe { (self.wake)(raw) })?;
                            let firmware = handle.firmware()?;
                            Ok(Device {
                                connection: None,
                                reader: Reader::start(self.read, raw, path.clone(), tx.clone()),
                                handle,
                                id,
                                path,
                                serial,
                                firmware,
                                vendor: info.vendor,
                                product: info.product,
                                gifs: BTreeMap::new(),
                                gif_running: false,
                                heartbeat: Instant::now(),
                            })
                        })();
                        match opened {
                            Ok(device) => added.push(device),
                            Err(error) => {
                                let _ = tx.try_send(SdkEvent::InputReport {
                                    header: format!("设备打开失败：{error:#}"),
                                });
                            }
                        }
                    }
                }
                ptr = info.next;
            }
            Ok((present, added))
        };
        unsafe { (self.free)(head) };
        result
    }
}
fn wide_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut text = Vec::new();
    for i in 0..256 {
        let ch = unsafe { *ptr.add(i) };
        if ch == 0 {
            break;
        }
        text.push(ch);
    }
    String::from_utf16_lossy(&text)
}
struct Transport<'a> {
    api: &'a Api,
    raw: Handle,
}
impl Drop for Transport<'_> {
    fn drop(&mut self) {
        unsafe { (self.api.destroy)(self.raw) };
    }
}
impl Transport<'_> {
    fn firmware(&self) -> Result<String> {
        let mut buffer = [0; 513];
        check(unsafe { (self.api.firmware)(self.raw, buffer.as_mut_ptr(), buffer.len()) })?;
        let text = buffer
            .split(|x| *x == 0)
            .find(|part| !part.is_empty())
            .unwrap_or_default();
        Ok(String::from_utf8_lossy(text).trim().to_string())
    }
    fn image(&self, key: u8, data: &[u8]) -> Result<()> {
        check(unsafe {
            (self.api.image)(self.raw, data.as_ptr(), data.len(), sdk::m18_key_index(key))
        })
    }
    fn refresh(&self) -> Result<()> {
        check(unsafe { (self.api.refresh)(self.raw) })
    }
}
struct Device<'a> {
    connection: Option<Connection>,
    // Drop order matters: join the reader before releasing the transport and its DLL.
    reader: Reader,
    handle: Transport<'a>,
    id: String,
    path: String,
    serial: String,
    firmware: String,
    vendor: u16,
    product: u16,
    gifs: BTreeMap<u8, Animation>,
    gif_running: bool,
    heartbeat: Instant,
}
type ReadFn = unsafe extern "C" fn(Handle, *mut u8, *mut usize, i32) -> u32;
struct Reader {
    id: u64,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    error: std::sync::mpsc::Receiver<String>,
}
fn read_has_data(result: u32) -> Result<bool> {
    if result == 0x05000302 {
        return Ok(false);
    }
    check(result)?;
    Ok(true)
}
impl Reader {
    fn start(read: ReadFn, handle: Handle, path: String, tx: mpsc::Sender<SdkEvent>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        static READER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let reader_id = READER_ID.fetch_add(1, Ordering::Relaxed);
        let (error_tx, error) = std::sync::mpsc::channel();
        let handle = handle as usize;
        let thread = std::thread::spawn(move || {
            let mut unrecognized = 0;
            while !stopped.load(Ordering::Acquire) {
                let mut buffer = [0; 2048];
                let mut length = buffer.len();
                let result = read_has_data(unsafe {
                    read(handle as Handle, buffer.as_mut_ptr(), &mut length, 5)
                });
                match result {
                    Ok(false) => continue,
                    Err(e) => {
                        let _ = error_tx.send(e.to_string());
                        break;
                    }
                    Ok(true) => {}
                }
                if let Some((key, pressed)) = input_event(&buffer[..length.min(buffer.len())]) {
                    if tx
                        .try_send(SdkEvent::Key {
                            reader_id,
                            path: path.clone(),
                            key,
                            pressed,
                        })
                        .is_err()
                    {
                        let _ = error_tx.send("输入队列不可用，请重新连接".into());
                        break;
                    }
                } else if length > 0 && unrecognized < 4 {
                    let header = buffer[..length.min(16)]
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>();
                    let _ = tx.try_send(SdkEvent::InputReport { header });
                    unrecognized += 1;
                }
            }
        });
        Self {
            id: reader_id,
            stop,
            thread: Some(thread),
            error,
        }
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
struct Animation {
    frames: Vec<(Vec<u8>, Duration)>,
    index: usize,
    next: Instant,
}
struct Prepared {
    command: HardwareCommand,
    image: Vec<u8>,
    animation: Option<Animation>,
}

fn jpeg(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let decoded = image::load_from_memory(data)?.resize_exact(
        width,
        height,
        image::imageops::FilterType::Lanczos3,
    );
    encode_jpeg(&decoded)
}
fn encode_jpeg(image: &DynamicImage) -> Result<Vec<u8>> {
    // M18's static icon protocol uses JPEG; pre-rendered key backgrounds are opaque.
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95)
        .encode_image(&image.to_rgb8())?;
    Ok(out)
}
fn prepare(root: &Path, work: &TransportWork, cancel: &CancellationToken) -> Result<Prepared> {
    let command = work.led_command.as_ref().unwrap_or(&work.command).clone();
    let mut image = Vec::new();
    let mut animation = None;
    match &command {
        HardwareCommand::KeyImage { asset_id, .. }
        | HardwareCommand::BulkImage { asset_id, .. } => {
            image = jpeg(&std::fs::read(storage::asset(root, asset_id)?)?, 64, 64)?
        }
        HardwareCommand::KeyData { data, .. } => {
            image = if let Some(png) = &work.image_frame {
                jpeg(png, 64, 64)?
            } else {
                jpeg(&storage::decode_data(data)?, 64, 64)?
            }
        }
        HardwareCommand::Background { asset_id, .. } => {
            image = jpeg(&std::fs::read(storage::asset(root, asset_id)?)?, 480, 272)?
        }
        HardwareCommand::BackgroundData { data, .. } => {
            image = jpeg(&storage::decode_data(data)?, 480, 272)?
        }
        HardwareCommand::KeyGif { asset_id, .. } => {
            let decoder = image::codecs::gif::GifDecoder::new(BufReader::new(
                std::fs::File::open(storage::asset(root, asset_id)?)?,
            ))?;
            let mut frames = Vec::new();
            let mut bytes = 0;
            for frame in decoder.into_frames() {
                if cancel.is_cancelled() {
                    bail!(alchemy_device_api::UiFailure::new("backend.b313", "设备会话已停止"))
                }
                let frame = frame?;
                let (num, den) = frame.delay().numer_denom_ms();
                let delay = Duration::from_millis((u64::from(num) / u64::from(den.max(1))).max(10));
                let img = DynamicImage::ImageRgba8(frame.into_buffer()).resize_exact(
                    64,
                    64,
                    image::imageops::FilterType::Lanczos3,
                );
                let data = encode_jpeg(&img)?;
                bytes += data.len();
                if frames.len() >= 1000 || bytes > 32 * 1024 * 1024 {
                    bail!(alchemy_device_api::UiFailure::new("backend.b314", "GIF 解码结果超过 1000 帧或 32 MiB"))
                }
                frames.push((data, delay));
            }
            if frames.is_empty() {
                bail!(alchemy_device_api::UiFailure::new("backend.b315", "GIF 没有可显示帧"))
            }
            image = frames[0].0.clone();
            animation = Some(Animation {
                next: Instant::now() + frames[0].1,
                frames,
                index: 0,
            });
        }
        _ => {}
    }
    Ok(Prepared {
        command,
        image,
        animation,
    })
}
impl Device<'_> {
    fn execute(&mut self, mut p: Prepared, host: &AdapterHost) -> Result<bool> {
        use HardwareCommand::*;
        let api = self.handle.api;
        let raw = self.handle.raw;
        let dirty = match &p.command {
            ScreenBrightness { value } => {
                check(unsafe { (api.brightness)(raw, *value) })?;
                false
            }
            LedBrightness { .. } => bail!(alchemy_device_api::UiFailure::new("backend.b316", "LED 亮度必须先经 RGB 比例调光解析")),
            LedColor { color } => {
                let rgb = storage::color(color)?;
                check(unsafe { (api.rgb)(raw, 24, rgb[0], rgb[1], rgb[2]) })?;
                false
            }
            LedColors { colors } => {
                check(unsafe { (api.colors)(raw, colors.len() as u16, colors.as_ptr().cast()) })?;
                false
            }
            ResetLed => {
                check(unsafe { (api.reset_led)(raw) })?;
                false
            }
            KeyImage { key, .. } | KeyData { key, .. } => {
                self.gifs.remove(key);
                self.handle.image(*key, &p.image)?;
                true
            }
            BulkImage { keys, .. } => {
                for key in keys {
                    self.gifs.remove(key);
                    self.handle.image(*key, &p.image)?;
                }
                true
            }
            Background { persistent, .. } | BackgroundData { persistent, .. } => {
                if *persistent {
                    check(unsafe { (api.background)(raw, p.image.as_ptr(), p.image.len(), 3000) })?;
                } else {
                    check(unsafe {
                        (api.frame)(raw, p.image.as_ptr(), p.image.len(), 480, 272, 0, 0, 0)
                    })?;
                }
                true
            }
            ClearKey { key } => {
                self.gifs.remove(key);
                check(unsafe { (api.clear_key)(raw, sdk::m18_key_index(*key)) })?;
                true
            }
            ClearAll => {
                self.gifs.clear();
                check(unsafe { (api.clear_all)(raw) })?;
                true
            }
            Refresh => {
                self.handle.refresh()?;
                false
            }
            KeyGif { key, .. } => {
                self.handle.image(*key, &p.image)?;
                self.gifs
                    .insert(*key, p.animation.take().context(alchemy_device_api::UiFailure::new("backend.b317", "GIF 未解码"))?);
                true
            }
            ClearGif { key } => {
                self.gifs.remove(key);
                false
            }
            StartGif => {
                self.gif_running = true;
                false
            }
            StopGif => {
                self.gif_running = false;
                false
            }
            GifStatus => {
                host.api.cached_value(self.connection.as_ref().context(alchemy_device_api::UiFailure::new("backend.b318", "设备会话已失效"))?,"gif_status",json!({"running":self.gif_running,"keys":self.gifs.keys().collect::<Vec<_>>()} ).to_string())?;
                false
            }
            Firmware => {
                self.firmware = self.handle.firmware()?;
                host.api.cached_value(
                    self.connection.as_ref().context(alchemy_device_api::UiFailure::new("backend.b318", "设备会话已失效"))?,
                    "firmware",
                    self.firmware.clone(),
                )?;
                false
            }
        };
        Ok(dirty)
    }
    fn tick(&mut self) -> Result<()> {
        if let Ok(error) = self.reader.error.try_recv() {
            bail!("按键读取失败：{error}");
        }
        let now = Instant::now();
        let mut dirty = false;
        if self.gif_running {
            for (key, a) in &mut self.gifs {
                if a.frames.len() > 1 && now >= a.next {
                    a.index = (a.index + 1) % a.frames.len();
                    self.handle.image(*key, &a.frames[a.index].0)?;
                    a.next = now + a.frames[a.index].1;
                    dirty = true;
                }
            }
        }
        if dirty {
            self.handle.refresh()?;
        }
        if now.duration_since(self.heartbeat) >= Duration::from_secs(5) {
            check(unsafe { (self.handle.api.heartbeat)(self.handle.raw) })?;
            self.heartbeat = now;
        }
        Ok(())
    }
}
fn input_event(data: &[u8]) -> Option<(u8, bool)> {
    let data = if matches!(data.first(), Some(0 | 4)) && data.get(1..4) == Some(b"ACK") {
        &data[1..]
    } else {
        data
    };
    if data.len() < 11 || &data[..3] != b"ACK" || &data[5..7] != b"OK" {
        return None;
    }
    let key = match data[9] {
        1..=15 => data[9],
        0x25 => 16,
        0x30 => 17,
        0x31 => 18,
        _ => return None,
    };
    match data[10] {
        0 | 2 => Some((key, false)),
        1 => Some((key, true)),
        _ => None,
    }
}

impl DeviceSession for Device<'_> {
    type Prepared = Vec<Prepared>;
    fn prepare_batch(
        &self,
        work: &[TransportWork],
        root: &Path,
        cancel: &CancellationToken,
    ) -> Result<Self::Prepared> {
        let mut prepared = Vec::with_capacity(work.len());
        let mut bytes = 0usize;
        for work in work {
            let item = prepare(root, work, cancel)?;
            bytes = bytes.saturating_add(item.image.len()).saturating_add(
                item.animation
                    .as_ref()
                    .map(|a| a.frames.iter().map(|(frame, _)| frame.len()).sum::<usize>())
                    .unwrap_or(0),
            );
            if bytes > MAX_PAGE_BYTES {
                bail!(alchemy_device_api::UiFailure::new("backend.b319", "预编码数据超过 64 MiB"))
            }
            prepared.push(item);
        }
        Ok(prepared)
    }
    fn submit_batch(
        &mut self,
        prepared: Self::Prepared,
        page: bool,
        host: &AdapterHost,
    ) -> Result<()> {
        submit_prepared(prepared, page, |prepared| match prepared {
            Some(prepared) => self.execute(prepared, host),
            None => {
                self.handle.refresh()?;
                Ok(false)
            }
        })
    }
}

fn execute_batch(device: &mut Device<'_>, batch: TransportBatch, host: &AdapterHost) -> Result<()> {
    let result = (|| {
        let prepared = device.prepare_batch(&batch.work, &host.root, &host.cancel)?;
        if host.cancel.is_cancelled() || !host.api.current(&batch.id) {
            bail!(alchemy_device_api::UiFailure::new("backend.b311", "操作已过期"))
        }
        device.submit_batch(prepared, batch.page, host)
    })();
    host.api.complete(&batch.id, result);
    Ok(())
}

fn submit_prepared(
    prepared: Vec<Prepared>,
    is_page: bool,
    mut apply: impl FnMut(Option<Prepared>) -> Result<bool>,
) -> Result<()> {
    let mut dirty = false;
    for p in prepared {
        let refresh = matches!(p.command, HardwareCommand::Refresh);
        let changed = apply(Some(p))?;
        if refresh {
            dirty = false;
        } else {
            dirty |= changed;
        }
    }
    if dirty {
        if is_page {
            bail!(alchemy_device_api::UiFailure::new("backend.b320", "页面批次缺少末尾刷新命令"));
        }
        apply(None)?;
    }
    Ok(())
}
type ReaderConnections = Arc<std::sync::Mutex<BTreeMap<String, (u64, Connection)>>>;
fn worker(
    host: AdapterHost,
    paths: SdkPaths,
    tx: mpsc::Sender<SdkEvent>,
    readers: ReaderConnections,
) -> Result<()> {
    let api = Api::load(&paths.verified_transport(&host.root)?)?;
    let mut cursor = 0;
    let mut devices: BTreeMap<String, Device<'_>> = BTreeMap::new();
    let mut scanned = Instant::now() - Duration::from_secs(1);
    while !host.cancel.is_cancelled() {
        for id in host.api.retry_requests() {
            if let Some(d) = devices.remove(&id) {
                readers.lock().unwrap().remove(&d.path);
                if let Some(token) = &d.connection {
                    host.api.disconnected(token);
                }
                drop(d);
            }
        }
        if scanned.elapsed() >= Duration::from_millis(500) {
            let existing = devices.values().map(|d| d.path.clone()).collect();
            let (present, added) = api.discover(&existing, &tx)?;
            let removed: Vec<_> = devices
                .iter()
                .filter(|(_, d)| !present.contains(&d.path))
                .map(|(id, _)| id.clone())
                .collect();
            for id in removed {
                if let Some(d) = devices.remove(&id) {
                    readers.lock().unwrap().remove(&d.path);
                    if let Some(token) = &d.connection {
                        host.api.disconnected(token);
                    }
                    drop(d);
                }
            }
            let descriptions = devices
                .values()
                .chain(added.iter())
                .map(|d| {
                    sdk::device_descriptor(
                        d.vendor,
                        d.product,
                        d.serial.clone(),
                        d.firmware.clone(),
                        &d.path,
                    )
                })
                .collect();
            host.api.discover(descriptions)?;
            for mut d in added {
                let descriptor = sdk::device_descriptor(
                    d.vendor,
                    d.product,
                    d.serial.clone(),
                    d.firmware.clone(),
                    &d.path,
                );
                match host.api.connected(descriptor, d.path.clone()) {
                    Ok(connection) => {
                        d.id = connection.device_id.clone();
                        d.connection = Some(connection.clone());
                        readers
                            .lock()
                            .unwrap()
                            .insert(d.path.clone(), (d.reader.id, connection));
                        devices.insert(d.id.clone(), d);
                    }
                    Err(e) => host.api.log("warning", e.to_string()),
                }
            }
            // A newly bound discovered device can be opened on the next scan.
            scanned = Instant::now();
        }
        host.api.tick();
        if let Some(batch) = host.api.next_batch(&mut cursor) {
            if let Some(device) = devices.get_mut(&batch.connection.device_id) {
                execute_batch(device, batch, &host)?;
            } else {
                host.api
                    .complete(&batch.id, Err(anyhow::anyhow!(alchemy_device_api::UiFailure::new("backend.b321", "设备已断开"))));
            }
        }
        let mut failed = vec![];
        for (id, d) in &mut devices {
            if let Err(e) = d.tick() {
                host.api.log("warning", e.to_string());
                readers.lock().unwrap().remove(&d.path);
                if let Some(token) = &d.connection {
                    host.api.disconnected(token);
                }
                failed.push(id.clone());
            }
        }
        for id in failed {
            devices.remove(&id);
        }
        std::thread::sleep(Duration::from_millis(if devices.is_empty() {
            20
        } else {
            2
        }));
    }
    readers.lock().unwrap().clear();
    drop(devices); // joins all readers before Api's DLL is released.
    Ok(())
}
pub async fn session(host: AdapterHost, paths: &SdkPaths) -> Result<()> {
    let _owner = sdk::acquire_owner().await?;
    let (tx, mut rx) = mpsc::channel(256);
    let readers: ReaderConnections = Default::default();
    let h = host.clone();
    let p = paths.clone();
    let r = readers.clone();
    let mut worker = tokio::task::spawn_blocking(move || worker(h, p, tx, r));
    loop {
        tokio::select! {
            result=&mut worker=>return result?,
            event=rx.recv()=>{match event{
                Some(SdkEvent::Key{path,key,pressed,reader_id})=>{
                    let current=readers.lock().unwrap().get(&path).cloned();
                    if let Some((generation,connection))=current{if generation==reader_id{host.api.input(&connection,InputValue::Button{control_id:key.to_string(),pressed});}}
                },
                Some(SdkEvent::InputReport{header})=>host.api.log("sdk",header),None=>{},
            }},
            _=host.cancel.cancelled()=>break,
        }
    }
    worker.await?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_read_timeout_does_not_disconnect_but_other_errors_fail() {
        assert!(!read_has_data(0x05000302).unwrap());
        assert!(read_has_data(0).unwrap());
        assert!(read_has_data(0x02000201).is_err());
    }
    fn prepared(command: HardwareCommand) -> Prepared {
        Prepared {
            command,
            image: vec![],
            animation: None,
        }
    }
    #[test]
    fn page_has_one_commit_after_all_keys_but_single_key_commits_itself() {
        let mut page = vec![prepared(HardwareCommand::StopGif)];
        for key in 1..=15 {
            page.push(prepared(HardwareCommand::ClearGif { key }));
            page.push(prepared(HardwareCommand::KeyImage {
                key,
                asset_id: String::new(),
            }));
        }
        page.push(prepared(HardwareCommand::Refresh));
        let mut writes = Vec::new();
        submit_prepared(page, true, |p| match p.map(|p| p.command) {
            Some(HardwareCommand::KeyImage { key, .. }) => {
                writes.push(key);
                Ok(true)
            }
            Some(HardwareCommand::Refresh) | None => {
                writes.push(0);
                Ok(false)
            }
            _ => Ok(false),
        })
        .unwrap();
        assert_eq!(writes, (1..=15).chain([0]).collect::<Vec<_>>());
        let mut commits = 0;
        submit_prepared(
            vec![prepared(HardwareCommand::KeyImage {
                key: 1,
                asset_id: String::new(),
            })],
            false,
            |p| {
                if p.is_none() {
                    commits += 1;
                    Ok(false)
                } else {
                    Ok(true)
                }
            },
        )
        .unwrap();
        assert_eq!(commits, 1);
    }
    #[test]
    fn input_checks_header_state_and_all_eighteen_keys() {
        for (raw, key) in (1..=15)
            .map(|k| (k, k))
            .chain([(0x25, 16), (0x30, 17), (0x31, 18)])
        {
            let mut data = [0; 64];
            data[..3].copy_from_slice(b"ACK");
            data[5..7].copy_from_slice(b"OK");
            data[9] = raw;
            for (state, pressed) in [(0, false), (1, true), (2, false)] {
                data[10] = state;
                assert_eq!(input_event(&data), Some((key, pressed)));
                for prefix in [0, 4] {
                    let mut report = vec![prefix];
                    report.extend_from_slice(&data);
                    assert_eq!(input_event(&report), Some((key, pressed)));
                }
            }
            data[10] = 3;
            assert_eq!(input_event(&data), None);
        }
        assert_eq!(input_event(b"ACK"), None);
        assert_eq!(input_event(&[0; 64]), None);
    }
}
