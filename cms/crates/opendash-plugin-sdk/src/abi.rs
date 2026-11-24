use crate::types::SdkError;

pub const INITIAL_RESP_BUF_CAP: usize = 64 * 1024;
pub const MAX_IPC_FRAME_BYTES: usize = 2 * 1024 * 1024;
const BUFFER_TOO_SMALL: i32 = -2;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "opendash_v1")]
unsafe extern "C" {
    fn host_get_input_len() -> i32;
    fn host_read_input(dest_ptr: i32, max_len: i32) -> i32;
    fn host_write_output(src_ptr: i32, len: i32) -> i32;
    fn host_call(op_ptr: i32, op_len: i32, resp_ptr: i32, max_resp_len: i32) -> i32;
    fn host_last_resp_len() -> i32;
    fn host_read_last_resp(dest_ptr: i32, max_len: i32) -> i32;
}

/// Reads raw UTF-8 JSON envelope bytes supplied by the OpenDash host.
pub fn read_input_bytes() -> Result<Vec<u8>, SdkError> {
    #[cfg(target_arch = "wasm32")]
    let raw_len = unsafe { host_get_input_len() };
    #[cfg(not(target_arch = "wasm32"))]
    let raw_len = crate::mock::abi_get_input_len()?;

    if raw_len < 0 {
        return Err(SdkError::HostError(format!(
            "host_get_input_len returned {raw_len}"
        )));
    }
    let len = raw_len as usize;
    if len == 0 {
        return Ok(Vec::new());
    }
    if len > MAX_IPC_FRAME_BYTES {
        return Err(SdkError::InputTooLarge(len));
    }

    let mut buf = vec![0u8; len];
    #[cfg(target_arch = "wasm32")]
    let read_len = unsafe { host_read_input(buf.as_mut_ptr() as i32, raw_len) };
    #[cfg(not(target_arch = "wasm32"))]
    let read_len = crate::mock::abi_read_input(&mut buf)?;

    if read_len < 0 || (read_len as usize) > len {
        return Err(SdkError::HostError(format!(
            "host_read_input returned {read_len}"
        )));
    }
    buf.truncate(read_len as usize);
    Ok(buf)
}

/// Writes UTF-8 JSON output bytes back to the OpenDash host.
pub fn write_output_bytes(bytes: &[u8]) -> Result<(), SdkError> {
    if bytes.len() > 1024 * 1024 {
        return Err(SdkError::InputTooLarge(bytes.len()));
    }
    #[cfg(target_arch = "wasm32")]
    let code = unsafe { host_write_output(bytes.as_ptr() as i32, bytes.len() as i32) };
    #[cfg(not(target_arch = "wasm32"))]
    let code = crate::mock::abi_write_output(bytes)?;

    if code < 0 {
        return Err(SdkError::HostError(format!(
            "host_write_output returned {code}"
        )));
    }
    Ok(())
}

/// Invokes a Host Op using ABI v1.1, transparently reading oversized responses
/// when the host returns `-2` (`BUFFER_TOO_SMALL`) via `host_last_resp_len` and `host_read_last_resp`.
pub fn host_call_raw(req_json: &[u8]) -> Result<Vec<u8>, SdkError> {
    if req_json.len() > MAX_IPC_FRAME_BYTES {
        return Err(SdkError::InputTooLarge(req_json.len()));
    }

    #[cfg(target_arch = "wasm32")]
    let init_cap = INITIAL_RESP_BUF_CAP;
    #[cfg(not(target_arch = "wasm32"))]
    let init_cap = crate::mock::abi_initial_resp_buf_cap()?;

    let cap = init_cap.clamp(1, MAX_IPC_FRAME_BYTES);
    let mut buf = vec![0u8; cap];

    #[cfg(target_arch = "wasm32")]
    let resp_len = unsafe {
        host_call(
            req_json.as_ptr() as i32,
            req_json.len() as i32,
            buf.as_mut_ptr() as i32,
            cap as i32,
        )
    };
    #[cfg(not(target_arch = "wasm32"))]
    let resp_len = crate::mock::abi_host_call(req_json, &mut buf)?;

    if resp_len > 0 {
        let len = resp_len as usize;
        if len > cap {
            return Err(SdkError::UnexpectedResponse(format!(
                "host_call returned length {len} exceeding buffer {cap}"
            )));
        }
        buf.truncate(len);
        return Ok(buf);
    }

    if resp_len == 0 {
        return Err(SdkError::UnexpectedResponse(
            "host_call returned empty response".into(),
        ));
    }

    if resp_len == BUFFER_TOO_SMALL {
        #[cfg(target_arch = "wasm32")]
        let full_len = unsafe { host_last_resp_len() };
        #[cfg(not(target_arch = "wasm32"))]
        let full_len = crate::mock::abi_host_last_resp_len()?;

        if full_len <= 0 || (full_len as usize) > MAX_IPC_FRAME_BYTES {
            return Err(SdkError::UnexpectedResponse(format!(
                "invalid host_last_resp_len: {full_len}"
            )));
        }
        let target_len = full_len as usize;
        let mut full_buf = vec![0u8; target_len];

        #[cfg(target_arch = "wasm32")]
        let copied = unsafe { host_read_last_resp(full_buf.as_mut_ptr() as i32, full_len) };
        #[cfg(not(target_arch = "wasm32"))]
        let copied = crate::mock::abi_host_read_last_resp(&mut full_buf)?;

        if copied <= 0 || (copied as usize) > target_len {
            return Err(SdkError::HostError(format!(
                "host_read_last_resp returned {copied}"
            )));
        }
        full_buf.truncate(copied as usize);
        return Ok(full_buf);
    }

    Err(SdkError::HostError(format!(
        "host_call failed with code {resp_len}"
    )))
}
