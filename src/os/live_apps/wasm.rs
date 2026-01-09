use std::ffi::{CStr, CString};
use std::fs;
use std::os::raw::{c_char, c_void};
use std::path::PathBuf;
use std::ptr;
use std::time::Duration;

use esp_idf_hal::delay::TickType;
use esp_idf_hal::i2s::{I2sDriver, I2sTx};
use esp_idf_hal::io::Write;
use esp_idf_hal::sys::wifi_interface_t_WIFI_IF_STA;
use esp_idf_svc::espnow::{EspNow, PeerInfo};

use crate::keyboard::{key_code, key_event_code, Key, KeyEvent};
use crate::swapchain::DoubleBuffer;
use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};

use super::{tick_interval, LiveAppError, LiveAppOutcome};

type M3Result = *const c_char;
type IM3Environment = *mut c_void;
type IM3Runtime = *mut c_void;
type IM3Module = *mut c_void;
type IM3Function = *mut c_void;

const WASM_STACK_SIZE: u32 = 16 * 1024;
const FRAMEBUFFER_BYTES: usize = SCREEN_WIDTH * SCREEN_HEIGHT * 2;

extern "C" {
    fn m3_NewEnvironment() -> IM3Environment;
    fn m3_FreeEnvironment(environment: IM3Environment);
    fn m3_NewRuntime(environment: IM3Environment, stack_size: u32, userdata: *mut c_void) -> IM3Runtime;
    fn m3_FreeRuntime(runtime: IM3Runtime);
    fn m3_ParseModule(
        environment: IM3Environment,
        module: *mut IM3Module,
        wasm_bytes: *const u8,
        num_bytes: u32,
    ) -> M3Result;
    fn m3_FreeModule(module: IM3Module);
    fn m3_LoadModule(runtime: IM3Runtime, module: IM3Module) -> M3Result;
    fn m3_RunStart(module: IM3Module) -> M3Result;
    fn m3_FindFunction(function: *mut IM3Function, runtime: IM3Runtime, name: *const c_char) -> M3Result;
    fn m3_Call(function: IM3Function, argc: u32, argv: *const *const c_void) -> M3Result;
    fn m3_GetResults(function: IM3Function, retc: u32, rets: *const *const c_void) -> M3Result;
    fn m3_GetMemory(runtime: IM3Runtime, size: *mut u32, index: u32) -> *mut u8;
}

pub struct WasmApp {
    env: IM3Environment,
    runtime: IM3Runtime,
    _module: IM3Module,
    _wasm_bytes: Vec<u8>,
    func_init: Option<IM3Function>,
    func_update: IM3Function,
    func_render: Option<IM3Function>,
    func_fb_ptr: IM3Function,
    func_fb_len: IM3Function,
    func_should_exit: Option<IM3Function>,
    func_audio_ptr: Option<IM3Function>,
    func_audio_len: Option<IM3Function>,
    func_audio_clear: Option<IM3Function>,
    func_net_out_ptr: Option<IM3Function>,
    func_net_out_len: Option<IM3Function>,
    func_net_out_clear: Option<IM3Function>,
    func_net_peer_ptr: Option<IM3Function>,
    func_net_peer_epoch: Option<IM3Function>,
    speaker: Option<I2sDriver<'static, I2sTx>>,
    speaker_enabled: bool,
    espnow: Option<EspNow<'static>>,
    peer_epoch: Option<i32>,
    peer_addr: Option<[u8; 6]>,
}

