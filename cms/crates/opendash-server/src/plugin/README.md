# Plugin host — module dự kiến

Plugin server chạy bằng Wasmi 2.0.0, với sandbox, quyền và giới hạn theo [Plugin ABI v1](../../../../../docs/contracts/PLUGIN_ABI.md). Module ở repository M1/M2 nguồn sẽ được đối chiếu và nhập tại đây.

Scaffold hiện tại **không** cài, gọi hay thực thi WebAssembly plugin; không expose route plugin API cho đến khi có xác thực và kiểm chứng quyền.
