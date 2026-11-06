// AssemblyScript host imports described in docs/contracts/PLUGIN_ABI.md.
// These declarations are only an SDK starting point, not a full guest runtime.

@external("opendash_v1", "host_get_input_len")
export declare function host_get_input_len(): i32;

@external("opendash_v1", "host_read_input")
export declare function host_read_input(dest_ptr: i32, max_len: i32): i32;

@external("opendash_v1", "host_write_output")
export declare function host_write_output(src_ptr: i32, len: i32): i32;

@external("opendash_v1", "host_call")
export declare function host_call(
  op_ptr: i32,
  op_len: i32,
  resp_ptr: i32,
  max_resp_len: i32,
): i32;