impl WasmApp {
    pub fn load(path: PathBuf) -> Result<Self, LiveAppError> {
        let wasm_bytes = fs::read(&path)
            .map_err(|err| LiveAppError::LoadFailed(format!("read failed: {}", err)))?;

        let env = unsafe { m3_NewEnvironment() };
        if env.is_null() {
            return Err(LiveAppError::LoadFailed("env init failed".to_string()));
        }

        let runtime = unsafe { m3_NewRuntime(env, WASM_STACK_SIZE, ptr::null_mut()) };
        if runtime.is_null() {
            unsafe { m3_FreeEnvironment(env) };
            return Err(LiveAppError::LoadFailed("runtime init failed".to_string()));
        }

        let mut module: IM3Module = ptr::null_mut();
        let parse_result = unsafe {
            m3_ParseModule(
                env,
                &mut module as *mut IM3Module,
                wasm_bytes.as_ptr(),
                wasm_bytes.len() as u32,
            )
        };
        if let Some(err) = m3_result_to_string(parse_result) {
            unsafe { m3_FreeRuntime(runtime) };
            unsafe { m3_FreeEnvironment(env) };
            return Err(LiveAppError::LoadFailed(err));
        }

        let load_result = unsafe { m3_LoadModule(runtime, module) };
        if let Some(err) = m3_result_to_string(load_result) {
            unsafe { m3_FreeModule(module) };
            unsafe { m3_FreeRuntime(runtime) };
            unsafe { m3_FreeEnvironment(env) };
            return Err(LiveAppError::LoadFailed(err));
        }

        if let Some(err) = m3_result_to_string(unsafe { m3_RunStart(module) }) {
            unsafe { m3_FreeRuntime(runtime) };
            unsafe { m3_FreeEnvironment(env) };
            return Err(LiveAppError::LoadFailed(err));
        }

        let func_init = find_function(runtime, "app_init").ok();
        let func_update = find_function(runtime, "app_update")?;
        let func_render = find_function(runtime, "app_render").ok();
        let func_fb_ptr = find_function(runtime, "app_framebuffer_ptr")?;
        let func_fb_len = find_function(runtime, "app_framebuffer_len")?;
        let func_should_exit = find_function(runtime, "app_should_exit").ok();
        let func_audio_ptr = find_function(runtime, "app_audio_ptr").ok();
        let func_audio_len = find_function(runtime, "app_audio_len").ok();
        let func_audio_clear = find_function(runtime, "app_audio_clear").ok();
        let func_net_out_ptr = find_function(runtime, "app_network_out_ptr").ok();
        let func_net_out_len = find_function(runtime, "app_network_out_len").ok();
        let func_net_out_clear = find_function(runtime, "app_network_out_clear").ok();
        let func_net_peer_ptr = find_function(runtime, "app_network_peer_ptr").ok();
        let func_net_peer_epoch = find_function(runtime, "app_network_peer_epoch").ok();

        let app = Self {
            env,
            runtime,
            _module: module,
            _wasm_bytes: wasm_bytes,
            func_init,
            func_update,
            func_render,
            func_fb_ptr,
            func_fb_len,
            func_should_exit,
            func_audio_ptr,
            func_audio_len,
            func_audio_clear,
            func_net_out_ptr,
            func_net_out_len,
            func_net_out_clear,
            func_net_peer_ptr,
            func_net_peer_epoch,
            speaker: None,
            speaker_enabled: false,
            espnow: None,
            peer_epoch: None,
            peer_addr: None,
        };

        if let Some(func) = app.func_init {
            app.call_void(func, &[])?;
        }

        Ok(app)
    }

    pub fn tick(
        &mut self,
        buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
        input: Option<(KeyEvent, Key)>,
        dt: Duration,
    ) -> Result<LiveAppOutcome, LiveAppError> {
        let (key_code, key_event) = match input {
            Some((event, key)) => (key_code(key) as i32, key_event_code(event) as i32),
            None => (-1, 0),
        };

        let dt_ms = tick_interval(dt) as i32;
        let args = [dt_ms, key_code, key_event];
        self.call_void(self.func_update, &args)?;

        if let Some(func) = self.func_render {
            self.call_void(func, &[])?;
        }

        let mut mem_size = 0u32;
        let mem_ptr = unsafe { m3_GetMemory(self.runtime, &mut mem_size as *mut u32, 0) };
        if mem_ptr.is_null() {
            return Err(LiveAppError::RuntimeFailed("wasm memory missing".to_string()));
        }

        let mem_size = mem_size as usize;
        self.handle_network(mem_ptr, mem_size)?;
        self.handle_audio(mem_ptr, mem_size)?;

        if let Some(func) = self.func_should_exit {
            if self.call_i32(func)? != 0 {
                return Ok(LiveAppOutcome::Exit);
            }
        }

        let fb_ptr = self.call_i32(self.func_fb_ptr)? as usize;
        let fb_len = self.call_i32(self.func_fb_len)? as usize;
        if fb_len != FRAMEBUFFER_BYTES {
            return Err(LiveAppError::RuntimeFailed(format!(
                "framebuffer size mismatch: {}",
                fb_len
            )));
        }
        if fb_ptr % 2 != 0 {
            return Err(LiveAppError::RuntimeFailed(
                "framebuffer pointer unaligned".to_string(),
            ));
        }
        if fb_ptr + fb_len > mem_size {
            return Err(LiveAppError::RuntimeFailed(
                "framebuffer out of bounds".to_string(),
            ));
        }

        let fbuf = buffers.swap_framebuffer();
        let dst = unsafe {
            std::slice::from_raw_parts_mut(
                fbuf.framebuffer as *mut u16,
                SCREEN_WIDTH * SCREEN_HEIGHT,
            )
        };

        let src = unsafe {
            std::slice::from_raw_parts(
                mem_ptr.add(fb_ptr) as *const u16,
                SCREEN_WIDTH * SCREEN_HEIGHT,
            )
        };
        dst.copy_from_slice(src);

        buffers.send_framebuffer();

        Ok(LiveAppOutcome::Continue)
    }

