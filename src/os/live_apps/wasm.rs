use std::ffi::{CStr, CString};
use std::fs;
use std::os::raw::{c_char, c_void};
use std::path::PathBuf;
use std::ptr;
use std::time::Duration;

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

        let mut mem_size = 0u32;
        let mem_ptr = unsafe { m3_GetMemory(self.runtime, &mut mem_size as *mut u32, 0) };
        if mem_ptr.is_null() {
            return Err(LiveAppError::RuntimeFailed("wasm memory missing".to_string()));
        }

        let mem_size = mem_size as usize;
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
