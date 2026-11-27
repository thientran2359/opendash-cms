use super::*;

pub(crate) fn register_host_imports(linker: &mut Linker<WasmStoreData>) -> Result<(), WasmiError> {
    // 1. host_get_input_len() -> i32
    linker.func_wrap(
        "opendash_v1",
        "host_get_input_len",
        |mut caller: Caller<'_, WasmStoreData>| -> Result<i32, wasmi::Error> {
            if caller.data().host.is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            Ok(caller.data().host.input_bytes.len() as i32)
        },
    )?;

    // 2. host_read_input(dest_ptr: i32, max_len: i32) -> i32
    linker.func_wrap(
        "opendash_v1",
        "host_read_input",
        |mut caller: Caller<'_, WasmStoreData>,
         dest_ptr: i32,
         max_len: i32|
         -> Result<i32, wasmi::Error> {
            if caller.data().host.is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            if dest_ptr < 0 || max_len < 0 {
                return Ok(-1);
            }
            let dest = dest_ptr as usize;
            let max = max_len as usize;
            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(-1);
            };

            let input = caller.data().host.input_bytes.clone();
            let to_copy = std::cmp::min(max, input.len());
            let Some(end) = dest.checked_add(to_copy) else {
                return Ok(-1);
            };
            if end > memory.data_size(&caller) {
                return Ok(-1);
            }

            if memory.write(&mut caller, dest, &input[..to_copy]).is_err() {
                return Ok(-1);
            }
            Ok(to_copy as i32)
        },
    )?;

    // 3. host_write_output(src_ptr: i32, len: i32) -> i32
    linker.func_wrap(
        "opendash_v1",
        "host_write_output",
        |mut caller: Caller<'_, WasmStoreData>,
         src_ptr: i32,
         len: i32|
         -> Result<i32, wasmi::Error> {
            if caller.data().host.is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            if src_ptr < 0 || len < 0 {
                return Ok(-1);
            }
            let src = src_ptr as usize;
            let length = len as usize;
            if length > 1024 * 1024 {
                return Ok(-2);
            }
            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(-1);
            };
            let Some(end) = src.checked_add(length) else {
                return Ok(-1);
            };
            if end > memory.data_size(&caller) {
                return Ok(-1);
            }

            let mut buf = vec![0u8; length];
            if memory.read(&caller, src, &mut buf).is_err() {
                return Ok(-1);
            }

            let Ok(text) = std::str::from_utf8(&buf) else {
                return Ok(-3);
            };
            if serde_json::from_str::<Value>(text).is_err() {
                return Ok(-4);
            }

            caller.data_mut().host.output_bytes = Some(buf);
            Ok(0)
        },
    )?;

    // 4. host_call(op_ptr: i32, op_len: i32, resp_ptr: i32, max_resp_len: i32) -> i32
    linker.func_wrap(
        "opendash_v1",
        "host_call",
        |mut caller: Caller<'_, WasmStoreData>,
         op_ptr: i32,
         op_len: i32,
         resp_ptr: i32,
         max_resp_len: i32|
         -> Result<i32, wasmi::Error> {
            if caller.data().host.is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            if op_ptr < 0 || op_len < 0 || resp_ptr < 0 || max_resp_len < 0 {
                return Ok(-1);
            }
            let op_src = op_ptr as usize;
            let op_length = op_len as usize;
            let resp_dest = resp_ptr as usize;
            let resp_max = max_resp_len as usize;

            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(-1);
            };
            let Some(op_end) = op_src.checked_add(op_length) else {
                return Ok(-1);
            };
            if op_end > memory.data_size(&caller) {
                return Ok(-1);
            }

            let mut op_buf = vec![0u8; op_length];
            if memory.read(&caller, op_src, &mut op_buf).is_err() {
                return Ok(-1);
            }

            let Ok(op_str) = std::str::from_utf8(&op_buf) else {
                return Ok(-1);
            };

            caller.data_mut().host.last_resp_bytes = None;
            let resp = host_ops::handle_host_call(&mut caller.data_mut().host, op_str);

            if caller.data().host.is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }

            let resp_bytes = serde_json::to_vec(&resp)
                .unwrap_or_else(|_| b"{\"ok\":false,\"error\":\"Internal JSON error\"}".to_vec());

            if resp_bytes.len() > MAX_IPC_FRAME_BYTES {
                return Ok(-1);
            }
            if resp_bytes.len() > resp_max {
                caller.data_mut().host.last_resp_bytes = Some(resp_bytes);
                return Ok(-2);
            }

            let Some(resp_end) = resp_dest.checked_add(resp_bytes.len()) else {
                return Ok(-1);
            };
            if resp_end > memory.data_size(&caller) {
                return Ok(-1);
            }

            if memory.write(&mut caller, resp_dest, &resp_bytes).is_err() {
                return Ok(-1);
            }
            Ok(resp_bytes.len() as i32)
        },
    )?;

    // 5. host_last_resp_len() -> i32 (ABI v1.1)
    linker.func_wrap(
        "opendash_v1",
        "host_last_resp_len",
        |mut caller: Caller<'_, WasmStoreData>| -> Result<i32, wasmi::Error> {
            if caller.data().host.is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            Ok(caller
                .data()
                .host
                .last_resp_bytes
                .as_ref()
                .map(|b| b.len() as i32)
                .unwrap_or(0))
        },
    )?;

    // 6. host_read_last_resp(dest_ptr: i32, max_len: i32) -> i32 (ABI v1.1)
    linker.func_wrap(
        "opendash_v1",
        "host_read_last_resp",
        |mut caller: Caller<'_, WasmStoreData>,
         dest_ptr: i32,
         max_len: i32|
         -> Result<i32, wasmi::Error> {
            if caller.data().host.is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            if dest_ptr < 0 || max_len < 0 {
                return Ok(-1);
            }
            let Some(bytes) = caller.data().host.last_resp_bytes.clone() else {
                return Ok(-1);
            };
            let dest = dest_ptr as usize;
            let max = max_len as usize;
            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(-1);
            };
            let to_copy = std::cmp::min(max, bytes.len());
            let Some(end) = dest.checked_add(to_copy) else {
                return Ok(-1);
            };
            if end > memory.data_size(&caller) {
                return Ok(-1);
            }
            if memory.write(&mut caller, dest, &bytes[..to_copy]).is_err() {
                return Ok(-1);
            }
            if to_copy == bytes.len() {
                caller.data_mut().host.last_resp_bytes = None;
            }
            Ok(to_copy as i32)
        },
    )?;

    linker.func_wrap(
        "env",
        "abort",
        |mut caller: Caller<'_, WasmStoreData>,
         _msg: i32,
         _file: i32,
         _line: i32,
         _col: i32|
         -> Result<(), wasmi::Error> {
            let _ = caller.as_context_mut().set_fuel(0);
            Err(wasmi::Error::new("Guest aborted via env.abort"))
        },
    )?;

    Ok(())
}

#[cfg(test)]
pub(crate) static TEST_IPC_KV_GET_WAITING: AtomicBool = AtomicBool::new(false);