    pub fn into_speaker(mut self) -> Option<I2sDriver<'static, I2sTx>> {
        self.speaker.take()
    }

    pub fn attach_speaker(&mut self, speaker: I2sDriver<'static, I2sTx>) {
        self.speaker = Some(speaker);
    }

    fn handle_audio(&mut self, mem_ptr: *mut u8, mem_size: usize) -> Result<(), LiveAppError> {
        let (Some(func_ptr), Some(func_len)) = (self.func_audio_ptr, self.func_audio_len) else {
            return Ok(());
        };

        let len = self.call_i32(func_len)?;
        if len <= 0 {
            return Ok(());
        }
        let ptr = self.call_i32(func_ptr)?;
        if ptr < 0 {
            return Err(LiveAppError::RuntimeFailed(
                "audio pointer out of bounds".to_string(),
            ));
        }
        let len = len as usize;
        let ptr = ptr as usize;
        if ptr + len > mem_size {
            return Err(LiveAppError::RuntimeFailed(
                "audio buffer out of bounds".to_string(),
            ));
        }

        let data = unsafe { std::slice::from_raw_parts(mem_ptr.add(ptr) as *const u8, len) };

        if let Some(speaker) = self.speaker.as_mut() {
            if !self.speaker_enabled {
                speaker
                    .tx_enable()
                    .map_err(|err| LiveAppError::RuntimeFailed(format!("speaker enable failed: {}", err)))?;
                self.speaker_enabled = true;
            }
            let timeout = TickType::new_millis(1000).into();
            speaker
                .write_all(data, timeout)
                .map_err(|err| LiveAppError::RuntimeFailed(format!("audio write failed: {}", err)))?;
        }

        if let Some(func_clear) = self.func_audio_clear {
            self.call_void(func_clear, &[])?;
        }

        Ok(())
    }

    fn handle_network(&mut self, mem_ptr: *mut u8, mem_size: usize) -> Result<(), LiveAppError> {
        if let (Some(func_peer_ptr), Some(func_peer_epoch)) =
            (self.func_net_peer_ptr, self.func_net_peer_epoch)
        {
            let epoch = self.call_i32(func_peer_epoch)?;
            if epoch > 0 && self.peer_epoch != Some(epoch) {
                let ptr = self.call_i32(func_peer_ptr)?;
                if ptr < 0 {
                    return Err(LiveAppError::RuntimeFailed(
                        "peer pointer out of bounds".to_string(),
                    ));
                }
                let ptr = ptr as usize;
                if ptr + 6 > mem_size {
                    return Err(LiveAppError::RuntimeFailed(
                        "peer buffer out of bounds".to_string(),
                    ));
                }
                let mut addr = [0u8; 6];
                let src =
                    unsafe { std::slice::from_raw_parts(mem_ptr.add(ptr) as *const u8, 6) };
                addr.copy_from_slice(src);
                self.peer_epoch = Some(epoch);
                self.peer_addr = Some(addr);
                self.ensure_espnow()?;
                if let Some(espnow) = self.espnow.as_ref() {
                    let peer_info = PeerInfo {
                        peer_addr: addr,
                        channel: 0,
                        ifidx: wifi_interface_t_WIFI_IF_STA,
                        ..Default::default()
                    };
                    let _ = espnow.add_peer(peer_info);
                }
            }
        }

        let (Some(func_out_ptr), Some(func_out_len)) =
            (self.func_net_out_ptr, self.func_net_out_len)
        else {
            return Ok(());
        };
        let out_len = self.call_i32(func_out_len)?;
        if out_len <= 0 {
            return Ok(());
        }
        let out_ptr = self.call_i32(func_out_ptr)?;
        if out_ptr < 0 {
            return Err(LiveAppError::RuntimeFailed(
                "network pointer out of bounds".to_string(),
            ));
        }
        let out_len = out_len as usize;
        let out_ptr = out_ptr as usize;
        if out_ptr + out_len > mem_size {
            return Err(LiveAppError::RuntimeFailed(
                "network buffer out of bounds".to_string(),
            ));
        }

        let payload =
            unsafe { std::slice::from_raw_parts(mem_ptr.add(out_ptr) as *const u8, out_len) };
        if let Some(peer) = self.peer_addr {
            self.ensure_espnow()?;
            if let Some(espnow) = self.espnow.as_ref() {
                espnow.send(peer, payload).map_err(|err| {
                    LiveAppError::RuntimeFailed(format!("espnow send failed: {}", err))
                })?;
            }
            if let Some(func_clear) = self.func_net_out_clear {
                self.call_void(func_clear, &[])?;
            }
        }

        Ok(())
    }

    fn ensure_espnow(&mut self) -> Result<(), LiveAppError> {
        if self.espnow.is_some() {
            return Ok(());
        }
        let espnow = EspNow::take()
            .map_err(|err| LiveAppError::RuntimeFailed(format!("espnow init failed: {}", err)))?;
        self.espnow = Some(espnow);
        Ok(())
    }

    fn call_void(&self, func: IM3Function, args: &[i32]) -> Result<(), LiveAppError> {
        let mut arg_ptrs: [*const c_void; 3] = [ptr::null(); 3];
        if args.len() > arg_ptrs.len() {
            return Err(LiveAppError::RuntimeFailed("too many args".to_string()));
        }
        for (idx, arg) in args.iter().enumerate() {
            arg_ptrs[idx] = arg as *const i32 as *const c_void;
        }
        let arg_ptrs = if args.is_empty() { ptr::null() } else { arg_ptrs.as_ptr() };
        let result = unsafe { m3_Call(func, args.len() as u32, arg_ptrs) };
        if let Some(err) = m3_result_to_string(result) {
            return Err(LiveAppError::RuntimeFailed(err));
        }
        Ok(())
    }

    fn call_i32(&self, func: IM3Function) -> Result<i32, LiveAppError> {
        let result = unsafe { m3_Call(func, 0, ptr::null()) };
        if let Some(err) = m3_result_to_string(result) {
            return Err(LiveAppError::RuntimeFailed(err));
        }

        let mut value: i32 = 0;
        let ret_ptrs = [&mut value as *mut i32 as *const c_void];
        let result = unsafe { m3_GetResults(func, 1, ret_ptrs.as_ptr()) };
        if let Some(err) = m3_result_to_string(result) {
            return Err(LiveAppError::RuntimeFailed(err));
        }
        Ok(value)
    }
}

impl Drop for WasmApp {
    fn drop(&mut self) {
        unsafe {
            m3_FreeRuntime(self.runtime);
            m3_FreeEnvironment(self.env);
        }
    }
}

fn find_function(runtime: IM3Runtime, name: &str) -> Result<IM3Function, LiveAppError> {
    let c_name = CString::new(name).map_err(|_| {
        LiveAppError::LoadFailed("function name contains null".to_string())
    })?;
    let mut func: IM3Function = ptr::null_mut();
    let result = unsafe { m3_FindFunction(&mut func as *mut IM3Function, runtime, c_name.as_ptr()) };
    if let Some(err) = m3_result_to_string(result) {
        return Err(LiveAppError::LoadFailed(err));
    }
    if func.is_null() {
        return Err(LiveAppError::LoadFailed("function missing".to_string()));
    }
    Ok(func)
}

fn m3_result_to_string(result: M3Result) -> Option<String> {
    if result.is_null() {
        None
    } else {
        let message = unsafe { CStr::from_ptr(result) };
        Some(message.to_string_lossy().to_string())
    }
}
