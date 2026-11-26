//! Admin API error messages. Translate only known system literals or anchored
//! templates; opaque plugin messages and interpolated user data stay untouched.

#[derive(Debug, PartialEq, Eq)]
pub struct LocalizedError {
    pub code: &'static str,
    pub message: String,
}

pub fn encode_ui_locale(configured: &str) -> u8 {
    if configured.trim().eq_ignore_ascii_case("vi") {
        2
    } else {
        1
    }
}

/// Unknown messages must remain byte-for-byte unchanged; the middleware can
/// distinguish them from a recognized system error using this sentinel.
pub const UNKNOWN_ERROR_CODE: &str = "server.unknown";

pub fn translate_api_message(raw_msg: &str, effective_ui_locale: &str) -> LocalizedError {
    let vietnamese = encode_ui_locale(effective_ui_locale) == 2;
    for &(code, vi, en) in EXACT_ERROR_ENTRIES {
        if raw_msg == vi || raw_msg == en {
            return LocalizedError {
                code,
                message: if vietnamese { vi } else { en }.to_owned(),
            };
        }
    }
    for &(code, vi, en) in TEMPLATE_MATCHERS {
        let (target, source) = if vietnamese { (vi, en) } else { (en, vi) };
        // A message that is already in the requested language needs no rewrite.
        if capture_template(raw_msg, target).is_some() {
            return LocalizedError {
                code,
                message: raw_msg.to_owned(),
            };
        }
        if let Some(values) = capture_template(raw_msg, source) {
            let mut message = String::from(target[0]);
            for (value, suffix) in values.into_iter().zip(&target[1..]) {
                message.push_str(value);
                message.push_str(suffix);
            }
            return LocalizedError { code, message };
        }
    }
    LocalizedError {
        code: UNKNOWN_ERROR_CODE,
        message: raw_msg.to_owned(),
    }
}

fn capture_template<'a>(raw: &'a str, pieces: &[&str]) -> Option<Vec<&'a str>> {
    let rest = raw.strip_prefix(pieces[0])?;
    let mut rest = rest.strip_suffix(*pieces.last()?)?;
    let mut values = Vec::with_capacity(pieces.len() - 1);
    for delimiter in &pieces[1..pieces.len() - 1] {
        // Consecutive unconstrained substitutions have no unambiguous boundary.
        if delimiter.is_empty() {
            return None;
        }
        let (value, remainder) = rest.split_once(delimiter)?;
        values.push(value);
        rest = remainder;
    }
    values.push(rest);
    Some(values)
}

const EXACT_ERROR_ENTRIES: &[(&str, &str, &str)] = &[
    (
        "request.invalid_body",
        "Body của DELETE locale phải rỗng.",
        "DELETE locale body must be empty.",
    ),
    (
        "multilingual.disabled",
        "Hỗ trợ đa ngôn ngữ đang tắt.",
        "Multilingual support is disabled.",
    ),
    (
        "locale.cannot_delete_default",
        "Không thể xóa ngôn ngữ mặc định.",
        "Cannot delete the default language.",
    ),
    (
        "locale.not_deleted",
        "Ngôn ngữ chưa bị xóa.",
        "The language has not been deleted.",
    ),
    (
        "locale.purge_confirm_mismatch",
        "Mã xác nhận xóa vĩnh viễn không khớp.",
        "The purge confirmation code does not match.",
    ),
    (
        "locale.orphaned_conflict",
        "Mã ngôn ngữ đang có dữ liệu mồ côi.",
        "The language code has orphaned data.",
    ),
    (
        "server.error",
        "Không thể điều phối tác vụ băm mật khẩu.",
        "Could not schedule password hashing.",
    ),
    (
        "server.error",
        "Lỗi luồng xử lý mật khẩu.",
        "Password processing thread failed.",
    ),
    ("auth.unauthorized", "Cần đăng nhập.", "Sign-in required."),
    (
        "auth.unauthorized",
        "Phiên đăng nhập không hợp lệ hoặc đã hết hạn.",
        "The session is invalid or has expired.",
    ),
    (
        "auth.forbidden",
        "CSRF token không hợp lệ hoặc bị thiếu.",
        "The CSRF token is invalid or missing.",
    ),
    (
        "auth.forbidden",
        "Chỉ quản trị viên mới có quyền thực hiện thao tác này.",
        "Only administrators can perform this action.",
    ),
    (
        "server.error",
        "Không thể tuần tự hóa yêu cầu snapshot worker.",
        "Could not serialize the snapshot worker request.",
    ),
    (
        "server.error",
        "Không xác định được đường dẫn thực thi.",
        "Could not determine the executable path.",
    ),
    (
        "server.error",
        "Không khởi chạy được operations worker.",
        "Could not start the operations worker.",
    ),
    (
        "server.error",
        "Không gửi được yêu cầu tới operations worker.",
        "Could not send the request to the operations worker.",
    ),
    (
        "backup.the_backup_job_was_cancelled",
        "Tác vụ backup đã bị hủy.",
        "The backup job was cancelled.",
    ),
    (
        "server.error",
        "Không đọc được kết quả worker.",
        "Could not read the worker result.",
    ),
    (
        "backup.the_snapshot_job_timed_out_and_was_stopped",
        "Tác vụ snapshot vượt quá thời gian cho phép và đã bị dừng.",
        "The snapshot job timed out and was stopped.",
    ),
    (
        "server.error",
        "Lỗi theo dõi tiến trình worker.",
        "Error monitoring the worker process.",
    ),
    (
        "server.error",
        "Lỗi join tác vụ snapshot worker.",
        "Could not join the snapshot worker task.",
    ),
    (
        "server.error",
        "Phản hồi từ operations worker không hợp lệ hoặc bị ngắt.",
        "The operations worker response was invalid or interrupted.",
    ),
    (
        "server.error",
        "Không đọc được manifest.json sau snapshot.",
        "Could not read manifest.json after the snapshot.",
    ),
    (
        "server.error",
        "manifest.json không hợp lệ.",
        "Invalid manifest.json.",
    ),
    (
        "backup.the_backup_is_being_downloaded_please_try_again_later",
        "Bản sao lưu đang được tải xuống; vui lòng thử lại sau.",
        "The backup is being downloaded; please try again later.",
    ),
    (
        "server.error",
        "Không thể tuần tự hóa yêu cầu archive worker.",
        "Could not serialize the archive worker request.",
    ),
    (
        "server.error",
        "Không khởi chạy được archive worker.",
        "Could not start the archive worker.",
    ),
    (
        "server.error",
        "Thiếu stdin pipe cho archive worker.",
        "The archive worker is missing its stdin pipe.",
    ),
    (
        "server.error",
        "Không gửi được yêu cầu tới archive worker.",
        "Could not send the request to the archive worker.",
    ),
    (
        "server.error",
        "Không đọc được kết quả archive worker.",
        "Could not read the archive worker result.",
    ),
    (
        "backup.the_backup_encryption_job_timed_out_and_was_stopped",
        "Tác vụ mã hóa backup vượt quá thời gian cho phép và đã bị dừng.",
        "The backup encryption job timed out and was stopped.",
    ),
    (
        "server.error",
        "Lỗi theo dõi tiến trình archive worker.",
        "Error monitoring the archive worker process.",
    ),
    (
        "server.error",
        "Lỗi join tác vụ archive worker.",
        "Could not join the archive worker task.",
    ),
    (
        "server.error",
        "Phản hồi từ archive worker không hợp lệ hoặc bị ngắt.",
        "The archive worker response was invalid or interrupted.",
    ),
    (
        "server.error",
        "Archive worker thiếu archive_size.",
        "The archive worker did not return archive_size.",
    ),
    (
        "server.error",
        "Archive worker thiếu archive_sha256.",
        "The archive worker did not return archive_sha256.",
    ),
    (
        "backup.the_backup_password_must_contain_16_to_128_unicode_characters",
        "Mật khẩu backup phải có từ 16 đến 128 ký tự Unicode.",
        "The backup password must contain 16 to 128 Unicode characters.",
    ),
    (
        "backup.the_backup_password_exceeds_512_utf_8_bytes",
        "Mật khẩu backup vượt quá 512 byte UTF-8.",
        "The backup password exceeds 512 UTF-8 bytes.",
    ),
    (
        "backup.invalid_output_backup_file_path",
        "Đường dẫn tệp backup đầu ra không hợp lệ.",
        "Invalid output backup file path.",
    ),
    (
        "backup.the_output_backup_file_already_exists",
        "Tệp backup đầu ra đã tồn tại.",
        "The output backup file already exists.",
    ),
    (
        "server.error",
        "Không mở được payload snapshot để đóng gói.",
        "Could not open the snapshot payload for packaging.",
    ),
    (
        "server.error",
        "Không ghi được entry payload vào backup ZIP.",
        "Could not write the payload entry to the backup ZIP.",
    ),
    (
        "server.error",
        "Lỗi đọc payload snapshot.",
        "Error reading the snapshot payload.",
    ),
    (
        "server.error",
        "Tràn kích thước payload.",
        "Payload size overflow.",
    ),
    (
        "server.error",
        "Lỗi ghi payload vào backup ZIP.",
        "Error writing the payload to the backup ZIP.",
    ),
    (
        "backup.the_snapshot_payload_changed_or_does_not_match_the_manifest",
        "Payload snapshot thay đổi hoặc không khớp manifest.",
        "The snapshot payload changed or does not match the manifest.",
    ),
    (
        "server.error",
        "Không đọc được manifest.json của snapshot.",
        "Could not read the snapshot manifest.json.",
    ),
    (
        "backup.manifest_json_exceeds_8_mib",
        "manifest.json vượt quá 8 MiB.",
        "manifest.json exceeds 8 MiB.",
    ),
    (
        "server.error",
        "Không ghi được manifest.json vào backup ZIP.",
        "Could not write manifest.json to the backup ZIP.",
    ),
    (
        "server.error",
        "Lỗi ghi manifest.json vào backup ZIP.",
        "Error writing manifest.json to the backup ZIP.",
    ),
    (
        "server.error",
        "Không hoàn tất được backup ZIP.",
        "Could not finalize the backup ZIP.",
    ),
    (
        "server.error",
        "Lỗi flush backup ZIP.",
        "Error flushing the backup ZIP.",
    ),
    (
        "server.error",
        "Lỗi hoàn tất buffer backup ZIP.",
        "Error finalizing the backup ZIP buffer.",
    ),
    (
        "server.error",
        "Lỗi fsync backup ZIP.",
        "Error syncing the backup ZIP to disk.",
    ),
    (
        "server.error",
        "Không đọc được kích thước backup ZIP.",
        "Could not read the backup ZIP size.",
    ),
    (
        "backup.the_total_backup_zip_size_exceeds_4_gib",
        "Tổng dung lượng backup ZIP vượt quá 4 GiB.",
        "The total backup ZIP size exceeds 4 GiB.",
    ),
    (
        "server.error",
        "Không khởi tạo được age encryptor.",
        "Could not initialize the age encryptor.",
    ),
    (
        "server.error",
        "Không mở được luồng mã hóa age.",
        "Could not open the age encryption stream.",
    ),
    (
        "server.error",
        "Không mở lại được backup ZIP để mã hóa.",
        "Could not reopen the backup ZIP for encryption.",
    ),
    (
        "server.error",
        "Lỗi đọc backup ZIP để mã hóa.",
        "Error reading the backup ZIP for encryption.",
    ),
    (
        "server.error",
        "Lỗi ghi luồng mã hóa age.",
        "Error writing the age encryption stream.",
    ),
    (
        "server.error",
        "Không hoàn tất được thẻ xác thực age AEAD.",
        "Could not finalize the age AEAD authentication tag.",
    ),
    (
        "server.error",
        "Lỗi flush tệp mã hóa age.",
        "Error flushing the age encrypted file.",
    ),
    (
        "server.error",
        "Lỗi hoàn tất buffer tệp mã hóa age.",
        "Error finalizing the age encrypted file buffer.",
    ),
    (
        "server.error",
        "Lỗi fsync tệp mã hóa age.",
        "Error syncing the age encrypted file.",
    ),
    (
        "server.error",
        "Không đổi tên hoàn tất tệp backup đã mã hóa.",
        "Could not rename the completed encrypted backup.",
    ),
    (
        "backup.could_not_read_the_backup_package_header",
        "Không đọc được header gói backup.",
        "Could not read the backup package header.",
    ),
    (
        "backup.the_backup_age_header_is_invalid_or_exceeds_8_kib",
        "Header age của gói backup không hợp lệ hoặc vượt quá 8 KiB.",
        "The backup age header is invalid or exceeds 8 KiB.",
    ),
    (
        "backup.the_backup_age_header_exceeds_8_kib",
        "Header age của gói backup vượt quá 8 KiB.",
        "The backup age header exceeds 8 KiB.",
    ),
    (
        "backup.the_age_header_is_not_valid_utf_8",
        "Header age không phải UTF-8 hợp lệ.",
        "The age header is not valid UTF-8.",
    ),
    (
        "backup.invalid_age_header_version",
        "Phiên bản header age không hợp lệ.",
        "Invalid age header version.",
    ),
    (
        "backup.the_age_header_is_missing_the_scrypt_stanza",
        "Thiếu stanza scrypt trong header age.",
        "The age header is missing the scrypt stanza.",
    ),
    (
        "backup.the_backup_package_supports_exactly_one_scrypt_stanza",
        "Gói backup chỉ hỗ trợ đúng một stanza scrypt.",
        "The backup package supports exactly one scrypt stanza.",
    ),
    (
        "backup.invalid_scrypt_salt_in_the_age_header",
        "Salt scrypt trong header age không hợp lệ.",
        "Invalid scrypt salt in the age header.",
    ),
    (
        "backup.invalid_scrypt_work_factor",
        "Work factor scrypt không hợp lệ.",
        "Invalid scrypt work factor.",
    ),
    (
        "backup.the_scrypt_work_factor_exceeds_the_allowed_limit",
        "Work factor scrypt vượt quá giới hạn cho phép.",
        "The scrypt work factor exceeds the allowed limit.",
    ),
    (
        "backup.missing_scrypt_stanza_body_in_the_age_header",
        "Thiếu thân stanza scrypt trong header age.",
        "Missing scrypt stanza body in the age header.",
    ),
    (
        "backup.invalid_scrypt_stanza_body",
        "Thân stanza scrypt không hợp lệ.",
        "Invalid scrypt stanza body.",
    ),
    (
        "backup.missing_mac_line_in_the_age_header",
        "Thiếu dòng MAC trong header age.",
        "Missing MAC line in the age header.",
    ),
    (
        "backup.the_age_header_contains_multiple_stanzas_or_an_invalid_structure",
        "Header age chứa nhiều stanza hoặc cấu trúc không hợp lệ.",
        "The age header contains multiple stanzas or an invalid structure.",
    ),
    (
        "backup.could_not_rewind_the_backup_package",
        "Không tua lại được gói backup.",
        "Could not rewind the backup package.",
    ),
    (
        "backup.could_not_read_backup_package_information",
        "Không đọc được thông tin gói backup.",
        "Could not read backup package information.",
    ),
    (
        "backup.the_backup_package_size_is_invalid_or_exceeds_the_limit",
        "Kích thước gói backup không hợp lệ hoặc vượt quá giới hạn.",
        "The backup package size is invalid or exceeds the limit.",
    ),
    (
        "backup.could_not_open_the_backup_package",
        "Không mở được gói backup.",
        "Could not open the backup package.",
    ),
    (
        "backup.could_not_decrypt_or_authenticate_the_backup_package",
        "Không thể giải mã hoặc xác thực gói backup.",
        "Could not decrypt or authenticate the backup package.",
    ),
    (
        "backup.the_decrypted_backup_package_exceeds_4_gib",
        "Gói backup giải mã vượt quá 4 GiB.",
        "The decrypted backup package exceeds 4 GiB.",
    ),
    (
        "server.error",
        "Không ghi được ZIP giải mã vào staging.",
        "Could not write the decrypted ZIP to staging.",
    ),
    (
        "server.error",
        "Không fsync được ZIP giải mã.",
        "Could not sync the decrypted ZIP to disk.",
    ),
    (
        "server.error",
        "Không mở lại được ZIP giải mã.",
        "Could not reopen the decrypted ZIP.",
    ),
    (
        "backup.the_backup_zip_is_missing_manifest_json",
        "Backup ZIP thiếu manifest.json.",
        "The backup ZIP is missing manifest.json.",
    ),
    (
        "backup.invalid_manifest_json_in_the_backup",
        "manifest.json trong backup không hợp lệ.",
        "Invalid manifest.json in the backup.",
    ),
    (
        "backup.the_number_of_entries_in_the_backup_zip_does_not_match_manifest_json",
        "Số lượng entry trong backup ZIP không khớp manifest.json.",
        "The number of entries in the backup ZIP does not match manifest.json.",
    ),
    (
        "backup.manifest_json_contains_duplicate_entry_name_values",
        "manifest.json chứa entry_name trùng lặp.",
        "manifest.json contains duplicate entry_name values.",
    ),
    (
        "backup.could_not_read_an_entry_in_the_backup_zip",
        "Không đọc được entry trong backup ZIP.",
        "Could not read an entry in the backup ZIP.",
    ),
    (
        "backup.the_backup_zip_contains_duplicate_entries",
        "Backup ZIP chứa entry trùng lặp.",
        "The backup ZIP contains duplicate entries.",
    ),
    (
        "backup.the_backup_zip_contains_an_extra_entry_not_listed_in_the_manifest",
        "Backup ZIP chứa entry thừa không có trong manifest.",
        "The backup ZIP contains an extra entry not listed in the manifest.",
    ),
    (
        "backup.error_reading_the_payload_from_the_backup_zip",
        "Lỗi đọc payload từ backup ZIP.",
        "Error reading the payload from the backup ZIP.",
    ),
    (
        "backup.payload_size_overflowed",
        "Kích thước payload bị tràn.",
        "Payload size overflowed.",
    ),
    (
        "backup.the_backup_payload_exceeds_512_mib",
        "Payload backup vượt quá giới hạn 512 MiB.",
        "The backup payload exceeds 512 MiB.",
    ),
    (
        "server.error",
        "Không ghi được payload vào staging.",
        "Could not write the payload to staging.",
    ),
    (
        "server.error",
        "Không fsync được payload vào staging.",
        "Could not sync the payload in staging to disk.",
    ),
    (
        "backup.the_payload_checksum_or_size_does_not_match_manifest_json",
        "Checksum hoặc kích thước payload không khớp manifest.json.",
        "The payload checksum or size does not match manifest.json.",
    ),
    (
        "backup.symlink_or_non_file_object_rejected",
        "Từ chối symlink hoặc đối tượng không phải tệp.",
        "Symlink or non-file object rejected.",
    ),
    (
        "backup.could_not_open_the_source_file_while_creating_the_snapshot",
        "Không thể mở tệp nguồn khi tạo snapshot.",
        "Could not open the source file while creating the snapshot.",
    ),
    (
        "server.error",
        "Lỗi đọc tệp khi tạo snapshot.",
        "Error reading a file while creating the snapshot.",
    ),
    (
        "server.error",
        "Lỗi ghi tệp snapshot (có thể đầy đĩa).",
        "Error writing a snapshot file (the disk may be full).",
    ),
    (
        "server.error",
        "Không thể fsync tệp snapshot.",
        "Could not sync the snapshot file to disk.",
    ),
    (
        "server.error",
        "Không mở được tệp DB snapshot để băm.",
        "Could not open the snapshot database for hashing.",
    ),
    (
        "server.error",
        "Lỗi đọc DB snapshot.",
        "Error reading the snapshot database.",
    ),
    (
        "server.error",
        "Worker không mở được SQLite nguồn.",
        "The worker could not open the source SQLite database.",
    ),
    (
        "backup.could_not_read_the_quarantine_directory",
        "Không đọc được thư mục quarantine.",
        "Could not read the quarantine directory.",
    ),
    (
        "backup.error_reading_a_quarantine_file",
        "Lỗi đọc tệp quarantine.",
        "Error reading a quarantine file.",
    ),
    (
        "backup.could_not_read_uploads_media",
        "Không đọc được uploads/media.",
        "Could not read uploads/media.",
    ),
    (
        "backup.error_reading_a_shard_in_uploads_media",
        "Lỗi đọc shard trong uploads/media.",
        "Error reading a shard in uploads/media.",
    ),
    (
        "backup.unexpected_object_found_in_uploads_media",
        "Phát hiện đối tượng lạ trong uploads/media.",
        "Unexpected object found in uploads/media.",
    ),
    (
        "backup.could_not_read_a_shard_in_uploads_media",
        "Không đọc được shard uploads/media.",
        "Could not read a shard in uploads/media.",
    ),
    (
        "backup.error_reading_a_file_in_uploads_media",
        "Lỗi đọc tệp trong uploads/media.",
        "Error reading a file in uploads/media.",
    ),
    (
        "backup.invalid_file_found_in_uploads_media_reconcile_first",
        "Phát hiện tệp không hợp lệ trong uploads/media; cần reconcile trước.",
        "Invalid file found in uploads/media; reconcile first.",
    ),
    (
        "backup.the_referenced_smtp_secret_file_is_missing_could_not_create_the_snapshot",
        "Thiếu tệp bí mật SMTP được tham chiếu; không thể tạo snapshot.",
        "The referenced SMTP secret file is missing; could not create the snapshot.",
    ),
    (
        "backup.could_not_read_config_site_json",
        "Không đọc được config/site.json.",
        "Could not read config/site.json.",
    ),
    (
        "backup.invalid_config_site_json",
        "config/site.json không hợp lệ.",
        "Invalid config/site.json.",
    ),
    (
        "server.error",
        "Không thể tuần tự hóa config/site.json.",
        "Could not serialize config/site.json.",
    ),
    (
        "server.error",
        "Không thể tuần tự hóa config mặc định.",
        "Could not serialize the default configuration.",
    ),
    (
        "server.error",
        "Lỗi ghi config snapshot.",
        "Error writing the snapshot configuration.",
    ),
    (
        "server.error",
        "Lỗi fsync config snapshot.",
        "Error syncing the snapshot configuration to disk.",
    ),
    (
        "server.error",
        "Mục snapshot không có nguồn dữ liệu.",
        "The snapshot entry has no data source.",
    ),
    (
        "server.error",
        "Không thể tuần tự hóa manifest.json.",
        "Could not serialize manifest.json.",
    ),
    (
        "backup.manifest_json_exceeds_the_8_mib_limit",
        "manifest.json vượt giới hạn 8 MiB.",
        "manifest.json exceeds the 8 MiB limit.",
    ),
    (
        "server.error",
        "Lỗi ghi manifest.json.",
        "Error writing manifest.json.",
    ),
    (
        "server.error",
        "Lỗi fsync manifest.json.",
        "Error syncing manifest.json to disk.",
    ),
    (
        "backup.could_not_read_the_package_directory",
        "Không đọc được thư mục gói.",
        "Could not read the package directory.",
    ),
    (
        "backup.error_reading_an_item_in_the_package_directory",
        "Lỗi đọc mục trong thư mục gói.",
        "Error reading an item in the package directory.",
    ),
    (
        "server.error",
        "Không đọc được thông tin READY.json.",
        "Could not read READY.json information.",
    ),
    (
        "config.ready_json_is_invalid_or_too_large",
        "READY.json không hợp lệ hoặc vượt kích thước.",
        "READY.json is invalid or too large.",
    ),
    (
        "server.error",
        "Không đọc được READY.json.",
        "Could not read READY.json.",
    ),
    (
        "config.invalid_ready_json_format",
        "READY.json không đúng định dạng.",
        "Invalid READY.json format.",
    ),
    (
        "server.error",
        "Không đọc được metadata config/site.json.",
        "Could not read config/site.json metadata.",
    ),
    (
        "config.config_site_json_is_invalid_or_exceeds_64_kib",
        "config/site.json không hợp lệ hoặc vượt quá 64 KiB.",
        "config/site.json is invalid or exceeds 64 KiB.",
    ),
    (
        "config.invalid_config_site_json_format",
        "config/site.json không đúng định dạng.",
        "Invalid config/site.json format.",
    ),
    (
        "config.opendash_port_is_not_a_valid_port",
        "OPENDASH_PORT không phải cổng hợp lệ.",
        "OPENDASH_PORT is not a valid port.",
    ),
    (
        "server.error",
        "Không tuần tự hóa được config/site.json.",
        "Could not serialize config/site.json to JSON.",
    ),
    (
        "server.error",
        "Không ghi được config/site.json.",
        "Could not write config/site.json.",
    ),
    (
        "server.error",
        "Không fsync được config/site.json.",
        "Could not sync config/site.json to disk.",
    ),
    (
        "server.error",
        "Không cập nhật được config/site.json.",
        "Could not update config/site.json.",
    ),
    (
        "contact.contact_form_not_found",
        "Không tìm thấy form liên hệ.",
        "Contact form not found.",
    ),
    (
        "contact.invalid_form_nonce",
        "Nonce biểu mẫu không hợp lệ.",
        "Invalid form nonce.",
    ),
    (
        "contact.the_contact_form_is_currently_inactive",
        "Form liên hệ hiện không hoạt động.",
        "The contact form is currently inactive.",
    ),
    (
        "contact.the_form_version_has_changed_please_reload_the_page",
        "Phiên bản form đã thay đổi; vui lòng tải lại trang.",
        "The form version has changed; please reload the page.",
    ),
    (
        "hook.could_not_look_up_the_filter_content_prepare_save_plugin",
        "Không thể tra cứu plugin filter:content/prepare_save.",
        "Could not look up the filter:content/prepare_save plugin.",
    ),
    (
        "hook.the_filter_content_prepare_save_processing_limit_was_exceeded",
        "Vượt quá giới hạn xử lý filter:content/prepare_save.",
        "The filter:content/prepare_save processing limit was exceeded.",
    ),
    (
        "hook.the_filter_content_prepare_save_plugin_manifest_was_not_found",
        "Không tìm thấy manifest plugin filter:content/prepare_save.",
        "The filter:content/prepare_save plugin manifest was not found.",
    ),
    (
        "hook.the_plugin_cannot_write_this_content_type_in_filter_content_prepare_save",
        "Plugin không có quyền ghi loại nội dung trong filter:content/prepare_save.",
        "The plugin cannot write this content type in filter:content/prepare_save.",
    ),
    (
        "hook.could_not_look_up_the_contact_form_validation_plugin",
        "Không thể tra cứu plugin kiểm tra biểu mẫu liên hệ.",
        "Could not look up the contact form validation plugin.",
    ),
    (
        "hook.the_filter_contact_before_submit_validation_limit_was_exceeded",
        "Vượt quá giới hạn kiểm tra filter:contact/before_submit.",
        "The filter:contact/before_submit validation limit was exceeded.",
    ),
    (
        "media.the_image_is_empty_or_exceeds_8_mib",
        "Ảnh trống hoặc vượt quá 8 MiB.",
        "The image is empty or exceeds 8 MiB.",
    ),
    (
        "media.could_not_recognize_the_image_format",
        "Không nhận diện được định dạng ảnh.",
        "Could not recognize the image format.",
    ),
    (
        "request.unsupported_media_type",
        "Chỉ hỗ trợ JPEG, PNG và WebP.",
        "Only JPEG, PNG and WebP are supported.",
    ),
    (
        "request.unsupported_media_type",
        "Content-Type không khớp với định dạng ảnh thực tế.",
        "Content-Type does not match the actual image format.",
    ),
    (
        "media.the_image_is_corrupted_too_large_or_could_not_be_decoded",
        "Ảnh bị lỗi, quá lớn hoặc không thể giải mã.",
        "The image is corrupted, too large or could not be decoded.",
    ),
    (
        "media.the_image_dimensions_exceed_the_limit",
        "Kích thước ảnh vượt giới hạn.",
        "The image dimensions exceed the limit.",
    ),
    (
        "server.error",
        "Không thể tạo ảnh thu nhỏ.",
        "Could not generate a thumbnail.",
    ),
    (
        "media.could_not_normalize_the_uploaded_image",
        "Không thể chuẩn hóa ảnh đã upload.",
        "Could not normalize the uploaded image.",
    ),
    (
        "media.the_processed_image_exceeds_the_8_mib_limit",
        "Ảnh sau khi xử lý vượt quá giới hạn 8 MiB.",
        "The processed image exceeds the 8 MiB limit.",
    ),
    (
        "media.media_not_found",
        "Không tìm thấy media.",
        "Media not found.",
    ),
    (
        "media.invalid_image_id",
        "Mã ảnh không hợp lệ.",
        "Invalid image ID.",
    ),
    (
        "server.error",
        "Không tạo được thư mục Media Library.",
        "Could not create the Media Library directory.",
    ),
    (
        "server.error",
        "Không đọc được file ảnh đã lưu.",
        "Could not read the saved image file.",
    ),
    (
        "server.error",
        "Không tạo được thư mục tạm cho video.",
        "Could not create the temporary video directory.",
    ),
    (
        "server.error",
        "Không lưu được file video an toàn.",
        "Could not safely save the video file.",
    ),
    (
        "media.video_file_not_found",
        "Không tìm thấy file video.",
        "Video file not found.",
    ),
    (
        "media.could_not_read_video_file_information",
        "Không đọc được thông tin file video.",
        "Could not read video file information.",
    ),
    (
        "server.error",
        "Không seek được file video.",
        "Could not seek in the video file.",
    ),
    (
        "server.error",
        "Không mở được file tạm cho media.",
        "Could not open the temporary media file.",
    ),
    (
        "server.error",
        "Không ghi được file media an toàn.",
        "Could not safely write the media file.",
    ),
    (
        "request.if_match_required",
        "Thiếu header If-Match.",
        "The If-Match header is missing.",
    ),
    (
        "request.invalid_if_match",
        "Header If-Match phải có dạng \"gv<generation>-v<version>\".",
        "The If-Match header must have the format \"gv<generation>-v<version>\".",
    ),
    (
        "request.invalid_if_match",
        "Header If-Match không đúng định dạng gv<generation>-v<version>.",
        "The If-Match header does not match the gv<generation>-v<version> format.",
    ),
    (
        "request.invalid_if_match",
        "Generation trong If-Match không hợp lệ.",
        "Invalid generation in If-Match.",
    ),
    (
        "request.invalid_if_match",
        "Version trong If-Match không hợp lệ.",
        "Invalid version in If-Match.",
    ),
    (
        "server.error",
        "Không đọc lại được delete job.",
        "Could not read the deletion job.",
    ),
    (
        "server.error",
        "Không đọc lại được delete job sau khi hoàn tất.",
        "Could not read the deletion job after completion.",
    ),
    (
        "media.could_not_read_the_file_modification_time",
        "Không đọc được thời gian sửa đổi tệp.",
        "Could not read the file modification time.",
    ),
    (
        "media.could_not_read_the_file_to_compute_its_checksum",
        "Không đọc được tệp để tính checksum.",
        "Could not read the file to compute its checksum.",
    ),
    (
        "media.could_not_read_metadata_in_the_data_tree",
        "Không đọc được metadata trong cây dữ liệu.",
        "Could not read metadata in the data tree.",
    ),
    (
        "media.a_symlink_was_found_in_the_data_tree_reconciliation_stopped",
        "Phát hiện symlink trong cây dữ liệu; dừng đối soát.",
        "A symlink was found in the data tree; reconciliation stopped.",
    ),
    (
        "media.the_file_count_exceeds_the_reconciliation_limit_20_000_files",
        "Số lượng tệp vượt giới hạn reconcile (tối đa 20.000 tệp).",
        "The file count exceeds the reconciliation limit (20,000 files).",
    ),
    (
        "media.could_not_read_the_uploads_directory",
        "Không đọc được thư mục uploads.",
        "Could not read the uploads directory.",
    ),
    (
        "media.invalid_uploads_directory",
        "Thư mục uploads không hợp lệ.",
        "Invalid uploads directory.",
    ),
    (
        "media.could_not_list_the_uploads_directory",
        "Không thể liệt kê thư mục uploads.",
        "Could not list the uploads directory.",
    ),
    (
        "media.error_reading_an_item_in_uploads",
        "Lỗi đọc mục trong uploads.",
        "Error reading an item in uploads.",
    ),
    (
        "media.unable_to_read_uploads_media",
        "Không thể đọc uploads/media.",
        "Unable to read uploads/media.",
    ),
    (
        "media.error_reading_a_media_shard_directory",
        "Lỗi đọc thư mục shard media.",
        "Error reading a media shard directory.",
    ),
    (
        "media.an_invalid_file_or_directory_was_found_in_uploads_media",
        "Phát hiện tệp hoặc thư mục không hợp lệ trong uploads/media.",
        "An invalid file or directory was found in uploads/media.",
    ),
    (
        "media.could_not_read_a_media_shard_directory",
        "Không đọc được thư mục shard media.",
        "Could not read a media shard directory.",
    ),
    (
        "media.error_reading_a_file_in_a_media_shard",
        "Lỗi đọc tệp trong shard media.",
        "Error reading a file in a media shard.",
    ),
    (
        "media.a_non_regular_file_was_found_in_uploads_media",
        "Phát hiện đối tượng không phải tệp thường trong uploads/media.",
        "A non-regular file was found in uploads/media.",
    ),
    (
        "media.an_unrecognized_file_was_found_in_uploads_media",
        "Phát hiện tệp không rõ định danh trong uploads/media.",
        "An unrecognized file was found in uploads/media.",
    ),
    (
        "media.could_not_read_uploads_quarantine",
        "Không đọc được uploads/.quarantine.",
        "Could not read uploads/.quarantine.",
    ),
    (
        "media.error_reading_uploads_quarantine",
        "Lỗi đọc uploads/.quarantine.",
        "Error reading uploads/.quarantine.",
    ),
    (
        "media.an_unexpected_file_was_found_in_uploads_quarantine",
        "Phát hiện tệp lạ trong uploads/.quarantine.",
        "An unexpected file was found in uploads/.quarantine.",
    ),
    (
        "media.an_invalid_object_was_found_in_quarantine",
        "Phát hiện đối tượng không hợp lệ trong quarantine.",
        "An invalid object was found in quarantine.",
    ),
    (
        "media.an_unidentified_file_or_directory_was_found_in_uploads",
        "Phát hiện tệp hoặc thư mục không xác định trong uploads.",
        "An unidentified file or directory was found in uploads.",
    ),
    (
        "media.could_not_read_the_secrets_directory",
        "Không đọc được thư mục secrets.",
        "Could not read the secrets directory.",
    ),
    (
        "media.invalid_secrets_directory",
        "Thư mục secrets không hợp lệ.",
        "Invalid secrets directory.",
    ),
    (
        "media.could_not_list_the_secrets_directory",
        "Không thể liệt kê thư mục secrets.",
        "Could not list the secrets directory.",
    ),
    (
        "media.error_reading_an_item_in_the_secrets_directory",
        "Lỗi đọc mục trong thư mục secrets.",
        "Error reading an item in the secrets directory.",
    ),
    (
        "media.a_non_file_object_was_found_in_the_secrets_directory",
        "Phát hiện đối tượng không phải tệp trong thư mục secrets.",
        "A non-file object was found in the secrets directory.",
    ),
    (
        "media.an_unidentified_file_was_found_in_the_secrets_directory",
        "Phát hiện tệp không rõ định danh trong thư mục secrets.",
        "An unidentified file was found in the secrets directory.",
    ),
    (
        "server.error",
        "Không thể tuần tự hóa kế hoạch đối soát.",
        "Could not serialize the reconciliation plan.",
    ),
    (
        "media.explicit_confirmation_confirm_true_is_required_to_apply_reconciliation",
        "Cần xác nhận rõ ràng (confirm=true) để áp dụng đối soát.",
        "Explicit confirmation (confirm=true) is required to apply reconciliation.",
    ),
    (
        "media.reconciliation_plan_not_found",
        "Không tìm thấy kế hoạch đối soát.",
        "Reconciliation plan not found.",
    ),
    (
        "media.the_job_id_does_not_identify_a_reconciliation_plan",
        "Mã tác vụ không phải kế hoạch đối soát.",
        "The job ID does not identify a reconciliation plan.",
    ),
    (
        "request.precondition_failed",
        "Kế hoạch đối soát không có dữ liệu.",
        "The reconciliation plan has no data.",
    ),
    (
        "request.precondition_failed",
        "Dữ liệu kế hoạch đối soát không hợp lệ.",
        "Invalid reconciliation plan data.",
    ),
    (
        "request.precondition_failed",
        "Trạng thái hoặc digest của kế hoạch đối soát đã thay đổi; vui lòng lập kế hoạch lại.",
        "The reconciliation plan status or digest changed; please create a new plan.",
    ),
    (
        "request.precondition_failed",
        "Tệp ứng viên đã được tham chiếu hoặc đang có luồng đọc; vui lòng lập kế hoạch lại.",
        "The candidate file is now referenced or has an active reader; please create a new plan.",
    ),
    (
        "request.precondition_failed",
        "Tệp ứng viên không còn tồn tại; vui lòng lập kế hoạch lại.",
        "The candidate file no longer exists; please create a new plan.",
    ),
    (
        "request.precondition_failed",
        "Danh tính tệp ứng viên đã thay đổi.",
        "The candidate file identity has changed.",
    ),
    (
        "request.precondition_failed",
        "Nội dung hoặc thời gian sửa đổi tệp ứng viên đã thay đổi.",
        "The candidate file contents or modification time have changed.",
    ),
    (
        "server.error",
        "Tên tệp ứng viên không hợp lệ.",
        "Invalid candidate file name.",
    ),
    (
        "media.explicit_confirmation_confirm_true_is_required_to_clean_quarantine",
        "Cần xác nhận rõ ràng (confirm=true) để dọn vùng cách ly.",
        "Explicit confirmation (confirm=true) is required to clean quarantine.",
    ),
    (
        "server.error",
        "Không thể xóa thư mục cách ly đã hết hạn.",
        "Could not delete the expired quarantine directory.",
    ),
    (
        "server.error",
        "Không thể xóa tệp secret mồ côi đã hết hạn.",
        "Could not delete the expired orphan secret file.",
    ),
    (
        "media.mp4_video_metadata_validation_timed_out",
        "Video MP4 vượt quá thời gian kiểm tra metadata.",
        "MP4 video metadata validation timed out.",
    ),
    (
        "media.the_mp4_video_exceeds_the_allowed_box_count",
        "Video MP4 vượt quá số lượng box cho phép.",
        "The MP4 video exceeds the allowed box count.",
    ),
    (
        "media.truncated_mp4_box",
        "Box MP4 bị cắt cụt.",
        "Truncated MP4 box.",
    ),
    (
        "media.could_not_read_the_mp4_header",
        "Không đọc được header MP4.",
        "Could not read the MP4 header.",
    ),
    (
        "media.truncated_64_bit_mp4_box",
        "Box MP4 64-bit bị cắt cụt.",
        "Truncated 64-bit MP4 box.",
    ),
    (
        "media.could_not_read_the_64_bit_mp4_size",
        "Không đọc được kích thước 64-bit MP4.",
        "Could not read the 64-bit MP4 size.",
    ),
    (
        "media.mp4_box_size_overflow",
        "Kích thước box MP4 bị tràn số.",
        "MP4 box size overflow.",
    ),
    (
        "media.mp4_box_size_exceeds_the_file_bounds",
        "Kích thước box MP4 vượt ngoài phạm vi file.",
        "MP4 box size exceeds the file bounds.",
    ),
    (
        "media.mp4_box_nesting_is_too_deep",
        "Cấu trúc box MP4 lồng nhau quá sâu.",
        "MP4 box nesting is too deep.",
    ),
    (
        "media.truncated_mp4_sub_box",
        "Sub-box MP4 bị cắt cụt.",
        "Truncated MP4 sub-box.",
    ),
    (
        "media.truncated_64_bit_mp4_sub_box",
        "Sub-box MP4 64-bit bị cắt cụt.",
        "Truncated 64-bit MP4 sub-box.",
    ),
    (
        "media.mp4_sub_box_is_too_large",
        "Sub-box MP4 quá lớn.",
        "MP4 sub-box is too large.",
    ),
    (
        "media.mp4_sub_box_exceeds_the_container_bounds",
        "Sub-box MP4 vượt phạm vi container.",
        "MP4 sub-box exceeds the container bounds.",
    ),
    (
        "media.the_video_is_empty_or_exceeds_the_100_mib_limit",
        "Video trống hoặc vượt quá giới hạn 100 MiB.",
        "The video is empty or exceeds the 100 MiB limit.",
    ),
    (
        "server.error",
        "Không mở được file video tạm để kiểm tra.",
        "Could not open the temporary video file for validation.",
    ),
    (
        "server.error",
        "Không đọc được kích thước file video tạm.",
        "Could not read the temporary video file size.",
    ),
    (
        "media.the_video_file_size_does_not_match",
        "Kích thước file video không khớp.",
        "The video file size does not match.",
    ),
    (
        "media.invalid_mp4_ftyp_box",
        "Box ftyp MP4 không hợp lệ.",
        "Invalid MP4 ftyp box.",
    ),
    (
        "media.invalid_mp4_ftyp_box_size",
        "Box ftyp MP4 có kích thước không hợp lệ.",
        "Invalid MP4 ftyp box size.",
    ),
    (
        "media.could_not_read_the_mp4_ftyp_box",
        "Không đọc được box ftyp MP4.",
        "Could not read the MP4 ftyp box.",
    ),
    (
        "media.mp4_is_missing_ftyp_at_the_start_of_the_file",
        "MP4 thiếu ftyp ở đầu file.",
        "MP4 is missing ftyp at the start of the file.",
    ),
    (
        "media.mp4_must_contain_exactly_one_moov_box",
        "MP4 chỉ được phép có đúng một box moov.",
        "MP4 must contain exactly one moov box.",
    ),
    (
        "media.mp4_moov_metadata_is_empty_or_exceeds_2_mib",
        "Metadata moov của MP4 trống hoặc vượt quá 2 MiB.",
        "MP4 moov metadata is empty or exceeds 2 MiB.",
    ),
    (
        "media.could_not_read_the_mp4_moov_box",
        "Không đọc được box moov MP4.",
        "Could not read the MP4 moov box.",
    ),
    (
        "media.fragmented_mp4_is_not_supported",
        "Không hỗ trợ MP4 phân mảnh (fragmented MP4).",
        "Fragmented MP4 is not supported.",
    ),
    (
        "media.mp4_contains_a_disallowed_top_level_box",
        "MP4 chứa top-level box không được phép.",
        "MP4 contains a disallowed top-level box.",
    ),
    (
        "media.mp4_is_missing_a_valid_ftyp_or_mdat_box",
        "MP4 thiếu ftyp hoặc mdat hợp lệ.",
        "MP4 is missing a valid ftyp or mdat box.",
    ),
    (
        "media.mp4_is_missing_the_moov_box",
        "MP4 thiếu box moov.",
        "MP4 is missing the moov box.",
    ),
    (
        "media.malformed_mp4_ftyp_box",
        "Box ftyp MP4 sai định dạng.",
        "Malformed MP4 ftyp box.",
    ),
    (
        "request.unsupported_media_type",
        "MP4 phải tương thích isom hoặc mp42.",
        "MP4 must be compatible with isom or mp42.",
    ),
    (
        "media.mp4_contains_more_than_one_mvhd_box",
        "MP4 có nhiều hơn một mvhd.",
        "MP4 contains more than one mvhd box.",
    ),
    (
        "media.mp4_exceeds_the_maximum_of_2_tracks",
        "MP4 vượt quá tối đa 2 track.",
        "MP4 exceeds the maximum of 2 tracks.",
    ),
    (
        "media.fragmented_or_encrypted_mp4_is_not_supported",
        "Không hỗ trợ MP4 phân mảnh hoặc mã hóa.",
        "Fragmented or encrypted MP4 is not supported.",
    ),
    (
        "media.mp4_is_missing_the_mvhd_box",
        "MP4 thiếu box mvhd.",
        "MP4 is missing the mvhd box.",
    ),
    (
        "media.mp4_must_contain_exactly_one_video_track",
        "MP4 chỉ được phép có đúng một video track.",
        "MP4 must contain exactly one video track.",
    ),
    (
        "media.mp4_may_contain_at_most_one_audio_track",
        "MP4 chỉ được phép có tối đa một audio track.",
        "MP4 may contain at most one audio track.",
    ),
    (
        "media.mp4_must_contain_an_avc_video_track",
        "MP4 phải có một video track AVC.",
        "MP4 must contain an AVC video track.",
    ),
    (
        "media.mp4_has_overlapping_sample_byte_ranges",
        "MP4 có sample byte range chồng lấn.",
        "MP4 has overlapping sample byte ranges.",
    ),
    (
        "media.truncated_mvhd_box",
        "Box mvhd bị cắt cụt.",
        "Truncated mvhd box.",
    ),
    (
        "media.truncated_mvhd_v0_box",
        "Box mvhd v0 bị cắt cụt.",
        "Truncated mvhd v0 box.",
    ),
    (
        "media.truncated_mvhd_v1_box",
        "Box mvhd v1 bị cắt cụt.",
        "Truncated mvhd v1 box.",
    ),
    (
        "media.invalid_mvhd_version",
        "Phiên bản mvhd không hợp lệ.",
        "Invalid mvhd version.",
    ),
    (
        "media.mp4_timescale_or_duration_is_zero",
        "Timescale hoặc duration MP4 bằng 0.",
        "MP4 timescale or duration is zero.",
    ),
    (
        "media.mp4_duration_overflow",
        "Duration MP4 bị tràn số.",
        "MP4 duration overflow.",
    ),
    (
        "media.mp4_duration_exceeds_the_limit",
        "Duration MP4 vượt giới hạn.",
        "MP4 duration exceeds the limit.",
    ),
    (
        "media.video_duration_is_outside_the_allowed_1_600000_ms_range",
        "Thời lượng video vượt quá giới hạn 1..600000 ms.",
        "Video duration is outside the allowed 1..600000 ms range.",
    ),
    (
        "media.mp4_track_is_missing_tkhd",
        "Track MP4 thiếu tkhd.",
        "MP4 track is missing tkhd.",
    ),
    (
        "media.mp4_track_is_missing_mdia",
        "Track MP4 thiếu mdia.",
        "MP4 track is missing mdia.",
    ),
    (
        "media.truncated_hdlr_box",
        "Box hdlr bị cắt cụt.",
        "Truncated hdlr box.",
    ),
    (
        "media.mp4_track_is_missing_mdhd",
        "Track MP4 thiếu mdhd.",
        "MP4 track is missing mdhd.",
    ),
    (
        "media.mp4_track_is_missing_hdlr",
        "Track MP4 thiếu hdlr.",
        "MP4 track is missing hdlr.",
    ),
    (
        "request.unsupported_media_type",
        "MP4 chứa loại track không được hỗ trợ.",
        "MP4 contains an unsupported track type.",
    ),
    (
        "media.mp4_track_is_missing_minf",
        "Track MP4 thiếu minf.",
        "MP4 track is missing minf.",
    ),
    (
        "media.mp4_track_is_missing_dinf",
        "Track MP4 thiếu dinf.",
        "MP4 track is missing dinf.",
    ),
    (
        "media.mp4_track_is_missing_stbl",
        "Track MP4 thiếu stbl.",
        "MP4 track is missing stbl.",
    ),
    (
        "media.encrypted_mp4_tracks_are_not_supported",
        "Không hỗ trợ track MP4 mã hóa.",
        "Encrypted MP4 tracks are not supported.",
    ),
    (
        "media.mp4_track_is_missing_stsd",
        "Track MP4 thiếu stsd.",
        "MP4 track is missing stsd.",
    ),
    (
        "media.mp4_track_is_missing_stts",
        "Track MP4 thiếu stts.",
        "MP4 track is missing stts.",
    ),
    (
        "media.mp4_track_is_missing_stsc",
        "Track MP4 thiếu stsc.",
        "MP4 track is missing stsc.",
    ),
    (
        "media.mp4_track_is_missing_stsz",
        "Track MP4 thiếu stsz.",
        "MP4 track is missing stsz.",
    ),
    (
        "media.mp4_track_is_missing_stco_co64",
        "Track MP4 thiếu stco/co64.",
        "MP4 track is missing stco/co64.",
    ),
    (
        "media.mp4_track_has_no_samples",
        "Track MP4 không có sample nào.",
        "MP4 track has no samples.",
    ),
    (
        "media.mp4_track_dimensions_do_not_match_the_video_parameters",
        "Kích thước track MP4 không khớp với thông số video.",
        "MP4 track dimensions do not match the video parameters.",
    ),
    (
        "media.truncated_tkhd_box",
        "Box tkhd bị cắt cụt.",
        "Truncated tkhd box.",
    ),
    (
        "media.invalid_tkhd_version",
        "Phiên bản tkhd không hợp lệ.",
        "Invalid tkhd version.",
    ),
    (
        "media.truncated_mdhd_box",
        "Box mdhd bị cắt cụt.",
        "Truncated mdhd box.",
    ),
    (
        "media.truncated_mdhd_v0_box",
        "Box mdhd v0 bị cắt cụt.",
        "Truncated mdhd v0 box.",
    ),
    (
        "media.truncated_mdhd_v1_box",
        "Box mdhd v1 bị cắt cụt.",
        "Truncated mdhd v1 box.",
    ),
    (
        "media.invalid_mdhd_version",
        "Phiên bản mdhd không hợp lệ.",
        "Invalid mdhd version.",
    ),
    (
        "media.mdhd_timescale_or_duration_is_zero",
        "mdhd timescale hoặc duration bằng 0.",
        "mdhd timescale or duration is zero.",
    ),
    (
        "media.truncated_dref_box",
        "Box dref bị cắt cụt.",
        "Truncated dref box.",
    ),
    (
        "media.invalid_dref_count",
        "Số lượng dref không hợp lệ.",
        "Invalid dref count.",
    ),
    (
        "media.dref_entry_count_does_not_match",
        "Số entry dref không khớp.",
        "dref entry count does not match.",
    ),
    (
        "media.external_data_references_are_not_supported",
        "Không hỗ trợ tham chiếu dữ liệu ngoài (external data reference).",
        "External data references are not supported.",
    ),
    (
        "media.the_dinf_box_is_missing_dref",
        "Box dinf thiếu dref.",
        "The dinf box is missing dref.",
    ),
    (
        "media.truncated_stsd_box",
        "Box stsd bị cắt cụt.",
        "Truncated stsd box.",
    ),
    (
        "media.video_stsd_must_contain_exactly_1_entry",
        "stsd video phải có đúng 1 entry.",
        "Video stsd must contain exactly 1 entry.",
    ),
    (
        "media.video_stsd_is_missing_its_entry",
        "stsd video thiếu entry.",
        "Video stsd is missing its entry.",
    ),
    (
        "request.unsupported_media_type",
        "Video MP4 chỉ hỗ trợ codec H.264/AVC (avc1).",
        "MP4 video only supports the H.264/AVC (avc1) codec.",
    ),
    (
        "media.truncated_avc1_entry",
        "Entry avc1 bị cắt cụt.",
        "Truncated avc1 entry.",
    ),
    (
        "media.video_dimensions_are_outside_the_allowed_1_3840_x_1_2160_range",
        "Kích thước video vượt giới hạn 1..3840 x 1..2160.",
        "Video dimensions are outside the allowed 1..3840 x 1..2160 range.",
    ),
    (
        "media.encrypted_video_is_not_supported",
        "Không hỗ trợ video mã hóa.",
        "Encrypted video is not supported.",
    ),
    (
        "media.avc1_video_is_missing_avcc",
        "Video avc1 thiếu avcC.",
        "avc1 video is missing avcC.",
    ),
    (
        "media.sps_dimensions_do_not_match_avc1_metadata",
        "Kích thước SPS không khớp với metadata avc1.",
        "SPS dimensions do not match avc1 metadata.",
    ),
    (
        "media.invalid_avcc_box",
        "Box avcC không hợp lệ.",
        "Invalid avcC box.",
    ),
    (
        "media.avcc_is_missing_sps",
        "avcC thiếu SPS.",
        "avcC is missing SPS.",
    ),
    (
        "media.truncated_sps_in_avcc",
        "SPS trong avcC bị cắt cụt.",
        "Truncated SPS in avcC.",
    ),
    (
        "media.invalid_sps_length_in_avcc",
        "Độ dài SPS trong avcC không hợp lệ.",
        "Invalid SPS length in avcC.",
    ),
    (
        "media.the_first_nalu_in_avcc_is_not_sps",
        "NALU đầu tiên trong avcC không phải SPS.",
        "The first NALU in avcC is not SPS.",
    ),
    (
        "media.avcc_is_missing_pps",
        "avcC thiếu PPS.",
        "avcC is missing PPS.",
    ),
    (
        "media.truncated_pps_in_avcc",
        "PPS trong avcC bị cắt cụt.",
        "Truncated PPS in avcC.",
    ),
    (
        "media.invalid_pps_length_in_avcc",
        "Độ dài PPS trong avcC không hợp lệ.",
        "Invalid PPS length in avcC.",
    ),
    (
        "media.pps_in_avcc_has_an_incorrect_nalu_type",
        "PPS trong avcC sai loại NALU.",
        "PPS in avcC has an incorrect NALU type.",
    ),
    (
        "media.could_not_decode_sps",
        "Không giải mã được SPS.",
        "Could not decode SPS.",
    ),
    (
        "media.truncated_h_264_sps",
        "SPS H.264 bị cắt cụt.",
        "Truncated H.264 SPS.",
    ),
    (
        "media.sps_exp_golomb_exceeds_the_limit",
        "Exp-Golomb SPS vượt giới hạn.",
        "SPS Exp-Golomb exceeds the limit.",
    ),
    (
        "media.invalid_sps_chroma_format_idc",
        "chroma_format_idc SPS không hợp lệ.",
        "Invalid SPS chroma_format_idc.",
    ),
    (
        "media.invalid_sps_poc_cycle",
        "POC cycle SPS không hợp lệ.",
        "Invalid SPS POC cycle.",
    ),
    (
        "media.invalid_sps_pic_order_cnt_type",
        "pic_order_cnt_type SPS không hợp lệ.",
        "Invalid SPS pic_order_cnt_type.",
    ),
    (
        "media.sps_width_overflow",
        "Chiều rộng SPS bị tràn số.",
        "SPS width overflow.",
    ),
    (
        "media.sps_height_overflow",
        "Chiều cao SPS bị tràn số.",
        "SPS height overflow.",
    ),
    (
        "media.horizontal_sps_crop_exceeds_width",
        "Crop ngang SPS vượt chiều rộng.",
        "Horizontal SPS crop exceeds width.",
    ),
    (
        "media.vertical_sps_crop_exceeds_height",
        "Crop dọc SPS vượt chiều cao.",
        "Vertical SPS crop exceeds height.",
    ),
    (
        "media.sps_video_dimensions_are_outside_the_allowed_1_3840_x_1_2160_range",
        "Kích thước video SPS vượt giới hạn 1..3840 x 1..2160.",
        "SPS video dimensions are outside the allowed 1..3840 x 1..2160 range.",
    ),
    (
        "media.truncated_audio_stsd_box",
        "Box stsd audio bị cắt cụt.",
        "Truncated audio stsd box.",
    ),
    (
        "media.audio_stsd_must_contain_exactly_1_entry",
        "stsd audio phải có đúng 1 entry.",
        "Audio stsd must contain exactly 1 entry.",
    ),
    (
        "media.audio_stsd_is_missing_its_entry",
        "stsd audio thiếu entry.",
        "Audio stsd is missing its entry.",
    ),
    (
        "request.unsupported_media_type",
        "Audio MP4 chỉ hỗ trợ AAC-LC (mp4a).",
        "MP4 audio only supports AAC-LC (mp4a).",
    ),
    (
        "media.truncated_mp4a_entry",
        "Entry mp4a bị cắt cụt.",
        "Truncated mp4a entry.",
    ),
    (
        "media.encrypted_audio_is_not_supported",
        "Không hỗ trợ audio mã hóa.",
        "Encrypted audio is not supported.",
    ),
    (
        "media.mp4a_audio_is_missing_the_esds_box",
        "Audio mp4a thiếu box esds.",
        "mp4a audio is missing the esds box.",
    ),
    (
        "media.truncated_esds_descriptor",
        "Descriptor esds bị cắt cụt.",
        "Truncated esds descriptor.",
    ),
    (
        "media.invalid_esds_descriptor_length",
        "Độ dài descriptor esds không hợp lệ.",
        "Invalid esds descriptor length.",
    ),
    (
        "media.truncated_esds_box",
        "Box esds bị cắt cụt.",
        "Truncated esds box.",
    ),
    (
        "media.esds_is_missing_es_descrtag",
        "esds thiếu ES_DescrTag.",
        "esds is missing ES_DescrTag.",
    ),
    (
        "media.invalid_esds_es_descrtag_length",
        "esds ES_DescrTag sai độ dài.",
        "Invalid esds ES_DescrTag length.",
    ),
    (
        "media.truncated_esds_url",
        "esds URL bị cắt cụt.",
        "Truncated esds URL.",
    ),
    (
        "media.esds_is_missing_decoderconfigdescrtag",
        "esds thiếu DecoderConfigDescrTag.",
        "esds is missing DecoderConfigDescrTag.",
    ),
    (
        "media.invalid_esds_decoderconfig_length",
        "esds DecoderConfig sai độ dài.",
        "Invalid esds DecoderConfig length.",
    ),
    (
        "request.unsupported_media_type",
        "Audio MP4 chỉ hỗ trợ MPEG-4 AAC-LC.",
        "MP4 audio only supports MPEG-4 AAC-LC.",
    ),
    (
        "media.esds_is_missing_audiospecificconfig",
        "esds thiếu AudioSpecificConfig.",
        "esds is missing AudioSpecificConfig.",
    ),
    (
        "media.truncated_audiospecificconfig",
        "AudioSpecificConfig bị cắt cụt.",
        "Truncated AudioSpecificConfig.",
    ),
    (
        "request.unsupported_media_type",
        "Audio MP4 chỉ hỗ trợ profile AAC-LC.",
        "MP4 audio only supports the AAC-LC profile.",
    ),
    (
        "media.truncated_stts_box",
        "Box stts bị cắt cụt.",
        "Truncated stts box.",
    ),
    (
        "media.stts_entry_count_exceeds_the_payload",
        "Số entry stts vượt payload.",
        "stts entry count exceeds the payload.",
    ),
    (
        "media.truncated_stsz_box",
        "Box stsz bị cắt cụt.",
        "Truncated stsz box.",
    ),
    (
        "media.total_mp4_sample_count_exceeds_100_000",
        "Tổng số sample MP4 vượt giới hạn 100.000.",
        "Total MP4 sample count exceeds 100,000.",
    ),
    (
        "media.the_stsz_sample_size_table_exceeds_the_payload",
        "Bảng kích thước sample stsz vượt payload.",
        "The stsz sample size table exceeds the payload.",
    ),
    (
        "media.truncated_stsc_box",
        "Box stsc bị cắt cụt.",
        "Truncated stsc box.",
    ),
    (
        "media.invalid_stsc_entry_count",
        "Số entry stsc không hợp lệ.",
        "Invalid stsc entry count.",
    ),
    (
        "media.truncated_stco_co64_box",
        "Box stco/co64 bị cắt cụt.",
        "Truncated stco/co64 box.",
    ),
    (
        "media.invalid_stco_co64_chunk_count",
        "Số chunk stco/co64 không hợp lệ.",
        "Invalid stco/co64 chunk count.",
    ),
    (
        "media.invalid_stsc_table",
        "Bảng stsc không hợp lệ.",
        "Invalid stsc table.",
    ),
    (
        "media.stsc_must_start_at_chunk_1",
        "stsc phải bắt đầu từ chunk 1.",
        "stsc must start at chunk 1.",
    ),
    (
        "media.the_stsc_sample_count_exceeds_stsz_sample_count",
        "Số sample theo stsc vượt quá stsz sample_count.",
        "The stsc sample count exceeds stsz sample_count.",
    ),
    (
        "media.mp4_contains_a_zero_size_sample",
        "Sample MP4 có kích thước 0.",
        "MP4 contains a zero-size sample.",
    ),
    (
        "media.mp4_sample_offset_overflow",
        "Sample offset MP4 bị tràn số.",
        "MP4 sample offset overflow.",
    ),
    (
        "media.mp4_sample_points_outside_the_mdat_box",
        "Sample MP4 trỏ ra ngoài phạm vi box mdat.",
        "MP4 sample points outside the mdat box.",
    ),
    (
        "media.total_stsc_stco_sample_count_does_not_match_stsz",
        "Tổng số sample stsc/stco không khớp stsz.",
        "Total stsc/stco sample count does not match stsz.",
    ),
    (
        "page.invalid_block_css_scope",
        "Scope CSS block không hợp lệ.",
        "Invalid block CSS scope.",
    ),
    (
        "page.builder_template_composition_exceeds_512_kib",
        "Thành phần mẫu Builder vượt quá 512 KiB.",
        "Builder template composition exceeds 512 KiB.",
    ),
    (
        "page.page_rendering_fuel_budget_exhausted",
        "Đã hết ngân sách fuel render trang.",
        "Page rendering fuel budget exhausted.",
    ),
    (
        "page.could_not_compile_the_template",
        "Không thể biên dịch template.",
        "Could not compile the template.",
    ),
    (
        "page.template_not_found",
        "Không tìm thấy template.",
        "Template not found.",
    ),
    (
        "page.template_execution_error_fuel_recursion_undefined",
        "Lỗi thực thi template (fuel/recursion/undefined).",
        "Template execution error (fuel/recursion/undefined).",
    ),
    (
        "page.fuel_tracker_missing",
        "Thiếu bộ theo dõi fuel.",
        "Fuel tracker missing.",
    ),
    (
        "page.rendered_html_exceeds_the_512_kib_limit",
        "HTML render vượt quá giới hạn 512 KiB.",
        "Rendered HTML exceeds the 512 KiB limit.",
    ),
    (
        "server.error",
        "Không mở được tệp để băm.",
        "Could not open the file for hashing.",
    ),
    (
        "server.error",
        "Lỗi đọc tệp để băm.",
        "Error reading the file for hashing.",
    ),
    (
        "restore.the_restore_destination_directory_already_exists",
        "Thư mục đích khôi phục đã tồn tại.",
        "The restore destination directory already exists.",
    ),
    (
        "restore.the_restore_destination_must_not_be_or_contain_the_source_site",
        "Thư mục đích khôi phục không được trùng hoặc chứa site nguồn.",
        "The restore destination must not be or contain the source site.",
    ),
    (
        "restore.invalid_restore_destination_path",
        "Đường dẫn đích khôi phục không hợp lệ.",
        "Invalid restore destination path.",
    ),
    (
        "restore.the_restore_staging_directory_already_exists",
        "Thư mục staging khôi phục đã tồn tại.",
        "The restore staging directory already exists.",
    ),
    (
        "server.error",
        "Không đọc được portable config từ backup.",
        "Could not read the portable configuration from the backup.",
    ),
    (
        "restore.invalid_config_site_json_in_the_backup",
        "config/site.json trong backup không hợp lệ.",
        "Invalid config/site.json in the backup.",
    ),
    (
        "server.error",
        "Không tuần tự hóa được config khôi phục.",
        "Could not serialize the restored configuration.",
    ),
    (
        "server.error",
        "Không ghi được config khôi phục.",
        "Could not write the restored configuration.",
    ),
    (
        "server.error",
        "Không fsync được config khôi phục.",
        "Could not sync the restored configuration to disk.",
    ),
    (
        "server.error",
        "Không mở được payload khôi phục.",
        "Could not open the restore payload.",
    ),
    (
        "server.error",
        "Lỗi đọc payload khôi phục.",
        "Error reading the restore payload.",
    ),
    (
        "server.error",
        "Lỗi ghi payload khôi phục.",
        "Error writing the restore payload.",
    ),
    (
        "server.error",
        "Lỗi fsync payload khôi phục.",
        "Error syncing the restore payload to disk.",
    ),
    (
        "restore.the_restore_payload_does_not_match_the_manifest",
        "Payload khôi phục không khớp manifest.",
        "The restore payload does not match the manifest.",
    ),
    (
        "restore.the_backup_is_missing_database_opendash_sqlite",
        "Backup thiếu database/opendash.sqlite.",
        "The backup is missing database/opendash.sqlite.",
    ),
    (
        "server.error",
        "Không ghi được config mặc định.",
        "Could not write the default configuration.",
    ),
    (
        "server.error",
        "Không fsync được config mặc định.",
        "Could not sync the default configuration to disk.",
    ),
    (
        "server.error",
        "Từ chối publish restore: config/site.json chưa bật Safe Mode.",
        "Restore publication rejected: config/site.json does not enable Safe Mode.",
    ),
    (
        "server.error",
        "Điểm lỗi mô phỏng: AfterExtract",
        "Simulated failpoint: AfterExtract",
    ),
    (
        "server.error",
        "Điểm lỗi mô phỏng: AfterDbSanitize",
        "Simulated failpoint: AfterDbSanitize",
    ),
    (
        "server.error",
        "Không tuần tự hóa được READY.json.",
        "Could not serialize READY.json.",
    ),
    (
        "server.error",
        "Không ghi được READY.json.",
        "Could not write READY.json.",
    ),
    (
        "server.error",
        "Không fsync được READY.json.",
        "Could not sync READY.json to disk.",
    ),
    (
        "server.error",
        "Điểm lỗi mô phỏng: AfterReadyMarker",
        "Simulated failpoint: AfterReadyMarker",
    ),
    (
        "restore.the_restore_destination_directory_appeared_before_publication",
        "Thư mục đích khôi phục xuất hiện trước bước publish.",
        "The restore destination directory appeared before publication.",
    ),
    (
        "server.error",
        "Không thể publish thư mục khôi phục.",
        "Could not publish the restored directory.",
    ),
    (
        "auth.invalid_credentials",
        "Tên đăng nhập hoặc mật khẩu không chính xác.",
        "Incorrect username or password.",
    ),
    (
        "request.if_match_required",
        "Builder Save cần If-Match.",
        "Builder Save requires If-Match.",
    ),
    (
        "request.invalid_if_match",
        "If-Match không hợp lệ.",
        "Invalid If-Match.",
    ),
    (
        "request.invalid_if_match",
        "If-Match phải có dạng \"vN\".",
        "If-Match must have the format \"vN\".",
    ),
    (
        "request.unsupported_media_type",
        "Builder cần application/json.",
        "Builder requires application/json.",
    ),
    (
        "builder.builder_json_exceeds_256_kib",
        "Builder JSON vượt giới hạn 256 KiB.",
        "Builder JSON exceeds 256 KiB.",
    ),
    (
        "builder.invalid_content_type_key",
        "Content type key không hợp lệ.",
        "Invalid content type key.",
    ),
    (
        "builder.entry_builder_preview_target_is_fixed_by_the_path",
        "Đích xem trước Builder của nội dung được cố định bởi đường dẫn.",
        "Entry Builder preview target is fixed by the path.",
    ),
    (
        "contact.limit_must_be_in_1_100_and_offset_0",
        "limit phải trong 1..=100 và offset >= 0.",
        "limit must be in 1..=100 and offset >= 0.",
    ),
    (
        "contact.the_number_of_submitted_fields_exceeds_the_limit",
        "Số lượng trường gửi lên vượt quá giới hạn cho phép.",
        "The number of submitted fields exceeds the limit.",
    ),
    (
        "contact.form_version_was_submitted_more_than_once",
        "form_version bị gửi lặp lại.",
        "form_version was submitted more than once.",
    ),
    (
        "contact.invalid_form_version",
        "form_version không hợp lệ.",
        "Invalid form_version.",
    ),
    (
        "contact.nonce_was_submitted_more_than_once",
        "nonce bị gửi lặp lại.",
        "nonce was submitted more than once.",
    ),
    (
        "contact.honeypot_was_submitted_more_than_once",
        "honeypot bị gửi lặp lại.",
        "honeypot was submitted more than once.",
    ),
    (
        "contact.missing_form_version",
        "Thiếu form_version.",
        "Missing form_version.",
    ),
    ("contact.missing_nonce", "Thiếu nonce.", "Missing nonce."),
    (
        "content.content_type_not_found",
        "Không tìm thấy loại nội dung.",
        "Content type not found.",
    ),
    (
        "content.content_not_found",
        "Không tìm thấy nội dung.",
        "Content not found.",
    ),
    (
        "request.invalid_if_match",
        "Header If-Match không hợp lệ.",
        "Invalid If-Match header.",
    ),
    (
        "entries.the_content_table_query_is_too_long",
        "Truy vấn bảng nội dung quá dài.",
        "The content table query is too long.",
    ),
    (
        "entries.duplicate_table_query_parameters",
        "Tham số truy vấn bảng bị trùng.",
        "Duplicate table query parameters.",
    ),
    (
        "entries.the_search_query_is_too_long_or_invalid",
        "Tìm kiếm quá dài hoặc không hợp lệ.",
        "The search query is too long or invalid.",
    ),
    (
        "entries.invalid_locale_parameter",
        "Tham số locale không hợp lệ.",
        "Invalid locale parameter.",
    ),
    (
        "entries.invalid_category_id",
        "category_id không hợp lệ.",
        "Invalid category_id.",
    ),
    (
        "entries.invalid_tag_id",
        "tag_id không hợp lệ.",
        "Invalid tag_id.",
    ),
    (
        "entries.invalid_term_id",
        "term_id không hợp lệ.",
        "Invalid term_id.",
    ),
    (
        "entries.updated_month_must_be_yyyy_mm",
        "updated_month phải là YYYY-MM.",
        "updated_month must be YYYY-MM.",
    ),
    (
        "entries.limit_must_be_1_100",
        "limit phải là 1..100.",
        "limit must be 1..100.",
    ),
    (
        "entries.offset_must_be_0_1000000",
        "offset phải là 0..1000000.",
        "offset must be 0..1000000.",
    ),
    (
        "entries.invalid_content_table_parameters",
        "Tham số bảng nội dung không hợp lệ.",
        "Invalid content table parameters.",
    ),
    (
        "entries.taxonomy_key_term_id_pair_is_incomplete_or_aliases_are_mixed",
        "taxonomy_key/term_id thiếu cặp hoặc trộn aliases.",
        "taxonomy_key/term_id pair is incomplete or aliases are mixed.",
    ),
    (
        "entries.include_descendants_requires_a_taxonomy_filter",
        "include_descendants cần taxonomy filter.",
        "include_descendants requires a taxonomy filter.",
    ),
    (
        "entries.the_row_action_reads_the_latest_data_itself_client_supplied_row_json_is_not_accepted",
        "Row action tự đọc dữ liệu mới nhất; không chấp nhận JSON hàng do client gửi.",
        "The row action reads the latest data itself; client-supplied row JSON is not accepted.",
    ),
    (
        "entries.invalid_content_id",
        "ID nội dung không hợp lệ.",
        "Invalid content ID.",
    ),
    (
        "auth.forbidden",
        "Action không thuộc cột hiệu lực hoặc không đủ quyền.",
        "The action does not belong to an effective column or permission is insufficient.",
    ),
    (
        "request.precondition_failed",
        "Nội dung đã thay đổi trước khi chạy action; hãy tải lại hàng.",
        "The content changed before the action ran; please reload the row.",
    ),
    (
        "entries.invalid_key_for_the_content_type",
        "Khóa loại nội dung không hợp lệ.",
        "Invalid key for the content type.",
    ),
    (
        "request.if_match_required",
        "Cần If-Match với phiên bản hiện tại.",
        "If-Match with the current version is required.",
    ),
    (
        "request.invalid_if_match",
        "If-Match phải có dạng \"v1\".",
        "If-Match must have the format \"v1\".",
    ),
    (
        "entries.invalid_pagination",
        "Phân trang không hợp lệ.",
        "Invalid pagination.",
    ),
    (
        "entries.taxonomy_filter_ids_must_be_positive",
        "ID bộ lọc taxonomy phải dương.",
        "Taxonomy filter IDs must be positive.",
    ),
    (
        "entries.include_descendants_can_only_be_used_with_category_id",
        "include_descendants chỉ được dùng cùng category_id.",
        "include_descendants can only be used with category_id.",
    ),
    (
        "entries.the_locale_of_existing_content_cannot_be_changed",
        "Không thể đổi ngôn ngữ của nội dung đã tạo.",
        "The locale of existing content cannot be changed.",
    ),
    (
        "entries.only_post_page_support_draft_published",
        "Chỉ Post/Page hỗ trợ draft/published.",
        "Only Post/Page support draft/published.",
    ),
    (
        "entries.preview_does_not_support_pattern_section",
        "Preview không hỗ trợ Pattern/Section.",
        "Preview does not support Pattern/Section.",
    ),
    (
        "entries.preview_only_supports_post_page_or_cpts_with_builder_enabled",
        "Preview chỉ hỗ trợ Post/Page hoặc CPT bật Builder.",
        "Preview only supports Post/Page or CPTs with Builder enabled.",
    ),
    (
        "import.invalid_theme_id",
        "theme_id không hợp lệ.",
        "Invalid theme_id.",
    ),
    (
        "import.invalid_dry_run_data",
        "Dữ liệu dry-run không hợp lệ.",
        "Invalid dry-run data.",
    ),
    (
        "import.invalid_execute_data",
        "Dữ liệu execute không hợp lệ.",
        "Invalid execute data.",
    ),
    (
        "media.invalid_media_library_pagination",
        "Phân trang thư viện media không hợp lệ.",
        "Invalid Media Library pagination.",
    ),
    (
        "media.invalid_media_type_filter",
        "Bộ lọc loại media không hợp lệ.",
        "Invalid media type filter.",
    ),
    (
        "server.error",
        "Bộ xử lý ảnh không khả dụng.",
        "The image processor is unavailable.",
    ),
    (
        "server.error",
        "Không thể xử lý ảnh.",
        "Could not process the image.",
    ),
    (
        "media.too_many_concurrent_video_uploads",
        "Đang có quá nhiều lượt upload video đồng thời.",
        "Too many concurrent video uploads.",
    ),
    (
        "request.unsupported_media_type",
        "Chỉ hỗ trợ upload video định dạng video/mp4.",
        "Only video/mp4 uploads are supported.",
    ),
    (
        "server.error",
        "Không tạo được file tạm cho video.",
        "Could not create the temporary video file.",
    ),
    (
        "media.the_video_upload_stream_was_interrupted_or_exceeded_the_limit",
        "Luồng upload video bị gián đoạn hoặc vượt giới hạn.",
        "The video upload stream was interrupted or exceeded the limit.",
    ),
    (
        "media.the_video_size_exceeds_the_limit",
        "Kích thước video vượt giới hạn.",
        "The video size exceeds the limit.",
    ),
    (
        "media.the_video_exceeds_the_100_mib_limit",
        "Video vượt quá giới hạn 100 MiB.",
        "The video exceeds the 100 MiB limit.",
    ),
    (
        "server.error",
        "Không ghi được dữ liệu video tạm.",
        "Could not write temporary video data.",
    ),
    (
        "media.the_video_is_empty_0_bytes",
        "Video trống (0 byte).",
        "The video is empty (0 bytes).",
    ),
    (
        "server.error",
        "Không đồng bộ được file video tạm.",
        "Could not sync the temporary video file to disk.",
    ),
    (
        "server.error",
        "Không thể kiểm tra metadata video.",
        "Could not validate video metadata.",
    ),
    (
        "media.invalid_media_id",
        "Mã media không hợp lệ.",
        "Invalid media ID.",
    ),
    (
        "media.media_does_not_exist",
        "Media không tồn tại.",
        "Media does not exist.",
    ),
    (
        "media.the_video_has_no_thumbnail",
        "Video không có thumbnail.",
        "The video has no thumbnail.",
    ),
    (
        "menu.menu_not_found",
        "Không tìm thấy menu.",
        "Menu not found.",
    ),
    (
        "menu.theme_not_found",
        "Không tìm thấy theme.",
        "Theme not found.",
    ),
    (
        "operations.another_maintenance_job_is_running",
        "Đang có tác vụ bảo trì khác đang thực thi.",
        "Another maintenance job is running.",
    ),
    (
        "storage.busy",
        "Hệ thống đang bận xử lý ghi; vui lòng thử lại sau.",
        "The system is busy processing writes; please try again later.",
    ),
    (
        "operations.the_confirmation_password_does_not_match_the_backup_password",
        "Mật khẩu xác nhận không khớp với mật khẩu backup.",
        "The confirmation password does not match the backup password.",
    ),
    (
        "operations.request_id_must_contain_64_hexadecimal_characters",
        "request_id phải là 64 ký tự hex.",
        "request_id must contain 64 hexadecimal characters.",
    ),
    (
        "operations.request_id_was_already_used_with_different_configuration_parameters",
        "request_id đã được dùng với tham số cấu hình khác.",
        "request_id was already used with different configuration parameters.",
    ),
    (
        "operations.invalid_jobid",
        "JobId không hợp lệ.",
        "Invalid JobId.",
    ),
    (
        "operations.operations_job_not_found",
        "Không tìm thấy tác vụ vận hành.",
        "Operations job not found.",
    ),
    (
        "request.precondition_failed",
        "Phiên bản tác vụ đã thay đổi (CAS mismatch).",
        "The job version has changed (CAS mismatch).",
    ),
    (
        "request.precondition_failed",
        "Tác vụ đã ở trạng thái kết thúc.",
        "The job is already in a terminal state.",
    ),
    (
        "operations.the_limit_parameter_must_be_between_1_and_100",
        "Tham số limit phải từ 1 đến 100.",
        "The limit parameter must be between 1 and 100.",
    ),
    (
        "operations.invalid_backup_id",
        "Backup ID không hợp lệ.",
        "Invalid backup ID.",
    ),
    (
        "operations.backup_not_found",
        "Không tìm thấy bản sao lưu.",
        "Backup not found.",
    ),
    (
        "operations.the_backup_is_incomplete_or_does_not_exist",
        "Bản sao lưu chưa hoàn tất hoặc không tồn tại.",
        "The backup is incomplete or does not exist.",
    ),
    (
        "operations.missing_backup_metadata",
        "Thiếu metadata bản sao lưu.",
        "Missing backup metadata.",
    ),
    (
        "operations.the_backup_was_deleted",
        "Bản sao lưu đã bị xóa.",
        "The backup was deleted.",
    ),
    (
        "operations.backup_file_not_found_on_disk",
        "Không tìm thấy tệp backup trên đĩa.",
        "Backup file not found on disk.",
    ),
    (
        "server.error",
        "Content-Length không hợp lệ.",
        "Invalid content-length",
    ),
    (
        "server.error",
        "Content-Disposition không hợp lệ.",
        "Invalid content-disposition",
    ),
    (
        "operations.could_not_open_the_backup_file",
        "Không mở được tệp backup.",
        "Could not open the backup file.",
    ),
    (
        "request.precondition_failed",
        "Chỉ có thể xóa bản sao lưu đã hoàn tất.",
        "Only completed backups can be deleted.",
    ),
    (
        "operations.the_backup_was_already_deleted",
        "Bản sao lưu đã bị xóa trước đó.",
        "The backup was already deleted.",
    ),
    (
        "server.error",
        "Không tuần tự hóa được metadata xóa.",
        "Could not serialize deletion metadata.",
    ),
    (
        "server.error",
        "Không ghi được tệp upload restore.",
        "Could not write the uploaded restore file.",
    ),
    (
        "operations.the_upload_stream_was_interrupted",
        "Luồng upload bị gián đoạn.",
        "The upload stream was interrupted.",
    ),
    (
        "operations.the_backup_file_is_empty",
        "Tệp backup rỗng.",
        "The backup file is empty.",
    ),
    (
        "server.error",
        "Không đồng bộ được tệp upload restore.",
        "Could not sync the uploaded restore file to disk.",
    ),
    (
        "server.error",
        "Không hoàn tất được tệp upload restore.",
        "Could not finalize the uploaded restore file.",
    ),
    (
        "server.error",
        "Không tuần tự hóa được yêu cầu restore worker.",
        "Could not serialize the restore worker request.",
    ),
    (
        "server.error",
        "Không khởi chạy được restore worker.",
        "Could not start the restore worker.",
    ),
    (
        "server.error",
        "Không mở được stdin restore worker.",
        "Could not open restore worker stdin.",
    ),
    (
        "server.error",
        "Không gửi được yêu cầu tới restore worker.",
        "Could not send the request to the restore worker.",
    ),
    (
        "operations.the_restore_job_was_cancelled_by_an_administrator",
        "Tác vụ restore đã bị hủy bởi Admin.",
        "The restore job was cancelled by an administrator.",
    ),
    (
        "server.error",
        "Không đọc được kết quả restore worker.",
        "Could not read the restore worker result.",
    ),
    (
        "operations.the_restore_job_timed_out",
        "Tác vụ restore vượt quá thời gian cho phép.",
        "The restore job timed out.",
    ),
    (
        "server.error",
        "Lỗi theo dõi tiến trình restore worker.",
        "Error monitoring the restore worker process.",
    ),
    (
        "server.error",
        "Lỗi đồng bộ restore worker.",
        "Error synchronizing the restore worker.",
    ),
    (
        "operations.the_restore_worker_returned_an_invalid_response",
        "Restore worker trả về phản hồi không hợp lệ.",
        "The restore worker returned an invalid response.",
    ),
    (
        "operations.upload_id_must_contain_64_hexadecimal_characters",
        "upload_id phải là 64 ký tự hex.",
        "upload_id must contain 64 hexadecimal characters.",
    ),
    (
        "operations.the_passphrase_must_not_be_empty",
        "Passphrase không được để trống.",
        "The passphrase must not be empty.",
    ),
    (
        "operations.uploaded_backup_package_not_found",
        "Không tìm thấy gói backup đã tải lên.",
        "Uploaded backup package not found.",
    ),
    (
        "request.precondition_failed",
        "Không tìm thấy READY.json trong site root để kích hoạt.",
        "READY.json was not found in the site root for activation.",
    ),
    (
        "request.precondition_failed",
        "Generation READY.json không khớp (CAS mismatch).",
        "READY.json generation does not match (CAS mismatch).",
    ),
    (
        "auth.unauthorized",
        "Không tìm thấy tài khoản Admin hiện tại.",
        "Current administrator account not found.",
    ),
    (
        "auth.invalid_credentials",
        "Mật khẩu Admin không chính xác.",
        "Incorrect administrator password.",
    ),
    (
        "request.if_match_required",
        "Pattern cần If-Match để sửa/xóa.",
        "Pattern requires If-Match for updates or deletion.",
    ),
    (
        "plugin.content_type_must_indicate_a_zip_file",
        "Content-Type phải là file ZIP.",
        "Content-Type must indicate a ZIP file.",
    ),
    (
        "plugin.plugin_not_found",
        "Không tìm thấy plugin.",
        "Plugin not found.",
    ),
    (
        "plugin.invalid_plugin_settings_data",
        "Dữ liệu cài đặt plugin không hợp lệ.",
        "Invalid plugin settings data.",
    ),
    (
        "request.invalid_if_match",
        "Header If-Match phải là số nguyên.",
        "The If-Match header must be an integer.",
    ),
    (
        "plugin.active_plugin_not_found",
        "Không tìm thấy plugin đang hoạt động.",
        "Active plugin not found.",
    ),
    (
        "public_content.canonical_content_not_found",
        "Không tìm thấy canonical nội dung.",
        "Canonical content not found.",
    ),
    (
        "public_content.taxonomy_not_found",
        "Không tìm thấy taxonomy.",
        "Taxonomy not found.",
    ),
    (
        "server.error",
        "Không tìm thấy schema nội dung.",
        "Content schema not found.",
    ),
    (
        "public_content.public_image_does_not_exist",
        "Ảnh công khai không tồn tại.",
        "Public image does not exist.",
    ),
    (
        "auth.forbidden",
        "Cần quyền Editor hoặc Admin.",
        "Editor or Admin permission required.",
    ),
    (
        "seo.invalid_public_path",
        "Đường dẫn công khai không hợp lệ.",
        "Invalid public path.",
    ),
    (
        "request.if_match_required",
        "Cần If-Match để lưu cài đặt.",
        "If-Match is required to save settings.",
    ),
    (
        "request.invalid_if_match",
        "If-Match phải có định dạng \"vN\".",
        "If-Match must use the \"vN\" format.",
    ),
    (
        "settings.invalid_settings_data",
        "Dữ liệu Settings không hợp lệ.",
        "Invalid settings data.",
    ),
    (
        "request.precondition_failed",
        "Dữ liệu đã thay đổi ở một tab khác. Form chưa lưu vẫn được giữ lại.",
        "Data changed in another tab. Unsaved form data is preserved.",
    ),
    (
        "settings.invalid_timezone",
        "Múi giờ không có trong danh mục IANA tzdb 2026f.",
        "The time zone is not in the IANA tzdb 2026f catalog.",
    ),
    (
        "settings.secondary_timezone_locked",
        "Múi giờ là cấu hình toàn cục và chỉ được thay đổi ở ngôn ngữ mặc định.",
        "The time zone is a global setting and can only be changed in the default language.",
    ),
    (
        "locale.invalid_language_creation_data",
        "Dữ liệu tạo ngôn ngữ không hợp lệ.",
        "Invalid language creation data.",
    ),
    (
        "locale.invalid_language_update_data",
        "Dữ liệu cập nhật ngôn ngữ không hợp lệ.",
        "Invalid language update data.",
    ),
    (
        "settings.the_set_default_body_must_be_empty_or",
        "Body của set-default phải rỗng hoặc {}.",
        "The set-default body must be empty or {}.",
    ),
    (
        "settings.invalid_public_settings_data",
        "Dữ liệu cài đặt công khai không hợp lệ.",
        "Invalid public settings data.",
    ),
    (
        "request.precondition_failed",
        "Cài đặt công khai đã thay đổi ở một tab khác. Form chưa lưu vẫn được giữ lại.",
        "Public settings changed in another tab. Unsaved form data is preserved.",
    ),
    (
        "locale.the_site_has_multiple_languages_enabled_manage_the_default_language_under_content_languages",
        "Website đang bật nhiều ngôn ngữ; hãy quản lý ngôn ngữ mặc định tại phần Ngôn ngữ nội dung.",
        "The site has multiple languages enabled; manage the default language under Content Languages.",
    ),
    (
        "settings.invalid_permalink_configuration",
        "Cấu hình permalink không hợp lệ.",
        "Invalid permalink configuration.",
    ),
    (
        "settings.invalid_reading_settings",
        "Reading Settings không hợp lệ.",
        "Invalid Reading Settings.",
    ),
    (
        "settings.invalid_theme_options_data",
        "Dữ liệu theme options không hợp lệ.",
        "Invalid theme options data.",
    ),
    (
        "request.precondition_failed",
        "Theme options đã thay đổi ở phiên khác. Tải lại trước khi lưu.",
        "Theme options changed in another session. Reload before saving.",
    ),
    (
        "settings.invalid_site_wide_options_data",
        "Dữ liệu tùy chọn toàn trang không hợp lệ.",
        "Invalid site-wide options data.",
    ),
    (
        "request.precondition_failed",
        "Tùy chọn toàn trang đã thay đổi ở phiên khác. Tải lại trước khi lưu.",
        "Site-wide options changed in another session. Reload before saving.",
    ),
    (
        "sitemap.public_origin_must_be_configured_to_generate_a_sitemap",
        "Chưa cấu hình public_origin để tạo sitemap.",
        "public_origin must be configured to generate a sitemap.",
    ),
    (
        "sitemap.the_url_count_exceeds_the_maximum_sitemap_limit_1_000_000",
        "Số lượng URL vượt quá giới hạn sitemap tối đa (1.000.000).",
        "The URL count exceeds the maximum sitemap limit (1,000,000).",
    ),
    (
        "sitemap.sitemap_part_not_found",
        "Không tìm thấy phần sitemap.",
        "Sitemap part not found.",
    ),
    (
        "smtp.the_smtp_port_must_be_in_1_65535",
        "Cổng SMTP phải trong khoảng 1..=65535.",
        "The SMTP port must be in 1..=65535.",
    ),
    (
        "smtp.invalid_smtp_username",
        "Tên đăng nhập SMTP không hợp lệ.",
        "Invalid SMTP username.",
    ),
    (
        "smtp.a_new_password_is_required_when_mode_replace",
        "Thiếu mật khẩu mới khi mode=replace.",
        "A new password is required when mode=replace.",
    ),
    (
        "taxonomy.at_most_100_term_ids_can_be_looked_up_at_a_time",
        "Tối đa 100 ID term mỗi lần tra cứu.",
        "At most 100 term IDs can be looked up at a time.",
    ),
    (
        "taxonomy.the_term_id_list_is_invalid_or_exceeds_100_ids",
        "Danh sách ID term không hợp lệ hoặc vượt 100 ID.",
        "The term ID list is invalid or exceeds 100 IDs.",
    ),
    (
        "taxonomy.invalid_term_id",
        "ID term không hợp lệ.",
        "Invalid term ID.",
    ),
    (
        "taxonomy.the_term_id_list_is_invalid_or_contains_duplicates",
        "Danh sách ID term không hợp lệ hoặc trùng.",
        "The term ID list is invalid or contains duplicates.",
    ),
    (
        "taxonomy.invalid_taxonomy_json_data",
        "Dữ liệu JSON taxonomy không hợp lệ.",
        "Invalid taxonomy JSON data.",
    ),
    (
        "taxonomy.the_term_id_must_be_a_positive_integer",
        "ID term phải là số nguyên dương.",
        "The term ID must be a positive integer.",
    ),
    (
        "request.if_match_required",
        "Cần If-Match phiên bản term.",
        "If-Match with the term version is required.",
    ),
    (
        "taxonomy.invalid_taxonomy_key",
        "Khóa taxonomy không hợp lệ.",
        "Invalid taxonomy key.",
    ),
    (
        "taxonomy.taxonomy_does_not_exist",
        "Taxonomy không tồn tại.",
        "Taxonomy does not exist.",
    ),
    (
        "taxonomy.invalid_taxonomy_pagination",
        "Phân trang taxonomy không hợp lệ.",
        "Invalid taxonomy pagination.",
    ),
    (
        "taxonomy.term_not_found",
        "Không tìm thấy term.",
        "Term not found.",
    ),
    (
        "taxonomy.name_slug_parent_id_featured_image_or_extensions_is_required_for_an_update",
        "Cần name, slug, parent_id, featured_image_media_id hoặc extensions để cập nhật.",
        "name, slug, parent_id, featured_image_media_id or extensions is required for an update.",
    ),
    (
        "taxonomy.invalid_featured_image_id",
        "ID ảnh đại diện không hợp lệ.",
        "Invalid featured image ID.",
    ),
    (
        "taxonomy.featured_image_not_found_or_inactive",
        "Ảnh đại diện không tồn tại hoặc không phải ảnh đang hoạt động.",
        "Featured image not found or is not an active image.",
    ),
    (
        "request.invalid_if_match",
        "expected_version không khớp If-Match.",
        "expected_version does not match If-Match.",
    ),
    (
        "taxonomy.invalid_cpt_key",
        "Khóa CPT không hợp lệ.",
        "Invalid CPT key.",
    ),
    (
        "themes.no_template_override_to_reset",
        "Không có template override để reset.",
        "No template override to reset.",
    ),
    (
        "request.unsupported_media_type",
        "Yêu cầu application/json.",
        "Expected application/json.",
    ),
    (
        "themes.studio_preview_exceeds_512_kib",
        "Bản xem trước Studio vượt quá 512 KiB.",
        "Studio preview exceeds 512 KiB.",
    ),
    (
        "themes.invalid_studio_preview_json",
        "JSON bản xem trước Studio không hợp lệ.",
        "Invalid studio preview JSON.",
    ),
    (
        "themes.template_preview_exceeds_256_kib",
        "Bản xem trước template vượt quá 256 KiB.",
        "Template preview exceeds 256 KiB.",
    ),
    (
        "themes.invalid_template_preview_json",
        "JSON bản xem trước template không hợp lệ.",
        "Invalid template preview JSON.",
    ),
    (
        "request.if_match_required",
        "Cần If-Match để kích hoạt theme.",
        "If-Match is required to activate the theme.",
    ),
    (
        "request.unsupported_media_type",
        "Cần gói application/zip.",
        "An application/zip package is required.",
    ),
    (
        "themes.invalid_activation_data",
        "Dữ liệu kích hoạt không hợp lệ.",
        "Invalid activation data.",
    ),
    (
        "themes.the_theme_revision_is_not_installed",
        "Revision theme chưa được cài đặt.",
        "The theme revision is not installed.",
    ),
    (
        "request.precondition_failed",
        "Theme đã thay đổi ở phiên khác (412). Tải lại trạng thái trước khi thử tiếp.",
        "The theme changed in another session (412). Reload the state before trying again.",
    ),
    (
        "themes.the_revision_on_disk_is_missing_or_does_not_match_complete_digest",
        "Revision trên đĩa bị thiếu hoặc không khớp .complete/digest.",
        "The revision on disk is missing or does not match .complete/digest.",
    ),
    (
        "themes.a_v3_theme_revision_id_is_required",
        "Cần revision_id theme v3.",
        "A v3 theme revision_id is required.",
    ),
    (
        "request.invalid_json",
        "Dữ liệu JSON không hợp lệ.",
        "Invalid JSON data.",
    ),
    (
        "users.role_must_be_admin_or_editor",
        "Vai trò phải là admin hoặc editor.",
        "Role must be admin or editor.",
    ),
    (
        "users.the_user_id_must_be_a_positive_integer",
        "ID người dùng phải là số nguyên dương.",
        "The user ID must be a positive integer.",
    ),
    (
        "request.if_match_required",
        "Cần If-Match phiên bản người dùng.",
        "If-Match with the user version is required.",
    ),
    (
        "users.username_or_role_is_required",
        "Cần username hoặc role.",
        "username or role is required.",
    ),
    (
        "users.username_or_new_password_is_required",
        "Cần username hoặc new_password.",
        "username or new_password is required.",
    ),
    (
        "users.invalid_current_password_enter_the_current_password_to_change_your_password",
        "invalid_current_password: Phải nhập mật khẩu hiện tại khi đổi mật khẩu.",
        "invalid_current_password: Enter the current password to change your password.",
    ),
    (
        "users.invalid_current_password_the_current_password_is_incorrect",
        "invalid_current_password: Mật khẩu hiện tại không đúng.",
        "invalid_current_password: The current password is incorrect.",
    ),
    (
        "secrets.invalid_secretref",
        "SecretRef không hợp lệ.",
        "Invalid SecretRef.",
    ),
    (
        "server.error",
        "Không thể tạo thư mục bí mật.",
        "Could not create the secrets directory.",
    ),
    (
        "server.error",
        "Không thể tạo thư mục bí mật SMTP.",
        "Could not create the SMTP secrets directory.",
    ),
    (
        "secrets.the_secret_file_exceeds_the_limit",
        "Tệp bí mật vượt quá giới hạn.",
        "The secret file exceeds the limit.",
    ),
    (
        "server.error",
        "Không thể ghi tệp bí mật.",
        "Could not write the secret file.",
    ),
    (
        "server.error",
        "Không thể đồng bộ tệp bí mật.",
        "Could not sync the secret file to disk.",
    ),
    (
        "server.error",
        "Không thể hoàn tất tệp bí mật.",
        "Could not finalize the secret file.",
    ),
    (
        "server.error",
        "Không tìm thấy tệp bí mật.",
        "Secret file not found.",
    ),
    (
        "server.error",
        "Tệp bí mật không hợp lệ.",
        "Invalid secret file.",
    ),
    (
        "server.error",
        "Không thể đọc tệp bí mật.",
        "Could not read the secret file.",
    ),
    (
        "server.error",
        "Không thể đọc dữ liệu bí mật.",
        "Could not read secret data.",
    ),
    (
        "secrets.the_smtp_password_exceeds_1024_utf_8_bytes",
        "Mật khẩu SMTP vượt quá giới hạn 1024 byte UTF-8.",
        "The SMTP password exceeds 1024 UTF-8 bytes.",
    ),
    (
        "server.error",
        "Không thể mã hóa bí mật SMTP.",
        "Could not encrypt the SMTP secret.",
    ),
    (
        "server.error",
        "Dữ liệu bí mật SMTP bị hỏng.",
        "The SMTP secret data is corrupted.",
    ),
    (
        "server.error",
        "Phiên bản bí mật SMTP không hợp lệ.",
        "Invalid SMTP secret version.",
    ),
    (
        "secrets.the_plugin_secret_exceeds_4096_utf_8_bytes",
        "Bí mật plugin vượt quá giới hạn 4096 byte UTF-8.",
        "The plugin secret exceeds 4096 UTF-8 bytes.",
    ),
    (
        "server.error",
        "Không thể mã hóa bí mật plugin.",
        "Could not encrypt the plugin secret.",
    ),
    (
        "server.error",
        "Dữ liệu bí mật plugin bị hỏng.",
        "The plugin secret data is corrupted.",
    ),
    (
        "server.error",
        "Phiên bản bí mật plugin không hợp lệ.",
        "Invalid plugin secret version.",
    ),
    (
        "server.error",
        "Từ chối symlink tại kho bí mật.",
        "Symlink rejected in secret storage.",
    ),
    (
        "server.error",
        "Từ chối reparse point tại kho bí mật.",
        "Reparse point rejected in secret storage.",
    ),
    (
        "server.error",
        "Không thể kiểm tra metadata kho bí mật.",
        "Could not check secret storage metadata.",
    ),
    (
        "server.error",
        "Không thể tạo thư mục riêng tư.",
        "Could not create the private directory.",
    ),
    (
        "server.error",
        "Không thể tạo tệp bí mật mới.",
        "Could not create a new secret file.",
    ),
    (
        "server.error",
        "Không thể đặt quyền 0700 cho thư mục bí mật.",
        "Could not set permissions to 0700 for the secrets directory.",
    ),
    (
        "server.error",
        "Không thể đọc quyền thư mục bí mật.",
        "Could not read secrets directory permissions.",
    ),
    (
        "server.error",
        "Quyền thư mục bí mật quá rộng (yêu cầu 0700).",
        "Secrets directory permissions are too broad (0700 required).",
    ),
    (
        "server.error",
        "Không thể đọc quyền tệp bí mật.",
        "Could not read secret file permissions.",
    ),
    (
        "server.error",
        "Quyền tệp bí mật quá rộng (yêu cầu 0600).",
        "Secret file permissions are too broad (0600 required).",
    ),
    (
        "server.error",
        "Không thể mở process token.",
        "Could not open the process token.",
    ),
    (
        "server.error",
        "Không thể lấy kích thước TokenUser.",
        "Could not get the TokenUser size.",
    ),
    (
        "server.error",
        "Không thể đọc TokenUser SID.",
        "Could not read the TokenUser SID.",
    ),
    (
        "server.error",
        "Không thể tạo SYSTEM SID.",
        "Could not create the SYSTEM SID.",
    ),
    (
        "server.error",
        "SetEntriesInAclW thất bại.",
        "SetEntriesInAclW failed.",
    ),
    (
        "server.error",
        "SetNamedSecurityInfoW thất bại.",
        "SetNamedSecurityInfoW failed.",
    ),
    (
        "server.error",
        "Không thể đọc DACL bảo vệ.",
        "Could not read the protected DACL.",
    ),
    (
        "server.error",
        "Không thể đọc control flags DACL.",
        "Could not read DACL control flags.",
    ),
    (
        "server.error",
        "DACL chưa được bật PROTECTED_DACL.",
        "PROTECTED_DACL is not enabled on the DACL.",
    ),
    (
        "server.error",
        "Không thể đọc ACL_SIZE_INFORMATION.",
        "Could not read ACL_SIZE_INFORMATION.",
    ),
    (
        "server.error",
        "DACL rỗng không hợp lệ.",
        "An empty DACL is invalid.",
    ),
    (
        "server.error",
        "Không thể đọc ACE trong DACL.",
        "Could not read an ACE in the DACL.",
    ),
    (
        "server.error",
        "Phát hiện ACE không hợp lệ trong DACL.",
        "An invalid ACE was found in the DACL.",
    ),
    (
        "server.error",
        "DACL chứa SID ngoài service identity và SYSTEM.",
        "The DACL contains a SID other than the service identity and SYSTEM.",
    ),
    (
        "seo.invalid_og_image_id",
        "ID ảnh OG không hợp lệ.",
        "Invalid OG image ID.",
    ),
    (
        "seo.invalid_canonical_url",
        "Canonical URL không hợp lệ.",
        "Invalid canonical URL.",
    ),
    (
        "seo.the_canonical_url_must_be_a_valid_http_https_url_without_a_fragment_or_userinfo",
        "Canonical URL phải là URL http/https hợp lệ không có fragment hoặc userinfo.",
        "The canonical URL must be a valid HTTP/HTTPS URL without a fragment or userinfo.",
    ),
    (
        "seo.a_sitemap_url_exceeds_2048_bytes",
        "URL trong sitemap vượt quá giới hạn 2048 byte.",
        "A sitemap URL exceeds 2048 bytes.",
    ),
    (
        "seo.the_sitemap_part_exceeds_50_mib",
        "Phần sitemap vượt quá giới hạn 50 MiB.",
        "The sitemap part exceeds 50 MiB.",
    ),
    (
        "smtp.invalid_smtp_host_1_253_ascii_characters",
        "Host SMTP không hợp lệ (1..=253 ký tự ASCII).",
        "Invalid SMTP host (1..=253 ASCII characters).",
    ),
    (
        "smtp.the_smtp_host_only_accepts_an_ascii_hostname_or_ip_address_without_url_path_or_credentials",
        "Host SMTP chỉ chấp nhận tên miền ASCII hoặc địa chỉ IP, không chứa URL/path/thông tin đăng nhập.",
        "The SMTP host only accepts an ASCII hostname or IP address, without URL, path or credentials.",
    ),
    (
        "smtp.the_smtp_host_must_not_include_a_port",
        "Host SMTP không được chứa cổng.",
        "The SMTP host must not include a port.",
    ),
    (
        "smtp.invalid_smtp_domain_name",
        "Tên miền SMTP không hợp lệ.",
        "Invalid SMTP domain name.",
    ),
    (
        "smtp.the_system_is_in_safe_mode_after_restoration_smtp_is_disabled",
        "Hệ thống đang ở safe mode sau khi khôi phục; SMTP đang bị tắt.",
        "The system is in safe mode after restoration; SMTP is disabled.",
    ),
    (
        "server.error",
        "Không thể cấp phép kết nối SMTP.",
        "Could not authorize the SMTP connection.",
    ),
    (
        "server.error",
        "admin_extensions registry không hợp lệ.",
        "Invalid admin_extensions registry.",
    ),
    (
        "admin_extensions.admin_extensions_exceeds_32_effective_columns_for_the_same_content_type",
        "admin_extensions vượt giới hạn 32 cột hiệu lực cho cùng loại nội dung.",
        "admin_extensions exceeds 32 effective columns for the same content type.",
    ),
    (
        "admin_extensions.invalid_admin_extensions_owner_status",
        "Owner/status admin_extensions không hợp lệ.",
        "Invalid admin_extensions owner/status.",
    ),
    (
        "admin_extensions.the_panel_was_withdrawn_from_the_registry_or_permission_is_insufficient",
        "Panel đã rút khỏi registry hoặc không có quyền.",
        "The panel was withdrawn from the registry or permission is insufficient.",
    ),
    (
        "admin_extensions.panel_extensions_must_be_a_json_object",
        "extensions panel phải là JSON object.",
        "Panel extensions must be a JSON object.",
    ),
    (
        "auth.forbidden",
        "Không có quyền cập nhật extension field.",
        "Permission to update the extension field is missing.",
    ),
    (
        "admin_extensions.schema_token_changed_keep_your_input_and_reload_the_configuration",
        "schema_token đã thay đổi, giữ bản nhập và tải lại cấu hình.",
        "schema_token changed; keep your input and reload the configuration.",
    ),
    (
        "admin_extensions.invalid_extension_key",
        "Extension key không hợp lệ.",
        "Invalid extension key.",
    ),
    (
        "server.error",
        "Dữ liệu extension đã lưu không hợp lệ.",
        "Invalid stored extension data.",
    ),
    (
        "admin_extensions.a_required_field_cannot_be_cleared_with_null",
        "Trường bắt buộc không được xóa null.",
        "A required field cannot be cleared with null.",
    ),
    (
        "admin_extensions.a_boolean_extension_must_be_true_false",
        "Extension boolean phải là true/false.",
        "A boolean extension must be true/false.",
    ),
    (
        "admin_extensions.a_text_extension_must_be_a_string",
        "Extension text phải là chuỗi.",
        "A text extension must be a string.",
    ),
    (
        "admin_extensions.extension_text_exceeds_4096_characters_or_contains_control_characters",
        "Extension text vượt 4096 hoặc chứa ký tự điều khiển.",
        "Extension text exceeds 4096 characters or contains control characters.",
    ),
    (
        "admin_extensions.a_required_extension_field_must_not_be_empty",
        "Extension field bắt buộc không được rỗng.",
        "A required extension field must not be empty.",
    ),
    (
        "admin_extensions.content_type_is_unavailable",
        "Loại nội dung không khả dụng.",
        "Content type is unavailable.",
    ),
    (
        "admin_extensions.the_effective_admin_column_limit_was_exceeded",
        "Vượt giới hạn cột admin hiệu lực.",
        "The effective Admin column limit was exceeded.",
    ),
    (
        "server.error",
        "Vai trò người dùng không hợp lệ.",
        "Invalid user role.",
    ),
    (
        "auth.the_username_already_exists",
        "Tên đăng nhập đã tồn tại.",
        "The username already exists.",
    ),
    (
        "storage.busy",
        "Dữ liệu đang được cập nhật, vui lòng thử lại.",
        "Data is being updated; please try again.",
    ),
    (
        "auth.unauthorized",
        "Phiên đăng nhập đã bị thu hồi.",
        "The session has been revoked.",
    ),
    (
        "auth.forbidden",
        "Chỉ quản trị viên được quản lý tài khoản.",
        "Only administrators can manage accounts.",
    ),
    (
        "auth.user_not_found",
        "Không tìm thấy người dùng.",
        "User not found.",
    ),
    (
        "request.precondition_failed",
        "Tài khoản đã được cập nhật, hãy tải lại.",
        "The account was updated; please reload.",
    ),
    (
        "auth.the_last_administrator_cannot_be_deleted_or_demoted",
        "Không thể xóa hoặc hạ quyền Admin cuối cùng.",
        "The last administrator cannot be deleted or demoted.",
    ),
    (
        "auth.the_system_has_already_been_set_up",
        "Hệ thống đã được thiết lập.",
        "The system has already been set up.",
    ),
    (
        "auth.account_not_found",
        "Không tìm thấy tài khoản.",
        "Account not found.",
    ),
    (
        "server.error",
        "Vai trò tài khoản không hợp lệ.",
        "Invalid account role.",
    ),
    (
        "auth.unauthorized",
        "Phiên đăng nhập không hợp lệ.",
        "Invalid session.",
    ),
    (
        "request.precondition_failed",
        "Phiên bản tài khoản đã thay đổi. Hãy tải lại trước khi lưu.",
        "The account version changed. Reload before saving.",
    ),
    (
        "server.error",
        "Đường dẫn snapshot DB không phải UTF-8 hợp lệ.",
        "The snapshot database path is not valid UTF-8.",
    ),
    (
        "server.error",
        "Không mở được kết nối SQLite nguồn để tạo snapshot.",
        "Could not open the source SQLite connection to create a snapshot.",
    ),
    (
        "server.error",
        "VACUUM main INTO thất bại khi tạo snapshot.",
        "VACUUM main INTO failed while creating the snapshot.",
    ),
    (
        "server.error",
        "Không đóng được kết nối SQLite nguồn.",
        "Could not close the source SQLite connection.",
    ),
    (
        "server.error",
        "Không mở được DB snapshot để kiểm tra toàn vẹn.",
        "Could not open the snapshot database for integrity checking.",
    ),
    (
        "server.error",
        "Kiểm tra integrity_check trên snapshot DB thất bại.",
        "integrity_check failed on the snapshot database.",
    ),
    (
        "server.error",
        "Kiểm tra foreign_key_check trên snapshot DB phát hiện lỗi.",
        "foreign_key_check detected errors in the snapshot database.",
    ),
    (
        "backup.a_media_deletion_job_is_incomplete_could_not_create_the_snapshot",
        "Đang có media delete job chưa hoàn tất; không thể tạo snapshot.",
        "A media deletion job is incomplete; could not create the snapshot.",
    ),
    (
        "builder.builder_exceeds_the_limit_of_8_lists",
        "Builder vượt giới hạn 8 list.",
        "Builder exceeds the limit of 8 lists.",
    ),
    (
        "server.error",
        "ID block khai báo không hợp lệ.",
        "Invalid declared block ID",
    ),
    (
        "builder.invalid_builder_media_id",
        "ID media Builder không hợp lệ.",
        "Invalid Builder media ID",
    ),
    (
        "builder.builder_media_is_missing_or_has_the_wrong_type_in_the_media_library",
        "Builder media không tồn tại hoặc không đúng loại trong Media Library.",
        "Builder media is missing or has the wrong type in the Media Library.",
    ),
    (
        "builder.builder_exceeds_16_pattern_references",
        "Builder vượt quá 16 tham chiếu Pattern.",
        "Builder exceeds 16 Pattern references.",
    ),
    (
        "builder.expanded_pattern_exceeds_1024_builder_nodes",
        "Pattern sau khi mở rộng vượt quá 1024 node Builder.",
        "Expanded Pattern exceeds 1024 Builder nodes.",
    ),
    (
        "builder.the_content_type_does_not_have_a_valid_body_field_for_page_builder",
        "Loại nội dung không có trường 'body' hợp lệ cho Trình dựng trang.",
        "The content type does not have a valid 'body' field for Page Builder.",
    ),
    (
        "builder.the_body_field_must_be_rich_text_to_use_page_builder",
        "Trường 'body' phải có kiểu rich_text để dùng Trình dựng trang.",
        "The 'body' field must be rich_text to use Page Builder.",
    ),
    (
        "auth.forbidden",
        "Bạn không có quyền chỉnh sửa nội dung 'body'.",
        "You do not have permission to edit 'body' content.",
    ),
    (
        "builder.content_builder_is_not_enabled",
        "Content Builder chưa được bật.",
        "Content Builder is not enabled.",
    ),
    (
        "builder.content_type_not_found",
        "Không tìm thấy loại nội dung.",
        "Content type not found.",
    ),
    (
        "builder.content_entry_not_found",
        "Không tìm thấy mục nội dung.",
        "Content entry not found.",
    ),
    (
        "server.error",
        "Chế độ nội dung đã lưu không hợp lệ.",
        "Invalid stored entry content mode.",
    ),
    (
        "server.error",
        "Không thể đọc tài liệu Builder của nội dung đã lưu.",
        "Stored entry Builder document is not readable.",
    ),
    (
        "builder.pattern_builder_requires_admin",
        "Pattern Builder yêu cầu quyền Admin.",
        "Pattern Builder requires Admin.",
    ),
    (
        "builder.pattern_cannot_contain_patternref_or_contentlist_depth_1",
        "Pattern không được chứa PatternRef hoặc ContentList (độ sâu = 1).",
        "Pattern cannot contain PatternRef or ContentList (depth = 1).",
    ),
    (
        "builder.referenced_pattern_does_not_exist",
        "Pattern được tham chiếu không tồn tại.",
        "Referenced Pattern does not exist.",
    ),
    (
        "builder.template_only_element_in_content_builder",
        "Phần tử chỉ dành cho template trong Content Builder.",
        "Template-only element in Content Builder.",
    ),
    (
        "builder.builder_cover_field_is_not_accessible",
        "Không thể truy cập trường ảnh bìa Builder.",
        "Builder cover field is not accessible.",
    ),
    (
        "builder.builder_binding_field_is_not_accessible",
        "Không thể truy cập trường liên kết Builder.",
        "Builder binding field is not accessible.",
    ),
    (
        "request.precondition_failed",
        "Phiên bản nội dung đã thay đổi.",
        "Entry version changed.",
    ),
    (
        "builder.no_saved_content_builder_document",
        "Không có tài liệu Content Builder đã lưu.",
        "No saved Content Builder document.",
    ),
    (
        "request.precondition_failed",
        "Schema nội dung đã thay đổi trong thao tác Builder.",
        "Content schema changed during Builder operation.",
    ),
    (
        "builder.invalid_builder_json_encoding",
        "Mã hóa JSON Builder không hợp lệ.",
        "Invalid Builder JSON encoding.",
    ),
    (
        "request.precondition_failed",
        "Nội dung đã thay đổi trong thao tác Builder.",
        "Entry changed during Builder operation.",
    ),
    (
        "builder.comment_block_body_exceeds_richtext_validation_limit",
        "Nội dung khối bình luận vượt giới hạn kiểm tra RichText.",
        "Comment block body exceeds RichText validation limit.",
    ),
    (
        "builder.entry_json_exceeds_768_kib_limit",
        "JSON nội dung vượt giới hạn 768 KiB.",
        "Entry JSON exceeds 768 KiB limit.",
    ),
    (
        "builder.reset_requires_confirmation",
        "Cần xác nhận để đặt lại.",
        "Reset requires confirmation.",
    ),
    (
        "contact.the_form_slug_must_be_ascii_kebab_case_at_most_64_bytes",
        "Slug form phải là kebab-case ASCII (tối đa 64 byte).",
        "The form slug must be ASCII kebab-case (at most 64 bytes).",
    ),
    (
        "contact.invalid_form_title_1_160_characters",
        "Tiêu đề form không hợp lệ (1..=160 ký tự).",
        "Invalid form title (1..=160 characters).",
    ),
    (
        "contact.the_form_status_must_be_active_or_disabled",
        "Trạng thái form phải là active hoặc disabled.",
        "The form status must be active or disabled.",
    ),
    (
        "contact.the_form_slug_already_exists",
        "Slug form đã tồn tại.",
        "The form slug already exists.",
    ),
    (
        "request.precondition_failed",
        "Form đã thay đổi ở phiên bản khác.",
        "The form changed in another version.",
    ),
    (
        "contact.a_form_with_submissions_cannot_be_deleted_set_it_to_disabled",
        "Không thể xóa form đã có dữ liệu gửi; hãy chuyển sang trạng thái disabled.",
        "A form with submissions cannot be deleted; set it to disabled.",
    ),
    (
        "contact.contact_submission_not_found",
        "Không tìm thấy lượt gửi liên hệ.",
        "Contact submission not found.",
    ),
    (
        "contact.a_submission_cannot_be_deleted_while_its_smtp_notification_is_being_sent",
        "Không thể xóa lượt gửi khi đang gửi thông báo SMTP.",
        "A submission cannot be deleted while its SMTP notification is being sent.",
    ),
    (
        "contact.duplicate_request_with_different_content",
        "Yêu cầu trùng lặp với nội dung khác.",
        "Duplicate request with different content.",
    ),
    (
        "server.error",
        "Chủ sở hữu loại nội dung không hợp lệ.",
        "Invalid content type owner.",
    ),
    (
        "server.error",
        "Trạng thái loại nội dung không hợp lệ.",
        "Invalid content type status.",
    ),
    (
        "server.error",
        "Trạng thái declaration loại nội dung không hợp lệ.",
        "Invalid content type declaration status.",
    ),
    (
        "server.error",
        "Dữ liệu loại nội dung không hợp lệ.",
        "Invalid content type data.",
    ),
    (
        "server.error",
        "Dữ liệu fields_json của field group không hợp lệ.",
        "Invalid field group fields_json data.",
    ),
    (
        "server.error",
        "default_region của field group không hợp lệ.",
        "Invalid field group default_region.",
    ),
    (
        "server.error",
        "Trạng thái field group không hợp lệ.",
        "Invalid field group status.",
    ),
    (
        "content.the_content_type_key_conflicts_with_a_reserved_system_prefix",
        "Khóa loại nội dung trùng với tiền tố hệ thống được bảo vệ.",
        "The content type key conflicts with a reserved system prefix.",
    ),
    (
        "content.the_content_type_key_conflicts_with_an_existing_taxonomy_key",
        "Khóa loại nội dung trùng với khóa taxonomy đang tồn tại.",
        "The content type key conflicts with an existing taxonomy key.",
    ),
    (
        "content.post_page_are_protected_core_keys",
        "post/page là khóa Core được bảo vệ.",
        "post/page are protected Core keys.",
    ),
    (
        "content.the_content_type_key_already_exists",
        "Khóa loại nội dung đã tồn tại.",
        "The content type key already exists.",
    ),
    (
        "server.error",
        "Không thể tải schema vừa tạo.",
        "Could not load the newly created schema.",
    ),
    (
        "content.the_content_type_key_in_the_body_does_not_match_the_url_path",
        "Khóa loại nội dung trong body không khớp với đường dẫn URL.",
        "The content type key in the body does not match the URL path.",
    ),
    (
        "content.the_system_pattern_cannot_be_changed",
        "Không thể thay đổi Pattern hệ thống.",
        "The system Pattern cannot be changed.",
    ),
    (
        "content.only_user_owned_content_types_can_be_updated",
        "Chỉ có thể cập nhật loại nội dung thuộc quyền sở hữu của User.",
        "Only User-owned content types can be updated.",
    ),
    (
        "request.precondition_failed",
        "Phiên bản loại nội dung đã thay đổi. Vui lòng tải lại.",
        "The content type version changed. Please reload.",
    ),
    (
        "server.error",
        "Không thể tải schema hiện tại.",
        "Could not load the current schema.",
    ),
    (
        "content.post_page_only_support_behavior_settings_changes_the_core_schema_must_remain_unchanged",
        "Post/Page chỉ hỗ trợ đổi cấu hình hành vi; schema Core phải giữ nguyên.",
        "Post/Page only support behavior settings changes; the Core schema must remain unchanged.",
    ),
    (
        "content.cannot_switch_to_immediate_saving_while_the_content_type_still_has_drafts",
        "Không thể chuyển sang lưu ngay khi loại nội dung còn bản nháp.",
        "Cannot switch to immediate saving while the content type still has drafts.",
    ),
    (
        "server.error",
        "Không thể tải schema sau cập nhật.",
        "Could not load the updated schema.",
    ),
    (
        "content.the_field_group_group_id_or_local_key_already_exists",
        "group_id hoặc local_key của nhóm trường đã tồn tại.",
        "The field group group_id or local_key already exists.",
    ),
    (
        "content.the_target_content_type_does_not_exist",
        "Loại nội dung mục tiêu không tồn tại.",
        "The target content type does not exist.",
    ),
    (
        "server.error",
        "Không thể tải schema mục tiêu.",
        "Could not load the target schema.",
    ),
    (
        "server.error",
        "Không thể tải field group vừa tạo.",
        "Could not load the newly created field group.",
    ),
    (
        "content.field_group_not_found",
        "Không tìm thấy nhóm trường.",
        "Field group not found.",
    ),
    (
        "content.only_user_owned_field_groups_can_be_updated",
        "Chỉ có thể cập nhật nhóm trường thuộc quyền sở hữu của User.",
        "Only User-owned field groups can be updated.",
    ),
    (
        "content.the_field_group_declaration_was_withdrawn_it_cannot_be_updated",
        "Nhóm trường đã bị rút khai báo; không thể cập nhật.",
        "The field group declaration was withdrawn; it cannot be updated.",
    ),
    (
        "request.precondition_failed",
        "Phiên bản nhóm trường đã thay đổi. Vui lòng tải lại.",
        "The field group version changed. Please reload.",
    ),
    (
        "server.error",
        "Không thể tải field group sau cập nhật.",
        "Could not load the updated field group.",
    ),
    (
        "content.only_user_owned_field_groups_can_be_withdrawn",
        "Chỉ có thể rút nhóm trường thuộc quyền sở hữu của User.",
        "Only User-owned field groups can be withdrawn.",
    ),
    (
        "server.error",
        "Không thể tải field group sau khi rút.",
        "Could not load the withdrawn field group.",
    ),
    (
        "content.the_content_type_key_belongs_to_another_owner_409",
        "Khóa loại nội dung thuộc chủ sở hữu khác (409).",
        "The content type key belongs to another owner (409).",
    ),
    (
        "content.cannot_link_a_taxonomy_owned_by_another_provider_or_inactive",
        "Không thể liên kết taxonomy thuộc provider khác hoặc không hoạt động.",
        "Cannot link a taxonomy owned by another provider or inactive.",
    ),
    (
        "content.taxonomy_eligibility_has_not_been_defined",
        "Taxonomy eligibility chưa được định nghĩa.",
        "Taxonomy eligibility has not been defined.",
    ),
    (
        "content.cannot_take_ownership_of_core_post_page_taxonomy_eligibility",
        "Không thể chiếm quyền taxonomy eligibility của Core post/page.",
        "Cannot take ownership of Core post/page taxonomy eligibility.",
    ),
    (
        "content.cannot_declare_taxonomy_eligibility_for_a_content_type_owned_by_someone_else_409",
        "Không thể khai báo taxonomy eligibility cho loại nội dung thuộc chủ sở hữu khác (409).",
        "Cannot declare taxonomy eligibility for a content type owned by someone else (409).",
    ),
    (
        "content.the_content_type_in_taxonomy_eligibility_has_not_been_declared",
        "Loại nội dung trong taxonomy_eligibility chưa được khai báo.",
        "The content type in taxonomy_eligibility has not been declared.",
    ),
    (
        "content.taxonomy_eligibility_belongs_to_another_owner_409",
        "Taxonomy eligibility thuộc chủ sở hữu khác (409).",
        "Taxonomy eligibility belongs to another owner (409).",
    ),
    (
        "server.error",
        "Danh sách taxonomy override bị hỏng.",
        "The taxonomy override list is corrupted.",
    ),
    (
        "content.this_content_type_does_not_support_taxonomy",
        "Loại nội dung không hỗ trợ taxonomy.",
        "This content type does not support taxonomy.",
    ),
    (
        "content.content_type_does_not_exist",
        "Loại nội dung không tồn tại.",
        "Content type does not exist.",
    ),
    (
        "content.taxonomy_cannot_be_configured_for_section",
        "Không thể cấu hình taxonomy cho Section.",
        "Taxonomy cannot be configured for Section.",
    ),
    (
        "content.at_most_64_taxonomies_are_allowed_per_cpt",
        "Tối đa 64 taxonomy cho mỗi CPT.",
        "At most 64 taxonomies are allowed per CPT.",
    ),
    (
        "content.the_taxonomy_list_contains_invalid_or_duplicate_keys",
        "Danh sách taxonomy chứa khóa không hợp lệ hoặc trùng.",
        "The taxonomy list contains invalid or duplicate keys.",
    ),
    (
        "content.the_account_no_longer_has_administrator_privileges",
        "Tài khoản không còn quyền quản trị.",
        "The account no longer has administrator privileges.",
    ),
    (
        "request.precondition_failed",
        "Schema CPT đã thay đổi; hãy tải lại.",
        "The CPT schema has changed; please reload.",
    ),
    (
        "content.the_selected_taxonomy_does_not_exist",
        "Taxonomy được chọn không tồn tại.",
        "The selected taxonomy does not exist.",
    ),
    (
        "server.error",
        "Không thể tải schema CPT.",
        "Could not load the CPT schema.",
    ),
    (
        "request.invalid_if_match",
        "If-Match version không hợp lệ.",
        "Invalid If-Match version.",
    ),
    (
        "entries.patterns_can_only_be_edited_through_the_private_api_admin_patterns_api",
        "Pattern chỉ được sửa qua API riêng tư /api/admin/patterns.",
        "Patterns can only be edited through the private /api/admin/patterns API.",
    ),
    (
        "entries.cannot_edit_body_while_this_feature_is_disabled",
        "Không thể chỉnh sửa 'body' khi tính năng này bị tắt.",
        "Cannot edit 'body' while this feature is disabled.",
    ),
    (
        "entries.cannot_edit_featured_image_while_this_feature_is_disabled",
        "Không thể chỉnh sửa 'featured_image' khi tính năng này bị tắt.",
        "Cannot edit 'featured_image' while this feature is disabled.",
    ),
    (
        "entries.invalid_media_id_in_comment_block",
        "ID media trong comment block không hợp lệ.",
        "Invalid media ID in comment block.",
    ),
    (
        "entries.media_referenced_in_the_comment_block_does_not_exist_or_has_the_wrong_type",
        "Media tham chiếu trong comment block không tồn tại hoặc không đúng loại.",
        "Media referenced in the comment block does not exist or has the wrong type.",
    ),
    (
        "entries.invalid_or_self_referencing_pattern_reference",
        "Pattern tham chiếu không hợp lệ hoặc tự tham chiếu.",
        "Invalid or self-referencing Pattern reference.",
    ),
    (
        "entries.pattern_referenced_in_the_comment_block_does_not_exist",
        "Pattern tham chiếu trong comment block không tồn tại.",
        "Pattern referenced in the comment block does not exist.",
    ),
    (
        "server.error",
        "Dữ liệu custom fields không hợp lệ.",
        "Invalid custom fields data.",
    ),
    (
        "server.error",
        "Chế độ nội dung không hợp lệ.",
        "Invalid content mode.",
    ),
    (
        "entries.the_plugin_was_disabled_or_its_version_changed_during_execution",
        "Plugin đã bị vô hiệu hóa hoặc thay đổi phiên bản trong lúc thực thi.",
        "The plugin was disabled or its version changed during execution.",
    ),
    (
        "auth.forbidden",
        "Quyền của tài khoản đã bị thu hồi.",
        "The account's permissions have been revoked.",
    ),
    (
        "auth.forbidden",
        "Bạn không có quyền chỉnh sửa nội dung.",
        "You do not have permission to edit content.",
    ),
    (
        "entries.invalid_term_ids",
        "term_ids không hợp lệ.",
        "Invalid term_ids.",
    ),
    (
        "entries.term_ids_must_not_contain_duplicates",
        "term_ids không được trùng lặp.",
        "term_ids must not contain duplicates.",
    ),
    (
        "entries.the_term_id_does_not_exist_or_is_inactive",
        "Term ID không tồn tại hoặc inactive.",
        "The term ID does not exist or is inactive.",
    ),
    (
        "entries.the_term_belongs_to_a_different_locale_than_the_content",
        "Term thuộc ngôn ngữ khác với nội dung.",
        "The term belongs to a different locale than the content.",
    ),
    (
        "entries.the_term_s_taxonomy_is_not_allowed_for_the_cpt",
        "Taxonomy của term không được phép dùng cho CPT.",
        "The term's taxonomy is not allowed for the CPT.",
    ),
    (
        "entries.the_taxonomy_term_count_exceeds_the_limit",
        "Số term của taxonomy vượt giới hạn.",
        "The taxonomy term count exceeds the limit.",
    ),
    (
        "request.precondition_failed",
        "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
        "The content version has changed. Reload before saving.",
    ),
    (
        "entries.pattern_is_private_content",
        "Pattern là nội dung riêng tư.",
        "Pattern is private content.",
    ),
    (
        "entries.the_list_page_must_be_1_100_and_page_size_1_20",
        "Trang danh sách phải từ 1–100 và page_size từ 1–20.",
        "The list page must be 1–100 and page_size 1–20.",
    ),
    (
        "entries.include_descendants_requires_category_id_or_a_generic_taxonomy",
        "include_descendants yêu cầu category_id hoặc generic taxonomy.",
        "include_descendants requires category_id or a generic taxonomy.",
    ),
    (
        "entries.invalid_taxonomy_filter",
        "Bộ lọc taxonomy không hợp lệ.",
        "Invalid taxonomy filter.",
    ),
    (
        "entries.mixed_or_incomplete_taxonomy_key_term_id_pair",
        "Trộn hoặc thiếu cặp taxonomy_key + term_id.",
        "Mixed or incomplete taxonomy_key + term_id pair.",
    ),
    (
        "entries.the_filtered_term_does_not_exist_or_belongs_to_the_wrong_taxonomy",
        "Term trong bộ lọc không tồn tại hoặc sai taxonomy.",
        "The filtered term does not exist or belongs to the wrong taxonomy.",
    ),
    (
        "entries.this_taxonomy_is_not_allowed_for_this_cpt",
        "Taxonomy không được dùng cho CPT này.",
        "This taxonomy is not allowed for this CPT.",
    ),
    (
        "entries.a_non_hierarchical_taxonomy_does_not_accept_descendants",
        "Taxonomy không phân cấp không nhận descendants.",
        "A non-hierarchical taxonomy does not accept descendants.",
    ),
    (
        "entries.the_slug_already_exists_in_the_content_type",
        "Slug đã tồn tại trong loại nội dung.",
        "The slug already exists in the content type.",
    ),
    (
        "entries.no_active_v3_theme_is_available_for_selecting_a_layout_template",
        "Chưa có theme v3 hoạt động để chọn layout/template.",
        "No active v3 theme is available for selecting a layout/template.",
    ),
    (
        "server.error",
        "Manifest theme hoạt động không hợp lệ.",
        "Invalid active theme manifest.",
    ),
    (
        "entries.page_layout_is_not_available_in_the_active_theme",
        "Page Layout không có trong theme đang hoạt động.",
        "Page Layout is not available in the active theme.",
    ),
    (
        "entries.custom_single_template_is_not_for_this_content_type",
        "Custom Single Template không dành cho loại nội dung này.",
        "Custom Single Template is not for this content type.",
    ),
    (
        "entries.unexpected_version_on_create_do_not_supply_expected_version_when_creating_content",
        "unexpected_version_on_create: Không truyền expected_version khi tạo mới.",
        "unexpected_version_on_create: Do not supply expected_version when creating content.",
    ),
    (
        "entries.publication_status_only_supports_draft_or_published",
        "publication_status chỉ hỗ trợ 'draft' hoặc 'published'.",
        "publication_status only supports 'draft' or 'published'.",
    ),
    (
        "entries.immediate_save_content_types_do_not_support_draft_status",
        "Loại nội dung lưu ngay không hỗ trợ trạng thái bản nháp.",
        "Immediate-save content types do not support draft status.",
    ),
    (
        "entries.primary_category_term_id_only_applies_to_posts",
        "primary_category_term_id chỉ áp dụng cho bài viết (post).",
        "primary_category_term_id only applies to posts.",
    ),
    (
        "entries.primary_category_term_id_must_be_a_positive_integer_id",
        "primary_category_term_id phải là ID nguyên dương.",
        "primary_category_term_id must be a positive integer ID.",
    ),
    (
        "entries.page_layout_key_only_applies_to_page",
        "page_layout_key chỉ áp dụng cho Page.",
        "page_layout_key only applies to Page.",
    ),
    (
        "entries.custom_template_key_does_not_apply_to_page",
        "custom_template_key không áp dụng cho Page.",
        "custom_template_key does not apply to Page.",
    ),
    (
        "entries.builder_is_not_enabled_for_this_content_type",
        "Builder chưa được bật cho loại nội dung này.",
        "Builder is not enabled for this content type.",
    ),
    (
        "entries.content_mode_cannot_be_null",
        "content_mode không được null.",
        "content_mode cannot be null.",
    ),
    (
        "entries.invalid_layout_template_name",
        "Tên layout/template không hợp lệ.",
        "Invalid layout/template name.",
    ),
    (
        "entries.primary_category_term_id_must_be_a_term_assigned_to_the_post",
        "primary_category_term_id phải là một term đã được gán cho bài viết.",
        "primary_category_term_id must be a term assigned to the post.",
    ),
    (
        "entries.primary_category_term_id_must_belong_to_the_category_taxonomy",
        "primary_category_term_id phải thuộc taxonomy 'category'.",
        "primary_category_term_id must belong to the 'category' taxonomy.",
    ),
    (
        "entries.invalid_slug",
        "Slug không hợp lệ.",
        "Invalid slug.",
    ),
    (
        "permalink.conflict",
        "permalink_category_root_collision: slug 'posts' trùng trang lưu trữ mặc định.",
        "permalink_category_root_collision: slug 'posts' conflicts with the default archive page.",
    ),
    (
        "entries.could_not_automatically_generate_a_unique_slug_within_100_attempts",
        "Không thể tự động tạo slug duy nhất trong giới hạn 100 lần thử.",
        "Could not automatically generate a unique slug within 100 attempts.",
    ),
    (
        "entries.missing_content_version",
        "Thiếu phiên bản nội dung.",
        "Missing content version.",
    ),
    (
        "entries.direct_translation_creation_is_not_supported_for_pattern_section",
        "Không hỗ trợ tạo bản dịch trực tiếp cho Pattern/Section.",
        "Direct translation creation is not supported for Pattern/Section.",
    ),
    (
        "entries.source_post_for_translation_not_found",
        "Không tìm thấy bài gốc để dịch.",
        "Source post for translation not found.",
    ),
    (
        "request.precondition_failed",
        "Phiên bản nội dung gốc đã thay đổi. Hãy tải lại trước khi tạo bản dịch.",
        "The source content version has changed. Reload before creating a translation.",
    ),
    (
        "entries.invalid_target_language_code",
        "Mã ngôn ngữ đích không hợp lệ.",
        "Invalid target language code.",
    ),
    (
        "entries.the_target_language_must_differ_from_the_source_post_language",
        "Ngôn ngữ đích phải khác ngôn ngữ của bài gốc.",
        "The target language must differ from the source post language.",
    ),
    (
        "locale.not_found",
        "Ngôn ngữ đích không tồn tại trong site_locales.",
        "The target language does not exist in site_locales.",
    ),
    (
        "entries.the_translation_group_already_has_a_translation_in_the_target_language",
        "Nhóm dịch đã có bản dịch ở ngôn ngữ đích.",
        "The translation group already has a translation in the target language.",
    ),
    (
        "entries.the_translation_slug_must_be_kebab_case_at_most_64_characters",
        "Slug bản dịch phải là kebab-case, tối đa 64 ký tự.",
        "The translation slug must be kebab-case, at most 64 characters.",
    ),
    (
        "entries.the_slug_already_exists_in_the_target_language",
        "Slug đã tồn tại ở ngôn ngữ đích.",
        "The slug already exists in the target language.",
    ),
    (
        "server.error",
        "Không thể đóng gói lại Builder block sau ánh xạ term.",
        "Could not repackage the Builder block after mapping terms.",
    ),
    (
        "entries.invalid_publication_status",
        "Trạng thái xuất bản không hợp lệ.",
        "Invalid publication status.",
    ),
    (
        "entries.this_content_type_uses_immediate_saving",
        "Loại nội dung này sử dụng chế độ lưu ngay.",
        "This content type uses immediate saving.",
    ),
    (
        "server.error",
        "Không thể tải trạng thái xuất bản.",
        "Could not load publication status.",
    ),
    (
        "entry_table.section_uses_a_private_management_table",
        "Section dùng bảng quản lý riêng tư.",
        "Section uses a private management table.",
    ),
    (
        "import.select_at_least_one_data_group_pages_menus_media_settings",
        "Cần chọn ít nhất một nhóm dữ liệu (pages/menus/media/settings).",
        "Select at least one data group (pages/menus/media/settings).",
    ),
    (
        "import.no_active_v3_theme_revision",
        "Chưa có revision theme v3 đang hoạt động.",
        "No active v3 theme revision.",
    ),
    (
        "import.the_revision_does_not_belong_to_the_requested_theme_or_is_not_v3",
        "Revision không thuộc theme yêu cầu hoặc không phải v3.",
        "The revision does not belong to the requested theme or is not v3.",
    ),
    (
        "server.error",
        "Manifest theme v3 không hợp lệ.",
        "Invalid v3 theme manifest.",
    ),
    (
        "import.the_theme_does_not_declare_sample_data",
        "Theme không khai báo dữ liệu mẫu (sample).",
        "The theme does not declare sample data.",
    ),
    (
        "import.unsupported_media_format",
        "Định dạng media không được hỗ trợ.",
        "Unsupported media format.",
    ),
    (
        "server.error",
        "Không tạo được thư mục staging.",
        "Could not create the staging directory.",
    ),
    (
        "server.error",
        "Không tạo được thư mục media.",
        "Could not create the media directory.",
    ),
    (
        "server.error",
        "Không mở được file staging.",
        "Could not open the staging file.",
    ),
    (
        "server.error",
        "Không ghi được file staging.",
        "Could not write the staging file.",
    ),
    (
        "server.error",
        "Không đồng bộ được file staging.",
        "Could not sync the staging file to disk.",
    ),
    (
        "server.error",
        "Không thể di chuyển file media từ staging.",
        "Could not move the media file from staging.",
    ),
    (
        "server.error",
        "Không thể di chuyển thumbnail từ staging.",
        "Could not move the thumbnail from staging.",
    ),
    (
        "import.the_import_run_id_already_exists_in_an_incomplete_state_or_with_a_different_plan",
        "Import run_id đã tồn tại ở trạng thái chưa hoàn tất hoặc khác kế hoạch.",
        "The import run_id already exists in an incomplete state or with a different plan.",
    ),
    (
        "request.precondition_failed",
        "Trạng thái site đã thay đổi sau dry-run (412). Vui lòng chạy lại dry-run.",
        "Site state changed after dry-run (412). Run dry-run again.",
    ),
    (
        "import.the_v3_theme_revision_does_not_exist",
        "Revision theme v3 không tồn tại.",
        "The v3 theme revision does not exist.",
    ),
    (
        "import.the_theme_has_no_sample_data",
        "Theme không có dữ liệu mẫu.",
        "The theme has no sample data.",
    ),
    (
        "server.error",
        "Sự cố mô phỏng sau Phase 1 (staged).",
        "Simulated crash after Phase 1 (staged).",
    ),
    (
        "server.error",
        "Không tìm thấy media item trong sample.",
        "Media item not found in the sample.",
    ),
    (
        "server.error",
        "Không đọc được file media từ revision theme.",
        "Could not read the media file from the theme revision.",
    ),
    (
        "server.error",
        "Sự cố mô phỏng sau Phase 2 (files_verified).",
        "Simulated crash after Phase 2 (files_verified).",
    ),
    (
        "request.precondition_failed",
        "Trạng thái site đã thay đổi trước khi commit (412).",
        "Site state changed before commit (412).",
    ),
    (
        "server.error",
        "Không tìm thấy sample page.",
        "Sample page not found.",
    ),
    (
        "server.error",
        "Không tìm thấy sample menu.",
        "Sample menu not found.",
    ),
    (
        "server.error",
        "Không tìm thấy sample setting.",
        "Sample setting not found.",
    ),
    (
        "server.error",
        "Thiếu media_id cho sample setting.",
        "Missing media_id for the sample setting.",
    ),
    (
        "import.the_sample_option_key_does_not_exist",
        "Sample option key không tồn tại.",
        "The sample option key does not exist.",
    ),
    (
        "media.media_deletion_is_in_progress_it_cannot_be_uploaded_again_until_completion",
        "Media đang trong quá trình xóa; không thể upload lại cho đến khi hoàn tất.",
        "Media deletion is in progress; it cannot be uploaded again until completion.",
    ),
    (
        "server.error",
        "Không thể tải media vừa lưu.",
        "Could not load the newly saved media.",
    ),
    (
        "media.media_is_pending_deletion_it_cannot_be_uploaded_again_until_finalized",
        "Media đang chờ hoàn tất xóa; không thể upload lại cho đến khi finalized.",
        "Media is pending deletion; it cannot be uploaded again until finalized.",
    ),
    (
        "media.the_referenced_image_is_not_in_the_media_library_select_an_uploaded_image",
        "Ảnh tham chiếu chưa có trong Media Library. Hãy chọn ảnh đã upload.",
        "The referenced image is not in the Media Library. Select an uploaded image.",
    ),
    (
        "media.the_referenced_image_is_being_deleted_new_references_cannot_be_created",
        "Ảnh tham chiếu đang trong quá trình xóa; không thể tạo tham chiếu mới.",
        "The referenced image is being deleted; new references cannot be created.",
    ),
    (
        "server.error",
        "Trạng thái media delete job không hợp lệ.",
        "Invalid media deletion job status.",
    ),
    (
        "media.media_deletion_is_in_progress",
        "Media đang trong quá trình xóa.",
        "Media deletion is in progress.",
    ),
    (
        "request.precondition_failed",
        "Phiên bản hoặc generation của media đã thay đổi.",
        "The media version or generation has changed.",
    ),
    (
        "request.precondition_failed",
        "Phiên bản media đã thay đổi đồng thời.",
        "The media version changed concurrently.",
    ),
    (
        "server.error",
        "Không đọc lại được media delete job.",
        "Could not read the media deletion job.",
    ),
    (
        "media.the_media_record_count_exceeds_the_reconciliation_limit_10_000_records",
        "Số bản ghi media vượt giới hạn reconcile (tối đa 10.000 bản ghi).",
        "The media record count exceeds the reconciliation limit (10,000 records).",
    ),
    (
        "media.the_total_record_count_exceeds_the_reconciliation_limit_10_000_records",
        "Tổng số bản ghi cần đối soát vượt giới hạn reconcile (tối đa 10.000 bản ghi).",
        "The total record count exceeds the reconciliation limit (10,000 records).",
    ),
    (
        "menu.invalid_menu_target_kind",
        "target_kind menu không hợp lệ.",
        "Invalid menu target_kind.",
    ),
    (
        "menu.invalid_menu_key",
        "menu_key không hợp lệ.",
        "Invalid menu_key.",
    ),
    (
        "menu.the_menu_url_is_invalid_or_too_long",
        "URL menu không hợp lệ hoặc quá dài.",
        "The menu URL is invalid or too long.",
    ),
    (
        "menu.menu_urls_cannot_be_protocol_relative_or_contain_control_characters",
        "URL menu không được dùng protocol-relative hoặc ký tự điều khiển.",
        "Menu URLs cannot be protocol-relative or contain control characters.",
    ),
    (
        "menu.the_menu_url_must_be_valid_http_s",
        "URL menu phải là HTTP(S) hợp lệ.",
        "The menu URL must be valid HTTP(S).",
    ),
    (
        "menu.menu_urls_only_accept_http_or_https",
        "URL menu chỉ chấp nhận http hoặc https.",
        "Menu URLs only accept http or https.",
    ),
    (
        "menu.menu_urls_must_not_contain_credentials",
        "URL menu không được chứa credentials.",
        "Menu URLs must not contain credentials.",
    ),
    (
        "menu.the_menu_url_is_missing_a_host",
        "URL menu thiếu host.",
        "The menu URL is missing a host.",
    ),
    (
        "menu.invalid_menu_item_label",
        "Nhãn menu item không hợp lệ.",
        "Invalid menu item label.",
    ),
    (
        "menu.invalid_origin_provenance",
        "origin_provenance không hợp lệ.",
        "Invalid origin_provenance.",
    ),
    (
        "menu.menu_origin_provenance_already_exists",
        "origin_provenance menu đã tồn tại.",
        "Menu origin_provenance already exists.",
    ),
    (
        "menu.menu_key_already_exists",
        "menu_key đã tồn tại.",
        "menu_key already exists.",
    ),
    (
        "server.error",
        "Không đọc lại được menu vừa tạo.",
        "Could not read the newly created menu.",
    ),
    (
        "menu.the_menu_exceeds_200_items",
        "Menu vượt quá giới hạn 200 items.",
        "The menu exceeds 200 items.",
    ),
    (
        "menu.invalid_menu_item_position",
        "Vị trí menu item không hợp lệ.",
        "Invalid menu item position.",
    ),
    (
        "menu.entry_menu_item_is_missing_target_entry_id",
        "Menu item entry thiếu target_entry_id.",
        "Entry menu item is missing target_entry_id.",
    ),
    (
        "menu.term_menu_item_is_missing_target_term_id",
        "Menu item term thiếu target_term_id.",
        "Term menu item is missing target_term_id.",
    ),
    (
        "menu.external_menu_item_is_missing_external_url",
        "Menu item external thiếu external_url.",
        "External menu item is missing external_url.",
    ),
    (
        "menu.the_menu_item_id_does_not_belong_to_the_current_menu",
        "Menu item id không thuộc menu hiện tại.",
        "The menu item ID does not belong to the current menu.",
    ),
    (
        "menu.duplicate_menu_item_key_in_the_batch",
        "Trùng key menu item trong batch.",
        "Duplicate menu item key in the batch.",
    ),
    (
        "menu.parent_id_does_not_belong_to_the_same_menu_or_does_not_exist",
        "parent_id không thuộc cùng menu hoặc không tồn tại.",
        "parent_id does not belong to the same menu or does not exist.",
    ),
    (
        "menu.parent_client_key_does_not_exist_in_the_menu",
        "parent_client_key không tồn tại trong menu.",
        "parent_client_key does not exist in the menu.",
    ),
    (
        "menu.duplicate_position_at_the_same_menu_level",
        "Trùng position trong cùng cấp menu.",
        "Duplicate position at the same menu level.",
    ),
    (
        "menu.a_cycle_was_detected_in_the_menu",
        "Phát hiện vòng lặp (cycle) trong menu.",
        "A cycle was detected in the menu.",
    ),
    (
        "menu.menu_depth_exceeds_8_levels",
        "Độ sâu menu vượt quá giới hạn 8 cấp.",
        "Menu depth exceeds 8 levels.",
    ),
    (
        "menu.could_not_arrange_the_menu_tree",
        "Không thể sắp xếp cây menu.",
        "Could not arrange the menu tree.",
    ),
    (
        "request.precondition_failed",
        "Menu đã được cập nhật ở phiên bản khác.",
        "The menu was updated in another version.",
    ),
    (
        "request.precondition_failed",
        "Menu đã thay đổi đồng thời.",
        "The menu changed concurrently.",
    ),
    (
        "server.error",
        "Không đọc lại được menu sau khi cập nhật.",
        "Could not read the updated menu.",
    ),
    (
        "menu.menu_not_found_for_slot_assignment",
        "Không tìm thấy menu để gắn slot.",
        "Menu not found for slot assignment.",
    ),
    (
        "menu.limit_must_be_in_1_100",
        "limit phải trong khoảng 1..=100.",
        "limit must be in 1..=100.",
    ),
    (
        "menu.invalid_offset",
        "offset không hợp lệ.",
        "Invalid offset.",
    ),
    (
        "menu.no_theme_is_currently_active",
        "Chưa có theme nào đang kích hoạt.",
        "No theme is currently active.",
    ),
    (
        "menu.the_theme_is_not_the_active_revision",
        "Theme không phải revision đang kích hoạt.",
        "The theme is not the active revision.",
    ),
    (
        "menu.the_menu_slot_does_not_exist_in_the_active_revision",
        "Slot menu không tồn tại trong revision đang kích hoạt.",
        "The menu slot does not exist in the active revision.",
    ),
    (
        "request.precondition_failed",
        "Slot menu đã thay đổi ở phiên bản khác.",
        "The menu slot changed in another version.",
    ),
    (
        "operations.invalid_operations_job_type",
        "Loại tác vụ vận hành không hợp lệ.",
        "Invalid operations job type.",
    ),
    (
        "operations.invalid_job_status",
        "Trạng thái tác vụ không hợp lệ.",
        "Invalid job status.",
    ),
    (
        "operations.jobid_must_be_a_64_character_hexadecimal_string",
        "JobId phải là chuỗi 64 ký tự hex.",
        "JobId must be a 64-character hexadecimal string.",
    ),
    (
        "operations.a_heavy_maintenance_job_is_already_active_on_this_site",
        "Đã có tác vụ bảo trì nặng đang hoạt động trên site này.",
        "A heavy maintenance job is already active on this site.",
    ),
    (
        "operations.the_job_id_already_exists",
        "Mã tác vụ đã tồn tại.",
        "The job ID already exists.",
    ),
    (
        "server.error",
        "Không đọc lại được tác vụ vừa tạo.",
        "Could not read the newly created job.",
    ),
    (
        "server.error",
        "Không đọc lại được tác vụ sau khi cập nhật.",
        "Could not read the updated job.",
    ),
    (
        "request.precondition_failed",
        "Phiên bản bản sao lưu đã thay đổi hoặc không thể xóa (CAS mismatch).",
        "The backup version changed or it cannot be deleted (CAS mismatch).",
    ),
    (
        "server.error",
        "Không đọc lại được tác vụ sau khi xóa artifact.",
        "Could not read the job after deleting the artifact.",
    ),
    (
        "overrides.invalid_scope_type_key",
        "scope_type_key không hợp lệ.",
        "Invalid scope_type_key.",
    ),
    (
        "overrides.invalid_scope_entry_id",
        "scope_entry_id không hợp lệ.",
        "Invalid scope_entry_id.",
    ),
    (
        "server.error",
        "Template override target trong DB không hợp lệ.",
        "Invalid template override target in the database.",
    ),
    (
        "server.error",
        "Template override target và canonical key không khớp.",
        "Template override target and canonical key do not match.",
    ),
    (
        "server.error",
        "Template override AST trong DB không hợp lệ.",
        "Invalid template override AST in the database.",
    ),
    (
        "overrides.content_type_for_scope_not_found",
        "Không tìm thấy loại nội dung cho scope.",
        "Content type for scope not found.",
    ),
    (
        "server.error",
        "Định nghĩa loại nội dung không hợp lệ.",
        "Invalid content type definition.",
    ),
    (
        "overrides.entry_for_scope_not_found",
        "Không tìm thấy entry cho scope.",
        "Entry for scope not found.",
    ),
    (
        "overrides.malformed_content_slot_or_template_part_block",
        "Content slot hoặc khối template part sai định dạng.",
        "Malformed content slot or template part block.",
    ),
    (
        "overrides.invalid_content_slot_count_for_template_target",
        "Số lượng content slot không hợp lệ cho đích template.",
        "Invalid content slot count for template target.",
    ),
    (
        "overrides.template_part_cycle_detected",
        "Phát hiện vòng lặp template part.",
        "Template part cycle detected.",
    ),
    (
        "overrides.template_part_depth_exceeds_3",
        "Độ sâu template part vượt quá 3.",
        "Template part depth exceeds 3.",
    ),
    (
        "overrides.template_parts_exceed_32_expansions",
        "Template part vượt quá 32 lần mở rộng.",
        "Template parts exceed 32 expansions.",
    ),
    (
        "overrides.taxonomy_term_for_template_target_not_found",
        "Không tìm thấy term taxonomy cho đích template.",
        "Taxonomy term for template target not found.",
    ),
    (
        "overrides.reusable_pattern_only_supports_builder_for_individual_entries_pages",
        "Reusable Pattern chỉ hỗ trợ Builder của từng entry/Page.",
        "Reusable Pattern only supports Builder for individual entries/Pages.",
    ),
    (
        "request.if_match_required",
        "Cần If-Match cho override đã tồn tại.",
        "If-Match required for existing override.",
    ),
    (
        "request.precondition_failed",
        "Template override đã thay đổi.",
        "Template override changed.",
    ),
    (
        "request.precondition_failed",
        "Template override không còn tồn tại.",
        "Template override no longer exists.",
    ),
    (
        "overrides.invalid_builder_utf_8_document",
        "Tài liệu Builder UTF-8 không hợp lệ.",
        "Invalid Builder UTF-8 document.",
    ),
    (
        "request.precondition_failed",
        "Template override đã thay đổi đồng thời.",
        "Template override changed concurrently.",
    ),
    (
        "server.error",
        "Không thể đọc template override sau khi lưu.",
        "Template override not readable after save.",
    ),
    (
        "request.if_match_required",
        "Cần If-Match để đặt lại.",
        "If-Match required for reset.",
    ),
    (
        "overrides.template_override_not_found",
        "Không tìm thấy template override.",
        "Template override not found.",
    ),
    (
        "request.precondition_failed",
        "Template override đã thay đổi trước khi đặt lại.",
        "Template override changed before reset.",
    ),
    (
        "overrides.reset_candidate_chain_must_begin_with_target",
        "Chuỗi ứng viên đặt lại phải bắt đầu bằng đích.",
        "Reset candidate chain must begin with target.",
    ),
    (
        "overrides.template_part_expansion_limit_exceeded",
        "Vượt quá giới hạn mở rộng template part.",
        "Template part expansion limit exceeded.",
    ),
    (
        "overrides.invalid_preview_entry",
        "Mục xem trước không hợp lệ.",
        "Invalid preview entry.",
    ),
    (
        "overrides.preview_entry_not_found",
        "Không tìm thấy mục xem trước.",
        "Preview entry not found.",
    ),
    (
        "overrides.preview_entry_does_not_match_template_target",
        "Mục xem trước không khớp đích template.",
        "Preview entry does not match template target.",
    ),
    (
        "overrides.preview_entry_slug_does_not_match_template_target",
        "Slug mục xem trước không khớp đích template.",
        "Preview entry slug does not match template target.",
    ),
    (
        "overrides.preview_entry_type_not_available",
        "Loại nội dung xem trước không khả dụng.",
        "Preview entry type not available.",
    ),
    (
        "overrides.invalid_preview_body",
        "Nội dung xem trước không hợp lệ.",
        "Invalid preview body.",
    ),
    (
        "overrides.preview_output_exceeds_512_kib",
        "Kết quả xem trước vượt quá 512 KiB.",
        "Preview output exceeds 512 KiB.",
    ),
    (
        "server.error",
        "Vòng lặp hoặc giới hạn độ sâu template part.",
        "Template part cycle or depth limit.",
    ),
    (
        "overrides.the_menu_position_is_not_declared_in_the_theme",
        "Vị trí menu không được khai báo trong theme.",
        "The menu position is not declared in the theme.",
    ),
    (
        "overrides.the_menu_does_not_exist",
        "Menu không tồn tại.",
        "The menu does not exist.",
    ),
    (
        "overrides.no_active_v3_theme_is_available_for_previewing_menu_slots",
        "Không có theme v3 đang kích hoạt để xem trước menu_slots.",
        "No active v3 theme is available for previewing menu_slots.",
    ),
    (
        "server.error",
        "owner_kind trong builder_block_definitions không hợp lệ.",
        "Invalid owner_kind in builder_block_definitions",
    ),
    (
        "server.error",
        "provider_status trong builder_block_definitions không hợp lệ.",
        "Invalid provider_status in builder_block_definitions",
    ),
    (
        "server.error",
        "declaration_state trong builder_block_definitions không hợp lệ.",
        "Invalid declaration_state in builder_block_definitions",
    ),
    (
        "page.invalid_owner_kind",
        "owner_kind không hợp lệ.",
        "Invalid owner_kind.",
    ),
    (
        "page.the_block_id_belongs_to_another_package",
        "ID block đã thuộc quyền sở hữu của gói khác.",
        "The block ID belongs to another package.",
    ),
    (
        "patterns.invalid_pattern_title_1_200_characters",
        "Tiêu đề Pattern không hợp lệ (1–200 ký tự).",
        "Invalid Pattern title (1–200 characters).",
    ),
    (
        "patterns.invalid_pattern_slug",
        "Slug Pattern không hợp lệ.",
        "Invalid Pattern slug.",
    ),
    (
        "patterns.pattern_id_must_be_positive",
        "Pattern ID phải dương.",
        "Pattern ID must be positive.",
    ),
    (
        "patterns.pattern_does_not_exist",
        "Pattern không tồn tại.",
        "Pattern does not exist.",
    ),
    (
        "patterns.the_pattern_slug_already_exists",
        "Slug Pattern đã tồn tại.",
        "The Pattern slug already exists.",
    ),
    (
        "request.precondition_failed",
        "Pattern đã thay đổi; nháp chưa lưu.",
        "The Pattern has changed; the draft is unsaved.",
    ),
    (
        "request.precondition_failed",
        "Pattern đã thay đổi.",
        "The Pattern has changed.",
    ),
    (
        "request.precondition_failed",
        "Pattern đã thay đổi; tải lại trước khi xóa.",
        "The Pattern has changed; reload before deleting.",
    ),
    (
        "patterns.invalid_pattern_key",
        "Khóa Pattern không hợp lệ.",
        "Invalid Pattern key.",
    ),
    (
        "patterns.no_active_v3_theme",
        "Không có active theme v3.",
        "No active v3 theme.",
    ),
    (
        "patterns.theme_pattern_does_not_exist",
        "Theme Pattern không tồn tại.",
        "Theme Pattern does not exist.",
    ),
    (
        "patterns.theme_pattern_is_unavailable",
        "Theme Pattern không khả dụng.",
        "Theme Pattern is unavailable.",
    ),
    (
        "permalink.invalid_provider_rewrite_owner",
        "Provider rewrite owner không hợp lệ.",
        "Invalid provider rewrite owner.",
    ),
    (
        "permalink.invalid_provider_rewrite_base",
        "Provider rewrite_base không hợp lệ.",
        "Invalid provider rewrite_base.",
    ),
    (
        "permalink.the_provider_rewrite_declares_a_duplicate_key",
        "Provider rewrite khai báo key trùng.",
        "The provider rewrite declares a duplicate key.",
    ),
    (
        "permalink.the_provider_rewrite_target_has_not_been_declared",
        "Provider rewrite target chưa được khai báo.",
        "The provider rewrite target has not been declared.",
    ),
    (
        "permalink.the_provider_rewrite_key_belongs_to_another_owner",
        "Provider rewrite key thuộc owner khác.",
        "The provider rewrite key belongs to another owner.",
    ),
    (
        "permalink.the_provider_rewrite_key_is_already_owned_by_someone_else",
        "Provider rewrite key đã thuộc owner khác.",
        "The provider rewrite key is already owned by someone else.",
    ),
    (
        "permalink.the_post_permalink_must_begin_with_and_be_at_most_512_bytes",
        "Post permalink phải bắt đầu bằng /, tối đa 512 bytes.",
        "The post permalink must begin with / and be at most 512 bytes.",
    ),
    (
        "permalink.the_post_permalink_must_contain_1_to_8_segments",
        "Post permalink phải có từ 1 đến 8 segments.",
        "The post permalink must contain 1 to 8 segments.",
    ),
    (
        "permalink.the_post_permalink_must_end_with_postname_or_post_id",
        "Post permalink phải kết thúc bằng %postname% hoặc %post_id%.",
        "The post permalink must end with %postname% or %post_id%.",
    ),
    (
        "permalink.the_post_permalink_supports_at_most_one_category_tag",
        "Post permalink chỉ hỗ trợ tối đa một thẻ %category%.",
        "The post permalink supports at most one %category% tag.",
    ),
    (
        "permalink.the_post_permalink_supports_only_kebab_case_literals_at_most_one_category_tag_and_a_final_postname_or_post_id_tag_no_day_month_year",
        "Post permalink chỉ hỗ trợ literal kebab-case, tối đa một thẻ %category% và thẻ cuối %postname% hoặc %post_id% (không dùng ngày/tháng/năm).",
        "The post permalink supports only kebab-case literals, at most one %category% tag, and a final %postname% or %post_id% tag (no day/month/year).",
    ),
    (
        "permalink.the_post_permalink_uses_a_reserved_system_prefix",
        "Post permalink dùng tiền tố hệ thống được bảo vệ.",
        "The post permalink uses a reserved system prefix.",
    ),
    (
        "request.precondition_failed",
        "Permalink settings đã thay đổi; hãy tải lại.",
        "Permalink settings changed; please reload.",
    ),
    (
        "permalink.permalink_claim_limit_at_most_200000_canonical_claims",
        "permalink_claim_limit: tối đa 200000 canonical claims.",
        "permalink_claim_limit: at most 200000 canonical claims.",
    ),
    (
        "plugin.patterns_sections_cannot_be_deleted_through_plugin_actions",
        "Không thể xóa Pattern/Section qua action plugin.",
        "Patterns/Sections cannot be deleted through plugin actions.",
    ),
    (
        "plugin.invalid_expected_version",
        "expected_version không hợp lệ.",
        "Invalid expected_version.",
    ),
    (
        "request.precondition_failed",
        "Nội dung đã bị thay đổi trước đó (xung đột version).",
        "The content was changed previously (version conflict).",
    ),
    (
        "plugin.content_to_delete_not_found",
        "Không tìm thấy nội dung cần xóa.",
        "Content to delete not found.",
    ),
    (
        "public_content.content_archive_does_not_exist",
        "Archive nội dung không tồn tại.",
        "Content archive does not exist.",
    ),
    (
        "public_content.invalid_custom_taxonomy_page_filter",
        "Bộ lọc trang taxonomy tùy chỉnh không hợp lệ.",
        "Invalid custom taxonomy page filter.",
    ),
    (
        "public_content.generic_taxonomy_filter_pair_is_incomplete_or_aliases_are_mixed",
        "Generic taxonomy filter thiếu cặp hoặc trộn aliases.",
        "Generic taxonomy filter pair is incomplete or aliases are mixed.",
    ),
    (
        "public_content.invalid_generic_taxonomy_filter",
        "Generic taxonomy filter không hợp lệ.",
        "Invalid generic taxonomy filter.",
    ),
    (
        "public_content.the_taxonomy_is_private_or_non_hierarchical",
        "Taxonomy private hoặc không phân cấp.",
        "The taxonomy is private or non-hierarchical.",
    ),
    (
        "restore.the_backup_schema_version_is_unsupported_or_newer_than_the_current_version",
        "Phiên bản schema trong backup không được hỗ trợ hoặc mới hơn phiên bản hiện tại.",
        "The backup schema version is unsupported or newer than the current version.",
    ),
    (
        "restore.no_valid_migration_for_the_backup_schema",
        "Không có migration hợp lệ cho schema backup.",
        "No valid migration for the backup schema.",
    ),
    (
        "server.error",
        "Không khởi tạo được SQLite memory để dựng schema.",
        "Could not initialize in-memory SQLite to reconstruct the schema.",
    ),
    (
        "server.error",
        "Không chạy được compiled migrations trên memory DB.",
        "Could not run compiled migrations on the in-memory database.",
    ),
    (
        "restore.could_not_open_the_backup_sqlite_database_in_read_only_mode",
        "Không mở được SQLite backup ở chế độ chỉ đọc.",
        "Could not open the backup SQLite database in read-only mode.",
    ),
    (
        "restore.backup_sqlite_failed_integrity_check",
        "SQLite backup lỗi integrity_check.",
        "Backup SQLite failed integrity_check.",
    ),
    (
        "restore.backup_sqlite_violated_foreign_key_check",
        "SQLite backup vi phạm foreign_key_check.",
        "Backup SQLite violated foreign_key_check.",
    ),
    (
        "restore.sqlite_schema_metadata_exceeds_safety_limits_256_objects_256kib_2mib",
        "Metadata sqlite_schema vượt quá giới hạn an toàn (256 objects / 256KiB / 2MiB).",
        "sqlite_schema metadata exceeds safety limits (256 objects / 256KiB / 2MiB).",
    ),
    (
        "restore.the_backup_is_missing_a_valid_sqlx_migrations_table",
        "Backup thiếu bảng _sqlx_migrations hợp lệ.",
        "The backup is missing a valid _sqlx_migrations table.",
    ),
    (
        "restore.the_backup_has_an_empty_sqlx_migrations_table",
        "Backup có _sqlx_migrations rỗng.",
        "The backup has an empty _sqlx_migrations table.",
    ),
    (
        "restore.the_backup_contains_an_incomplete_migration",
        "Backup chứa migration chưa hoàn tất.",
        "The backup contains an incomplete migration.",
    ),
    (
        "restore.the_backup_has_a_newer_or_excess_schema_compared_with_the_current_version",
        "Backup có schema mới hơn hoặc dư thừa so với phiên bản hiện tại.",
        "The backup has a newer or excess schema compared with the current version.",
    ),
    (
        "restore.backup_migration_history_does_not_match_compiled_migrations",
        "Lịch sử migration trong backup không khớp compiled migrations.",
        "Backup migration history does not match compiled migrations.",
    ),
    (
        "restore.backup_sqlite_schema_does_not_match_the_valid_fingerprint_unexpected_or_modified_objects_detected",
        "Cấu trúc sqlite_schema của backup không khớp fingerprint hợp lệ (phát hiện đối tượng lạ hoặc bị sửa đổi).",
        "Backup sqlite_schema does not match the valid fingerprint (unexpected or modified objects detected).",
    ),
    (
        "restore.the_backup_has_no_valid_administrator_account",
        "Backup không có tài khoản Admin hợp lệ.",
        "The backup has no valid administrator account.",
    ),
    (
        "server.error",
        "Không mở được bản sao SQLite để migrate/sanitize.",
        "Could not open the SQLite copy to migrate/sanitize it.",
    ),
    (
        "server.error",
        "Không chạy được migrations trên bản sao khôi phục.",
        "Could not run migrations on the restored copy.",
    ),
    (
        "server.error",
        "Integrity check thất bại sau khi sanitize bản sao.",
        "Integrity check failed after sanitizing the copy.",
    ),
    (
        "server.error",
        "Foreign key check thất bại sau khi sanitize bản sao.",
        "Foreign key check failed after sanitizing the copy.",
    ),
    (
        "restore.the_restored_copy_is_missing_an_original_media_file_referenced_in_the_database",
        "Bản sao khôi phục thiếu tệp media gốc được tham chiếu trong DB.",
        "The restored copy is missing an original media file referenced in the database.",
    ),
    (
        "restore.the_restored_copy_is_missing_an_smtp_secret_file_referenced_in_the_database",
        "Bản sao khôi phục thiếu tệp SMTP secret được tham chiếu trong DB.",
        "The restored copy is missing an SMTP secret file referenced in the database.",
    ),
    (
        "restore.the_restored_copy_is_missing_a_theme_revision_s_complete_file",
        "Bản sao khôi phục thiếu .complete của theme revision.",
        "The restored copy is missing a theme revision's .complete file.",
    ),
    (
        "seo.the_path_conflicts_with_an_active_content_type_root",
        "Đường dẫn trùng với gốc loại nội dung đang hoạt động.",
        "The path conflicts with an active content type root.",
    ),
    (
        "seo.the_path_conflicts_with_an_active_taxonomy_root",
        "Đường dẫn trùng với gốc taxonomy đang hoạt động.",
        "The path conflicts with an active taxonomy root.",
    ),
    (
        "seo.the_path_conflicts_with_an_active_default_rewrite_prefix",
        "Đường dẫn trùng với tiền tố rewrite mặc định đang hoạt động.",
        "The path conflicts with an active default rewrite prefix.",
    ),
    (
        "seo.the_path_conflicts_with_an_active_language_prefix",
        "Đường dẫn trùng với tiền tố ngôn ngữ đang hoạt động.",
        "The path conflicts with an active language prefix.",
    ),
    (
        "seo.the_posts_path_is_reserved_for_the_default_post_archive",
        "Đường dẫn /posts đang dành cho trang lưu trữ bài viết mặc định.",
        "The /posts path is reserved for the default post archive.",
    ),
    (
        "seo.the_custom_path_is_already_used_by_another_post",
        "Đường dẫn tùy chỉnh đã được bài viết khác sử dụng.",
        "The custom path is already used by another post.",
    ),
    (
        "seo.the_path_is_reserved_as_a_redirect_for_another_post",
        "Đường dẫn đang được giữ làm redirect cho một bài viết khác.",
        "The path is reserved as a redirect for another post.",
    ),
    (
        "seo.the_content_type_key_conflicts_with_an_active_language_prefix",
        "Khóa loại nội dung trùng với tiền tố ngôn ngữ đang hoạt động.",
        "The content type key conflicts with an active language prefix.",
    ),
    (
        "seo.the_content_type_key_conflicts_with_an_existing_custom_path",
        "Khóa loại nội dung trùng với đường dẫn tùy chỉnh đang tồn tại.",
        "The content type key conflicts with an existing custom path.",
    ),
    (
        "seo.the_content_type_key_conflicts_with_an_existing_public_path",
        "Khóa loại nội dung trùng với đường dẫn công khai đang tồn tại.",
        "The content type key conflicts with an existing public path.",
    ),
    (
        "seo.the_content_type_key_conflicts_with_an_existing_redirect",
        "Khóa loại nội dung trùng với redirect đang tồn tại.",
        "The content type key conflicts with an existing redirect.",
    ),
    (
        "request.precondition_failed",
        "Phiên bản nội dung đã thay đổi đồng thời.",
        "The content version changed concurrently.",
    ),
    (
        "seo.the_og_image_does_not_exist_in_the_media_library",
        "Ảnh OG không tồn tại trong thư viện media.",
        "The OG image does not exist in the Media Library.",
    ),
    (
        "request.precondition_failed",
        "Phiên bản SEO trang chủ đã thay đổi. Hãy tải lại trước khi lưu.",
        "The homepage SEO version changed. Reload before saving.",
    ),
    (
        "server.error",
        "Thiếu ngôn ngữ mặc định trong site_locales.",
        "The default language is missing from site_locales.",
    ),
    (
        "server.error",
        "Không tìm thấy cấu hình ngôn ngữ trong site_locales.",
        "Language configuration not found in site_locales.",
    ),
    (
        "locale.the_locale_parameter_must_not_be_empty",
        "Tham số locale không được rỗng.",
        "The locale parameter must not be empty.",
    ),
    (
        "locale.not_found",
        "Ngôn ngữ được yêu cầu không tồn tại.",
        "The requested language does not exist.",
    ),
    (
        "request.precondition_failed",
        "Danh sách ngôn ngữ đã thay đổi. Tải lại trước khi lưu.",
        "The language list changed. Reload before saving.",
    ),
    (
        "locale.the_system_has_reached_the_maximum_of_20_languages",
        "Đã đạt giới hạn tối đa 20 ngôn ngữ trên hệ thống.",
        "The system has reached the maximum of 20 languages.",
    ),
    (
        "locale.the_language_code_already_exists",
        "Mã ngôn ngữ đã tồn tại.",
        "The language code already exists.",
    ),
    (
        "locale.not_found",
        "Không tìm thấy ngôn ngữ.",
        "Language not found.",
    ),
    (
        "locale.the_default_language_cannot_be_disabled",
        "Không thể tắt ngôn ngữ mặc định.",
        "The default language cannot be disabled.",
    ),
    (
        "locale.a_disabled_language_cannot_be_set_as_the_default",
        "Không thể đặt ngôn ngữ đang tắt làm ngôn ngữ mặc định.",
        "A disabled language cannot be set as the default.",
    ),
    (
        "request.precondition_failed",
        "Reading Settings đã thay đổi. Tải lại trước khi lưu.",
        "Reading Settings changed. Reload before saving.",
    ),
    (
        "settings.the_homepage_and_posts_page_must_be_two_different_pages",
        "Trang chủ và Trang bài viết phải là hai Page khác nhau.",
        "The homepage and posts page must be two different Pages.",
    ),
    (
        "settings.the_selected_page_must_exist_be_public_and_published",
        "Page được chọn phải tồn tại, public và đã xuất bản.",
        "The selected Page must exist, be public and published.",
    ),
    (
        "locale.the_selected_page_must_belong_to_the_language_being_configured",
        "Page được chọn phải thuộc đúng ngôn ngữ đang cấu hình.",
        "The selected Page must belong to the language being configured.",
    ),
    (
        "settings.site_logo_media_id_does_not_exist",
        "media_id của site_logo không tồn tại.",
        "site_logo media_id does not exist.",
    ),
    (
        "settings.the_option_key_does_not_exist_in_the_theme_schema",
        "Option key không tồn tại trong schema của theme.",
        "The option key does not exist in the theme schema.",
    ),
    (
        "locale.an_option_that_does_not_support_translation_cannot_be_saved_in_a_secondary_language",
        "Không thể lưu option không hỗ trợ dịch ở ngôn ngữ phụ.",
        "An option that does not support translation cannot be saved in a secondary language.",
    ),
    (
        "settings.image_ref_media_id_does_not_exist",
        "media_id của image_ref không tồn tại.",
        "image_ref media_id does not exist.",
    ),
    (
        "server.error",
        "Không đọc lại được theme options sau khi lưu.",
        "Could not read theme options after saving.",
    ),
    (
        "settings.stored_theme_option_data_is_incompatible_with_the_new_revision",
        "Dữ liệu theme option đã lưu không tương thích với revision mới.",
        "Stored theme option data is incompatible with the new revision.",
    ),
    (
        "settings.the_new_revision_s_option_schema_is_incompatible_with_the_saved_values_409",
        "Schema option của revision mới không tương thích với giá trị đã lưu (409).",
        "The new revision's option schema is incompatible with the saved values (409).",
    ),
    (
        "settings.no_active_v3_theme_is_available_for_previewing_theme_options",
        "Không có theme v3 đang kích hoạt để xem trước theme_options.",
        "No active v3 theme is available for previewing theme_options.",
    ),
    (
        "settings.the_option_key_does_not_exist_in_the_schema",
        "Option key không tồn tại trong schema.",
        "The option key does not exist in the schema.",
    ),
    (
        "settings.the_setting_key_does_not_exist_in_the_plugin_schema",
        "Khóa cài đặt không tồn tại trong schema của plugin.",
        "The setting key does not exist in the plugin schema.",
    ),
    (
        "request.precondition_failed",
        "Cài đặt plugin đã thay đổi ở phiên khác. Tải lại trước khi lưu.",
        "Plugin settings changed in another session. Reload before saving.",
    ),
    (
        "settings.the_media_setting_must_be_a_valid_sha_256_media_id_or_empty",
        "Cài đặt media phải là media_id SHA-256 hợp lệ hoặc rỗng.",
        "The media setting must be a valid SHA-256 media_id or empty.",
    ),
    (
        "settings.the_plugin_setting_media_id_does_not_exist_or_is_not_an_active_image",
        "media_id của cài đặt plugin không tồn tại hoặc không phải ảnh active.",
        "The plugin setting media_id does not exist or is not an active image.",
    ),
    (
        "smtp.tls_mode_must_be_starttls_or_implicit_tls",
        "tls_mode phải là starttls hoặc implicit_tls.",
        "tls_mode must be starttls or implicit_tls.",
    ),
    (
        "smtp.smtp_configuration_changed_if_match_mismatch",
        "Cấu hình SMTP đã thay đổi (If-Match không khớp).",
        "SMTP configuration changed (If-Match mismatch).",
    ),
    (
        "server.error",
        "Không thể lưu snapshot SMTP.",
        "Could not save the SMTP snapshot.",
    ),
    (
        "server.error",
        "Snapshot SMTP không hợp lệ.",
        "Invalid SMTP snapshot.",
    ),
    (
        "smtp.the_attempt_status_has_changed",
        "Trạng thái attempt đã thay đổi.",
        "The attempt status has changed.",
    ),
    (
        "smtp.contact_message_not_found",
        "Không tìm thấy thư liên hệ.",
        "Contact message not found.",
    ),
    (
        "smtp.notification_attempt_not_found",
        "Không tìm thấy attempt thông báo.",
        "Notification attempt not found.",
    ),
    (
        "smtp.notification_attempt_version_does_not_match_if_match",
        "Phiên bản attempt thông báo không khớp (If-Match).",
        "Notification attempt version does not match (If-Match).",
    ),
    (
        "smtp.the_notification_is_being_sent",
        "Thông báo đang được gửi.",
        "The notification is being sent.",
    ),
    (
        "smtp.the_notification_was_sent_successfully",
        "Thông báo đã được gửi thành công.",
        "The notification was sent successfully.",
    ),
    (
        "smtp.the_previous_attempt_is_in_unknown_status_acknowledge_duplicate_risk_true_is_required_to_retry",
        "Attempt trước ở trạng thái unknown; cần acknowledge_duplicate_risk=true để gửi lại.",
        "The previous attempt is in unknown status; acknowledge_duplicate_risk=true is required to retry.",
    ),
    (
        "smtp.the_attempt_has_changed",
        "Attempt đã thay đổi.",
        "The attempt has changed.",
    ),
    (
        "smtp.the_maximum_of_3_notification_attempts_for_this_contact_message_has_been_reached",
        "Đã đạt giới hạn tối đa 3 lần thử thông báo cho thư liên hệ này.",
        "The maximum of 3 notification attempts for this contact message has been reached.",
    ),
    (
        "server.error",
        "Chủ sở hữu taxonomy không hợp lệ.",
        "Invalid taxonomy owner.",
    ),
    (
        "server.error",
        "Trạng thái provider taxonomy không hợp lệ.",
        "Invalid taxonomy provider status.",
    ),
    (
        "server.error",
        "Trạng thái khai báo taxonomy không hợp lệ.",
        "Invalid taxonomy declaration status.",
    ),
    (
        "taxonomy.the_taxonomy_key_conflicts_with_a_reserved_system_prefix",
        "Khóa taxonomy trùng với tiền tố hệ thống được bảo vệ.",
        "The taxonomy key conflicts with a reserved system prefix.",
    ),
    (
        "taxonomy.the_taxonomy_key_conflicts_with_an_existing_content_type_key",
        "Khóa taxonomy trùng với khóa loại nội dung đang tồn tại.",
        "The taxonomy key conflicts with an existing content type key.",
    ),
    (
        "taxonomy.invalid_provider_taxonomy_owner",
        "Provider taxonomy owner không hợp lệ.",
        "Invalid provider taxonomy owner.",
    ),
    (
        "taxonomy.the_provider_declares_duplicate_taxonomy_keys",
        "Provider khai báo taxonomy trùng key.",
        "The provider declares duplicate taxonomy keys.",
    ),
    (
        "taxonomy.the_taxonomy_key_belongs_to_another_owner",
        "Taxonomy key đã thuộc owner khác.",
        "The taxonomy key belongs to another owner.",
    ),
    (
        "taxonomy.the_taxonomy_still_contains_terms_with_parents",
        "Taxonomy còn term có parent.",
        "The taxonomy still contains terms with parents.",
    ),
    (
        "taxonomy.the_site_has_reached_the_limit_of_64_taxonomies",
        "Website đạt giới hạn 64 taxonomy.",
        "The site has reached the limit of 64 taxonomies.",
    ),
    (
        "taxonomy.the_taxonomy_key_is_already_registered",
        "Khóa taxonomy đã được đăng ký.",
        "The taxonomy key is already registered.",
    ),
    (
        "server.error",
        "Không thể đọc taxonomy vừa tạo.",
        "Could not read the newly created taxonomy.",
    ),
    (
        "request.precondition_failed",
        "Taxonomy đã thay đổi, hãy tải lại.",
        "The taxonomy changed; please reload.",
    ),
    (
        "taxonomy.the_taxonomy_is_provider_owned_and_cannot_be_edited_by_the_site",
        "Taxonomy thuộc provider, không thể sửa từ site.",
        "The taxonomy is provider-owned and cannot be edited by the site.",
    ),
    (
        "taxonomy.category_tag_hierarchy_settings_are_immutable",
        "Category/Tag có chế độ phân cấp bất biến.",
        "Category/Tag hierarchy settings are immutable.",
    ),
    (
        "server.error",
        "Không thể tải taxonomy vừa sửa.",
        "Could not load the updated taxonomy.",
    ),
    (
        "taxonomy.core_provider_taxonomies_cannot_be_deleted",
        "Không thể xóa taxonomy Core/provider.",
        "Core/provider taxonomies cannot be deleted.",
    ),
    (
        "taxonomy.the_taxonomy_is_used_by_a_term_or_content_type",
        "Taxonomy đang được dùng bởi term hoặc content type.",
        "The taxonomy is used by a term or content type.",
    ),
    (
        "taxonomy.the_term_name_must_be_between_1_and_120_characters",
        "Tên term phải dài từ 1 đến 120 ký tự.",
        "The term name must be between 1 and 120 characters.",
    ),
    (
        "taxonomy.the_slug_already_exists_in_the_taxonomy",
        "Slug đã tồn tại trong loại taxonomy.",
        "The slug already exists in the taxonomy.",
    ),
    (
        "taxonomy.taxonomy_data_violates_a_constraint",
        "Dữ liệu taxonomy vi phạm ràng buộc.",
        "Taxonomy data violates a constraint.",
    ),
    (
        "taxonomy.only_administrators_can_manage_taxonomies",
        "Chỉ quản trị viên được quản lý taxonomy.",
        "Only administrators can manage taxonomies.",
    ),
    (
        "request.precondition_failed",
        "Term đã thay đổi, hãy tải lại phiên bản mới.",
        "The term changed; reload the latest version.",
    ),
    (
        "taxonomy.the_category_branch_exceeds_8_levels",
        "Nhánh category vượt quá 8 cấp.",
        "The category branch exceeds 8 levels.",
    ),
    (
        "taxonomy.the_parent_category_id_must_be_a_positive_integer",
        "ID category cha phải là số nguyên dương.",
        "The parent category ID must be a positive integer.",
    ),
    (
        "taxonomy.the_parent_term_belongs_to_a_different_language_from_the_current_term",
        "Term cha thuộc ngôn ngữ khác với term hiện tại.",
        "The parent term belongs to a different language from the current term.",
    ),
    (
        "taxonomy.the_parent_term_does_not_exist_or_belongs_to_another_taxonomy",
        "Term cha không tồn tại hoặc thuộc taxonomy khác.",
        "The parent term does not exist or belongs to another taxonomy.",
    ),
    (
        "taxonomy.cannot_create_a_category_cycle",
        "Không thể tạo chu trình category.",
        "Cannot create a category cycle.",
    ),
    (
        "taxonomy.invalid_term_id_list",
        "Danh sách ID term không hợp lệ.",
        "Invalid term ID list.",
    ),
    (
        "taxonomy.the_slug_must_be_kebab_case_at_most_64_characters",
        "Slug phải là kebab-case, tối đa 64 ký tự.",
        "The slug must be kebab-case, at most 64 characters.",
    ),
    (
        "taxonomy.invalid_source_term_id",
        "source_term_id không hợp lệ.",
        "Invalid source_term_id.",
    ),
    (
        "taxonomy.the_taxonomy_is_no_longer_available",
        "Taxonomy không còn khả dụng.",
        "The taxonomy is no longer available.",
    ),
    (
        "taxonomy.non_hierarchical_taxonomies_do_not_accept_a_parent",
        "Taxonomy không phân cấp không nhận parent.",
        "Non-hierarchical taxonomies do not accept a parent.",
    ),
    (
        "taxonomy.source_term_for_translation_not_found",
        "Không tìm thấy term nguồn để dịch.",
        "Source term for translation not found.",
    ),
    (
        "taxonomy.the_source_term_does_not_belong_to_the_same_taxonomy",
        "Term nguồn không thuộc cùng loại taxonomy.",
        "The source term does not belong to the same taxonomy.",
    ),
    (
        "taxonomy.the_target_language_must_differ_from_the_source_term_language",
        "Ngôn ngữ đích phải khác ngôn ngữ của term nguồn.",
        "The target language must differ from the source term language.",
    ),
    (
        "server.error",
        "Không thể tải term vừa tạo.",
        "Could not load the newly created term.",
    ),
    (
        "taxonomy.editors_cannot_modify_core_taxonomy_fields",
        "Editor không được sửa trường taxonomy lõi.",
        "Editors cannot modify core taxonomy fields.",
    ),
    (
        "taxonomy.schema_token_is_required_to_write_extension_fields",
        "Thiếu schema_token để ghi extension field.",
        "schema_token is required to write extension fields.",
    ),
    (
        "request.precondition_failed",
        "Term đã thay đổi, hãy tải lại.",
        "The term changed; please reload.",
    ),
    (
        "server.error",
        "Không thể tải term vừa cập nhật.",
        "Could not load the updated term.",
    ),
    (
        "taxonomy.the_category_still_has_children_move_the_children_first",
        "Category còn mục con. Hãy chuyển mục con trước.",
        "The category still has children. Move the children first.",
    ),
    (
        "taxonomy.the_term_is_assigned_to_content_remove_the_associations_first",
        "Term đang được gắn vào nội dung. Hãy gỡ liên kết trước.",
        "The term is assigned to content. Remove the associations first.",
    ),
    (
        "server.error",
        "Định dạng theme không hợp lệ trong DB.",
        "Invalid theme format in the database.",
    ),
    (
        "theme.invalid_theme_file_path",
        "Đường dẫn file theme không hợp lệ.",
        "Invalid theme file path.",
    ),
    (
        "theme.invalid_theme_file_name",
        "Tên file theme không hợp lệ.",
        "Invalid theme file name.",
    ),
    (
        "theme.theme_file_path_rejected",
        "Đường dẫn file theme bị từ chối.",
        "Theme file path rejected.",
    ),
    (
        "theme.the_theme_file_path_is_empty",
        "Đường dẫn file theme rỗng.",
        "The theme file path is empty.",
    ),
    (
        "server.error",
        "Không thể khởi tạo thư mục themes/.",
        "Could not initialize the themes/ directory.",
    ),
    (
        "server.error",
        "Không thể đọc thư mục themes/.staging.",
        "Could not read the themes/.staging directory.",
    ),
    (
        "server.error",
        "Không thể duyệt thư mục themes/.staging.",
        "Could not traverse themes/.staging.",
    ),
    (
        "server.error",
        "Không thể dọn staging cũ trong themes/.staging.",
        "Could not clean old staging directories in themes/.staging.",
    ),
    (
        "server.error",
        "Không thể dọn file staging cũ trong themes/.staging.",
        "Could not clean old staging files in themes/.staging.",
    ),
    (
        "server.error",
        "Không thể tạo thư mục staging cho theme.",
        "Could not create the theme staging directory.",
    ),
    (
        "server.error",
        "Không thể tạo thư mục con cho file theme.",
        "Could not create a subdirectory for the theme file.",
    ),
    (
        "server.error",
        "Không thể ghi file theme vào staging.",
        "Could not write the theme file to staging.",
    ),
    (
        "server.error",
        "Lỗi ghi nội dung file theme.",
        "Error writing theme file contents.",
    ),
    (
        "server.error",
        "Lỗi đồng bộ file theme xuống đĩa.",
        "Error syncing the theme file to disk.",
    ),
    (
        "theme.invalid_v3_theme_files_or_manifest",
        "Theme v3 files hoặc manifest không hợp lệ.",
        "Invalid v3 theme files or manifest.",
    ),
    (
        "theme.the_v3_theme_manifest_json_does_not_match_theme_json",
        "Theme v3 manifest_json không khớp file theme.json.",
        "The v3 theme manifest_json does not match theme.json.",
    ),
    (
        "theme.theme_metadata_does_not_match_the_v3_manifest",
        "Theme metadata không khớp manifest v3.",
        "Theme metadata does not match the v3 manifest.",
    ),
    (
        "theme.invalid_staging_run_uuid",
        "run_uuid staging không hợp lệ.",
        "Invalid staging run_uuid.",
    ),
    (
        "theme.the_theme_id_belongs_to_an_older_package_format",
        "Theme ID đang thuộc package format cũ.",
        "The theme ID belongs to an older package format.",
    ),
    (
        "theme.the_revision_digest_already_exists_for_a_different_theme_version",
        "Digest revision đã tồn tại cho một theme/version khác.",
        "The revision digest already exists for a different theme/version.",
    ),
    (
        "theme.the_same_theme_id_and_version_have_a_different_canonical_digest",
        "Cùng theme_id và version nhưng khác canonical digest.",
        "The same theme_id and version have a different canonical digest.",
    ),
    (
        "server.error",
        "Không thể tạo thư mục .staging.",
        "Could not create the .staging directory.",
    ),
    (
        "server.error",
        "Không thể tạo thư mục revisions.",
        "Could not create the revisions directory.",
    ),
    (
        "theme.the_revision_directory_on_disk_does_not_match_complete_or_is_corrupted",
        "Thư mục revision trên đĩa không khớp .complete hoặc bị hỏng.",
        "The revision directory on disk does not match .complete or is corrupted.",
    ),
    (
        "server.error",
        "Không thể publish thư mục revision vào themes/revisions/.",
        "Could not publish the revision directory to themes/revisions/.",
    ),
    (
        "server.error",
        "Không thể ghi marker .complete cho revision.",
        "Could not write the revision .complete marker.",
    ),
    (
        "server.error",
        "Không đọc lại được revision vừa lưu.",
        "Could not read the newly saved revision.",
    ),
    (
        "server.error",
        "Starter revision path là symlink; không thể tự phục hồi.",
        "The Starter revision path is a symlink; automatic recovery is unavailable.",
    ),
    (
        "server.error",
        "Starter revision path không phải thư mục.",
        "The Starter revision path is not a directory.",
    ),
    (
        "server.error",
        "Không thể phục hồi thư mục Starter revision.",
        "Could not restore the Starter revision directory.",
    ),
    (
        "server.error",
        "Frozen Starter legacy snapshot không hợp lệ.",
        "Invalid frozen Starter legacy snapshot.",
    ),
    (
        "server.error",
        "Built-in Starter Theme v3 không hợp lệ.",
        "Invalid built-in Starter Theme v3.",
    ),
    (
        "server.error",
        "Không thể kích hoạt Starter theme mặc định.",
        "Could not activate the default Starter theme.",
    ),
    (
        "server.error",
        "Không thể giữ ETag v1 mặc định.",
        "Could not preserve the default v1 ETag.",
    ),
    (
        "theme.invalid_v3_theme_revision_manifest",
        "Theme revision manifest v3 không hợp lệ.",
        "Invalid v3 theme revision manifest.",
    ),
    (
        "theme.theme_revision_metadata_does_not_match_the_manifest",
        "Theme revision metadata không khớp manifest.",
        "Theme revision metadata does not match the manifest.",
    ),
    (
        "server.error",
        "Theme manifest của revision bị hỏng.",
        "The revision theme manifest is corrupted.",
    ),
    (
        "server.error",
        "Admin extensions theme revision bị hỏng.",
        "The revision theme Admin extensions are corrupted.",
    ),
    (
        "server.error",
        "Không đọc được block trong theme revision.",
        "Could not read a block in the theme revision.",
    ),
    (
        "theme.invalid_template_selection_identifier",
        "Định danh template selection không hợp lệ.",
        "Invalid template selection identifier.",
    ),
    (
        "theme.invalid_template_selection_name",
        "Tên template selection không hợp lệ.",
        "Invalid template selection name.",
    ),
    (
        "theme.invalid_template_selection_ast",
        "AST template selection không hợp lệ.",
        "Invalid template selection AST.",
    ),
    (
        "server.error",
        "Không đọc lại được template selection vừa lưu.",
        "Could not read the newly saved template selection.",
    ),
    (
        "theme.preview_theme_part_unavailable",
        "Theme part xem trước không khả dụng.",
        "Preview theme part unavailable.",
    ),
    (
        "theme.preview_theme_part_failed_integrity_check",
        "Theme part xem trước không vượt qua kiểm tra toàn vẹn.",
        "Preview theme part failed integrity check.",
    ),
    (
        "theme.invalid_theme_part_utf_8",
        "UTF-8 của theme part không hợp lệ.",
        "Invalid theme part UTF-8.",
    ),
    (
        "auth.the_username_must_contain_3_to_32_characters",
        "Tên đăng nhập phải từ 3 đến 32 ký tự.",
        "The username must contain 3 to 32 characters.",
    ),
    (
        "auth.the_username_must_start_with_a_letter_or_digit",
        "Tên đăng nhập phải bắt đầu bằng chữ cái hoặc chữ số.",
        "The username must start with a letter or digit.",
    ),
    (
        "auth.the_username_may_contain_only_letters_digits_underscores_or_hyphens",
        "Tên đăng nhập chỉ được chứa chữ cái, số, gạch dưới hoặc gạch ngang.",
        "The username may contain only letters, digits, underscores or hyphens.",
    ),
    (
        "auth.the_password_must_contain_8_to_128_characters",
        "Mật khẩu phải từ 8 đến 128 ký tự.",
        "The password must contain 8 to 128 characters.",
    ),
    (
        "backup.createjobobjectw_failed",
        "CreateJobObjectW thất bại.",
        "CreateJobObjectW failed",
    ),
    (
        "backup.setinformationjobobject_failed",
        "SetInformationJobObject thất bại.",
        "SetInformationJobObject failed",
    ),
    (
        "backup.assignprocesstojobobject_failed",
        "AssignProcessToJobObject thất bại.",
        "AssignProcessToJobObject failed",
    ),
    (
        "backup.native_memory_limiting_is_not_supported_on_this_platform",
        "Không hỗ trợ giới hạn bộ nhớ native cho nền tảng này.",
        "Native memory limiting is not supported on this platform.",
    ),
    (
        "backup.invalid_worker_request_size",
        "invalid_worker_request_size: Kích thước yêu cầu worker không hợp lệ.",
        "invalid_worker_request_size",
    ),
    (
        "server.error",
        "Không truy cập được dữ liệu.",
        "Could not access the data.",
    ),
    (
        "server.error",
        "Dữ liệu block không hợp lệ.",
        "Invalid block data.",
    ),
    (
        "hook.core_hook_panicked",
        "Core hook bị panic.",
        "Core hook panicked",
    ),
    (
        "hook.only_page_after_save_is_implemented",
        "Hiện chỉ triển khai page/after_save.",
        "Only page/after_save is implemented",
    ),
    (
        "hook.core_handler_id_must_be_core_snake_key",
        "ID Core handler phải có dạng core/snake_key.",
        "Core handler ID must be core/snake_key",
    ),
    (
        "hook.duplicate_handler_id",
        "ID handler trùng lặp.",
        "Duplicate handler ID",
    ),
    (
        "hook.too_many_handlers",
        "Quá nhiều handler.",
        "Too many handlers",
    ),
    (
        "hook.hook_dispatch_admission_budget_exceeded",
        "Vượt ngân sách cấp phép điều phối hook.",
        "Hook dispatch admission budget exceeded",
    ),
    (
        "hook.core_hook_timed_out",
        "Core hook vượt quá thời gian cho phép.",
        "Core hook timed out",
    ),
    (
        "hook.contentaftersave_phase_budget_exceeded",
        "Vượt ngân sách giai đoạn ContentAfterSave.",
        "ContentAfterSave phase budget exceeded",
    ),
    (
        "operations_worker_process.missing_macos_jetsam_spawn_proof",
        "Thiếu bằng chứng khởi chạy macOS Jetsam.",
        "missing macOS Jetsam spawn proof",
    ),
    (
        "operations_worker_process.macos_jetsam_spawn_proof_timed_out",
        "Bằng chứng khởi chạy macOS Jetsam quá hạn.",
        "macOS Jetsam spawn proof timed out",
    ),
    (
        "operations_worker_process.invalid_macos_jetsam_spawn_proof",
        "Bằng chứng khởi chạy macOS Jetsam không hợp lệ.",
        "invalid macOS Jetsam spawn proof",
    ),
    (
        "package.the_zip_contains_duplicate_paths",
        "ZIP chứa đường dẫn trùng lặp.",
        "The ZIP contains duplicate paths.",
    ),
    (
        "package.invalid_theme_zip_path",
        "Đường dẫn trong gói theme ZIP không hợp lệ.",
        "Invalid theme ZIP path.",
    ),
    (
        "package.the_theme_zip_path_contains_an_unsafe_segment",
        "Đường dẫn trong gói theme ZIP chứa segment không an toàn.",
        "The theme ZIP path contains an unsafe segment.",
    ),
    (
        "package.theme_zip_exceeds_8_mib",
        "Gói theme ZIP vượt quá giới hạn 8 MiB.",
        "The theme ZIP package exceeds the 8 MiB limit.",
    ),
    (
        "package.invalid_theme_zip",
        "Gói theme ZIP không hợp lệ.",
        "Invalid theme ZIP package.",
    ),
    (
        "package.could_not_read_zip_entry",
        "Không đọc được mục trong ZIP.",
        "Could not read entry in ZIP archive.",
    ),
    (
        "package.zip_must_not_contain_symlinks",
        "ZIP không được chứa symlink.",
        "ZIP archive must not contain symlinks.",
    ),
    (
        "package.invalid_zip_path",
        "Đường dẫn ZIP không hợp lệ.",
        "Invalid ZIP path.",
    ),
    (
        "package.zip_directory_entry_must_not_contain_data",
        "Mục thư mục trong ZIP không được chứa dữ liệu.",
        "ZIP directory entry must not contain data.",
    ),
    (
        "package.theme_zip_exceeds_256_files",
        "Gói theme ZIP vượt quá giới hạn 256 file.",
        "The theme ZIP package exceeds the 256-file limit.",
    ),
    (
        "package.theme_zip_duplicate_or_case_conflict",
        "Gói theme ZIP chứa đường dẫn trùng lặp hoặc xung đột chữ hoa/thường.",
        "The theme ZIP archive contains duplicate paths or case conflicts.",
    ),
    (
        "package.theme_zip_file_exceeds_size_limit",
        "File trong gói theme ZIP vượt quá giới hạn kích thước cho phép.",
        "A file in the theme ZIP archive exceeds the allowed size limit.",
    ),
    (
        "package.theme_zip_extracted_file_exceeds_size_limit",
        "File giải nén trong gói theme ZIP vượt quá giới hạn kích thước.",
        "An extracted file in the theme ZIP archive exceeds the allowed size limit.",
    ),
    (
        "package.theme_zip_total_extracted_exceeds_32_mib",
        "Tổng dung lượng giải nén gói theme ZIP vượt quá 32 MiB.",
        "Total extracted theme ZIP size exceeds the 32 MiB limit.",
    ),
    (
        "package.could_not_read_theme_zip_file",
        "Không thể đọc nội dung file trong gói theme ZIP.",
        "Could not read file contents in the theme ZIP archive.",
    ),
    (
        "package.theme_zip_rejected_zip_bomb",
        "Gói theme ZIP bị từ chối do tỷ lệ giải nén bất thường (zip bomb).",
        "The theme ZIP archive was rejected due to an abnormal expansion ratio (zip bomb).",
    ),
    (
        "package.the_backup_zip_file_size_is_outside_the_valid_range",
        "Kích thước tệp backup ZIP nằm ngoài giới hạn hợp lệ.",
        "The backup ZIP file size is outside the valid range.",
    ),
    (
        "package.the_backup_zip_lacks_a_valid_zip32_eocd_or_contains_a_trailing_comment",
        "Backup ZIP thiếu ZIP32 EOCD hợp lệ hoặc chứa comment cuối tệp.",
        "The backup ZIP lacks a valid ZIP32 EOCD or contains a trailing comment.",
    ),
    (
        "package.the_backup_zip32_eocd_header_is_invalid_or_exceeds_the_limit",
        "Header ZIP32 EOCD của backup không hợp lệ hoặc vượt ngưỡng.",
        "The backup ZIP32 EOCD header is invalid or exceeds the limit.",
    ),
    (
        "package.the_backup_zip_central_directory_offset_size_does_not_match",
        "Offset/kích thước Central Directory của backup ZIP không khớp.",
        "The backup ZIP Central Directory offset/size does not match.",
    ),
    (
        "package.backup_zip64_is_not_supported_in_format_v1",
        "Backup ZIP64 không được hỗ trợ trong format v1.",
        "Backup ZIP64 is not supported in format v1.",
    ),
    (
        "package.the_backup_zip_central_directory_is_truncated",
        "Central Directory backup ZIP bị cắt cụt.",
        "The backup ZIP Central Directory is truncated.",
    ),
    (
        "package.a_backup_zip_central_directory_entry_has_an_invalid_signature",
        "Entry Central Directory backup ZIP sai chữ ký.",
        "A backup ZIP Central Directory entry has an invalid signature.",
    ),
    (
        "package.a_backup_zip_entry_violates_the_store_zip32_profile",
        "Entry backup ZIP vi phạm profile Store/ZIP32.",
        "A backup ZIP entry violates the Store/ZIP32 profile.",
    ),
    (
        "package.a_backup_zip_entry_name_extends_beyond_the_central_directory",
        "Tên entry backup ZIP vượt quá Central Directory.",
        "A backup ZIP entry name extends beyond the Central Directory.",
    ),
    (
        "package.invalid_backup_zip_entry_name",
        "Tên entry backup ZIP không hợp lệ.",
        "Invalid backup ZIP entry name.",
    ),
    (
        "package.a_backup_zip_payload_entry_extends_beyond_the_data_area_before_the_central_directory",
        "Payload entry backup ZIP vượt quá vùng dữ liệu trước Central Directory.",
        "A backup ZIP payload entry extends beyond the data area before the Central Directory.",
    ),
    (
        "package.the_backup_zip_central_directory_contains_extra_data",
        "Central Directory backup ZIP có dữ liệu dư thừa.",
        "The backup ZIP Central Directory contains extra data.",
    ),
    (
        "package.backup_zip_entries_have_overlapping_byte_ranges",
        "Các entry trong backup ZIP có vùng byte chồng chéo.",
        "Backup ZIP entries have overlapping byte ranges.",
    ),
    (
        "plugin.the_http_header_count_exceeds_50",
        "Số lượng header HTTP vượt quá 50.",
        "The HTTP header count exceeds 50.",
    ),
    (
        "plugin.a_header_key_or_value_exceeds_the_length_limit",
        "Header key hoặc value vượt quá giới hạn độ dài.",
        "A header key or value exceeds the length limit.",
    ),
    (
        "plugin.permission_denied_the_header_contains_control_characters_or_invalid_crlf",
        "permission_denied: Header chứa ký tự điều khiển hoặc CRLF không hợp lệ.",
        "permission_denied: The header contains control characters or invalid CRLF.",
    ),
    (
        "plugin.dns_error_no_ip_address_found",
        "Lỗi DNS: không tìm thấy địa chỉ IP.",
        "DNS error: no IP address found.",
    ),
    (
        "plugin.permission_denied_the_domain_resolves_to_a_prohibited_loopback_or_private_network_address",
        "permission_denied: Tên miền phân giải về địa chỉ loopback hoặc private network bị cấm.",
        "permission_denied: The domain resolves to a prohibited loopback or private network address.",
    ),
    (
        "plugin.permission_denied_access_to_loopback_or_private_networks_is_prohibited",
        "permission_denied: Không được phép truy cập mạng loopback hoặc private network.",
        "permission_denied: Access to loopback or private networks is prohibited.",
    ),
    (
        "plugin.execution_timed_out_deadline_exceeded",
        "Quá thời gian thực thi (deadline exceeded).",
        "Execution timed out (deadline exceeded).",
    ),
    (
        "plugin.dns_error_could_not_initialize_the_runtime",
        "Lỗi DNS: không khởi tạo được runtime.",
        "DNS error: could not initialize the runtime.",
    ),
    (
        "plugin.dns_error_could_not_resolve_the_domain",
        "Lỗi DNS: không thể phân giải tên miền.",
        "DNS error: could not resolve the domain.",
    ),
    (
        "plugin.dns_error_domain_resolution_timed_out",
        "Lỗi DNS: quá thời gian phân giải tên miền.",
        "DNS error: domain resolution timed out.",
    ),
    (
        "plugin.the_http_response_exceeds_the_1_mb_limit",
        "Response HTTP vượt quá giới hạn 1 MB.",
        "The HTTP response exceeds the 1 MB limit.",
    ),
    (
        "plugin.unsupported_http_method",
        "Method HTTP không được hỗ trợ.",
        "Unsupported HTTP method.",
    ),
    (
        "plugin.only_http_or_https_schemes_are_supported",
        "Chỉ hỗ trợ scheme http hoặc https.",
        "Only http or https schemes are supported.",
    ),
    (
        "plugin.permission_denied_the_url_must_not_contain_credentials_or_a_fragment",
        "permission_denied: URL không được chứa credentials hoặc fragment.",
        "permission_denied: The URL must not contain credentials or a fragment.",
    ),
    (
        "plugin.permission_denied_the_url_is_not_in_the_manifest_http_network_allowlist",
        "permission_denied: URL không nằm trong allowlist HTTP/network của manifest.",
        "permission_denied: The URL is not in the manifest HTTP/network allowlist.",
    ),
    (
        "plugin.permission_denied_egress_calls_to_the_internal_control_plane_are_prohibited",
        "permission_denied: Không được phép gọi control plane nội bộ qua egress.",
        "permission_denied: Egress calls to the internal control plane are prohibited.",
    ),
    (
        "plugin.the_http_request_body_exceeds_1_mb",
        "Body request HTTP vượt quá giới hạn 1 MB.",
        "The HTTP request body exceeds 1 MB.",
    ),
    (
        "plugin.http_connection_error_timed_out",
        "Lỗi kết nối HTTP: Quá thời gian chờ (timeout).",
        "HTTP connection error: timed out.",
    ),
    (
        "plugin.http_connection_error_redirect_blocked_by_policy",
        "Lỗi kết nối HTTP: Bị chặn chuyển hướng (redirect policy).",
        "HTTP connection error: redirect blocked by policy.",
    ),
    (
        "plugin.http_connection_error_could_not_complete_the_request",
        "Lỗi kết nối HTTP: Không thể hoàn thành yêu cầu.",
        "HTTP connection error: could not complete the request.",
    ),
    (
        "plugin.data_reads_are_prohibited_in_filter_contact_before_submit",
        "Thao tác đọc dữ liệu bị cấm trong filter:contact/before_submit.",
        "Data reads are prohibited in filter:contact/before_submit.",
    ),
    (
        "plugin.permission_denied_sqlite_kv_read",
        "permission_denied: Quyền sqlite:kv_read bị từ chối.",
        "permission_denied: sqlite:kv_read",
    ),
    (
        "plugin.permission_denied_action_kv_read",
        "permission_denied: Quyền action:kv_read bị từ chối.",
        "permission_denied: action:kv_read",
    ),
    (
        "plugin.kv_list_is_allowed_only_in_actions_or_async_post_commit_hooks",
        "kv_list chỉ được phép trong action hoặc async post-commit hook.",
        "kv_list is allowed only in actions or async post-commit hooks.",
    ),
    (
        "plugin.permission_denied_action_kv_list",
        "permission_denied: Quyền action:kv_list bị từ chối.",
        "permission_denied: action:kv_list",
    ),
    (
        "plugin.permission_denied_content_read",
        "permission_denied: Quyền content_read bị từ chối.",
        "permission_denied: content_read",
    ),
    (
        "plugin.permission_denied_action_content_read",
        "permission_denied: Quyền action:content_read bị từ chối.",
        "permission_denied: action:content_read",
    ),
    (
        "plugin.permission_denied_taxonomy_read",
        "permission_denied: Quyền taxonomy_read bị từ chối.",
        "permission_denied: taxonomy_read",
    ),
    (
        "plugin.permission_denied_action_taxonomy_read",
        "permission_denied: Quyền action:taxonomy_read bị từ chối.",
        "permission_denied: action:taxonomy_read",
    ),
    (
        "plugin.permission_denied_media_read",
        "permission_denied: Quyền media_read bị từ chối.",
        "permission_denied: media_read",
    ),
    (
        "plugin.permission_denied_action_media_read",
        "permission_denied: Quyền action:media_read bị từ chối.",
        "permission_denied: action:media_read",
    ),
    (
        "plugin.permission_denied_site_options_read",
        "permission_denied: Quyền site_options_read bị từ chối.",
        "permission_denied: site_options_read",
    ),
    (
        "plugin.permission_denied_action_site_options_read",
        "permission_denied: Quyền action:site_options_read bị từ chối.",
        "permission_denied: action:site_options_read",
    ),
    (
        "plugin.permission_denied_plugin_settings_read",
        "permission_denied: Quyền plugin_settings_read bị từ chối.",
        "permission_denied: plugin_settings_read",
    ),
    (
        "plugin.permission_denied_action_plugin_settings_read",
        "permission_denied: Quyền action:plugin_settings_read bị từ chối.",
        "permission_denied: action:plugin_settings_read",
    ),
    (
        "plugin.plugin_secret_get_is_prohibited_in_the_current_hook_filter",
        "plugin_secret_get bị cấm trong hook/filter hiện tại.",
        "plugin_secret_get is prohibited in the current hook/filter.",
    ),
    (
        "plugin.permission_denied_plugin_secrets_read",
        "permission_denied: Quyền plugin_secrets_read bị từ chối.",
        "permission_denied: plugin_secrets_read",
    ),
    (
        "plugin.permission_denied_action_plugin_secrets_read",
        "permission_denied: Quyền action:plugin_secrets_read bị từ chối.",
        "permission_denied: action:plugin_secrets_read",
    ),
    (
        "package.the_zip_contains_too_many_entries_maximum_256",
        "ZIP chứa quá nhiều entry (tối đa 256).",
        "The ZIP contains too many entries (maximum 256).",
    ),
    (
        "package.the_zip_contains_a_prohibited_symlink",
        "ZIP chứa symlink bị cấm.",
        "The ZIP contains a prohibited symlink.",
    ),
    (
        "package.directories_in_the_zip_must_be_empty",
        "Thư mục trong ZIP phải rỗng.",
        "Directories in the ZIP must be empty.",
    ),
    (
        "package.the_zip_contains_too_many_files_maximum_72",
        "ZIP chứa quá nhiều file (tối đa 72 file).",
        "The ZIP contains too many files (maximum 72).",
    ),
    (
        "package.the_zip_contains_duplicate_or_case_conflicting_entries",
        "ZIP chứa mục trùng lặp hoặc xung đột chữ hoa/thường.",
        "The ZIP contains duplicate or case-conflicting entries.",
    ),
    (
        "package.the_plugin_file_exceeds_the_extraction_limit",
        "File plugin vượt quá giới hạn giải nén.",
        "The plugin file exceeds the extraction limit.",
    ),
    (
        "package.the_zip_contains_an_abnormal_compression_ratio",
        "ZIP chứa tỷ lệ giải nén bất thường.",
        "The ZIP contains an abnormal compression ratio.",
    ),
    (
        "package.manifest_json_exceeds_the_64_kb_size_limit",
        "manifest.json vượt quá giới hạn kích thước 64 KB.",
        "manifest.json exceeds the 64 KB size limit.",
    ),
    (
        "package.plugin_wasm_exceeds_the_2_mb_size_limit",
        "File plugin.wasm vượt quá giới hạn kích thước 2 MB.",
        "plugin.wasm exceeds the 2 MB size limit.",
    ),
    (
        "package.a_file_in_the_zip_exceeds_the_size_limit",
        "File trong ZIP vượt quá giới hạn kích thước.",
        "A file in the ZIP exceeds the size limit.",
    ),
    (
        "package.the_actual_total_extracted_size_exceeds_10_mb",
        "Tổng dung lượng giải nén thực tế vượt quá giới hạn 10 MB.",
        "The actual total extracted size exceeds 10 MB.",
    ),
    (
        "package.the_zip_file_exceeds_5_mb",
        "Kích thước file ZIP vượt quá giới hạn 5 MB.",
        "The ZIP file exceeds 5 MB.",
    ),
    (
        "package.the_webassembly_module_must_export_opendash_run",
        "Module WebAssembly phải export 'opendash_run'.",
        "The WebAssembly module must export 'opendash_run'.",
    ),
    (
        "package.the_exported_opendash_run_function_must_have_signature_i32",
        "Hàm export 'opendash_run' phải có signature () -> i32.",
        "The exported 'opendash_run' function must have signature () -> i32.",
    ),
    (
        "package.the_opendash_run_export_must_be_a_function",
        "Export 'opendash_run' phải là function.",
        "The 'opendash_run' export must be a function.",
    ),
    (
        "package.the_webassembly_module_must_export_memory",
        "Module WebAssembly phải export 'memory'.",
        "The WebAssembly module must export 'memory'.",
    ),
    (
        "package.the_memory_export_must_be_linear_memory",
        "Export 'memory' phải là linear memory.",
        "The 'memory' export must be linear memory.",
    ),
    (
        "package.the_webassembly_module_contains_a_prohibited_import_wasi_unknown_env_rejected",
        "Module WebAssembly chứa import không được phép (từ chối WASI/unknown env).",
        "The WebAssembly module contains a prohibited import (WASI/unknown env rejected).",
    ),
    (
        "package.the_total_plugin_size_exceeds_10_mb",
        "Tổng dung lượng plugin vượt quá giới hạn 10 MB.",
        "The total plugin size exceeds 10 MB.",
    ),
    (
        "public_context.the_query_contains_invalid_encoding",
        "Query có mã hóa không hợp lệ.",
        "The query contains invalid encoding.",
    ),
    (
        "public_context.public_parameters_must_not_be_repeated",
        "Không được lặp tham số công khai.",
        "Public parameters must not be repeated.",
    ),
    (
        "public_context.invalid_boolean_value",
        "Giá trị boolean không hợp lệ.",
        "Invalid boolean value.",
    ),
    (
        "public_context.the_public_page_exceeds_the_limit",
        "Trang công khai vượt giới hạn.",
        "The public page exceeds the limit.",
    ),
    (
        "public_context.invalid_taxonomy_key",
        "taxonomy_key không hợp lệ.",
        "Invalid taxonomy_key.",
    ),
    (
        "public_context.the_query_parameter_cannot_be_used_on_this_route",
        "Tham số query không dùng được trên route này.",
        "The query parameter cannot be used on this route.",
    ),
    (
        "public_context.taxonomy_key_and_term_id_must_be_used_together",
        "taxonomy_key và term_id phải dùng cùng nhau.",
        "taxonomy_key and term_id must be used together.",
    ),
    (
        "public_context.generic_taxonomy_filters_cannot_be_mixed_with_category_tag_aliases",
        "Không được trộn generic taxonomy filter với category/tag aliases.",
        "Generic taxonomy filters cannot be mixed with category/tag aliases.",
    ),
    (
        "public_context.include_descendants_requires_category_id_or_taxonomy_key_term_id",
        "include_descendants yêu cầu category_id hoặc taxonomy_key + term_id.",
        "include_descendants requires category_id or taxonomy_key + term_id.",
    ),
    (
        "public_context.descendants_requires_category_id_or_a_hierarchical_generic_taxonomy",
        "descendants yêu cầu category_id hoặc hierarchical generic taxonomy.",
        "descendants requires category_id or a hierarchical generic taxonomy.",
    ),
    (
        "contact.only_application_x_www_form_urlencoded_is_accepted",
        "Chỉ chấp nhận application/x-www-form-urlencoded.",
        "Only application/x-www-form-urlencoded is accepted.",
    ),
    (
        "menu.menu_data_exceeds_256_kib",
        "Dữ liệu menu vượt quá giới hạn 256 KiB.",
        "Menu data exceeds 256 KiB.",
    ),
    (
        "operations.the_maximum_of_10_backup_packages_has_been_reached_delete_some_before_creating_another",
        "Đã đạt giới hạn tối đa 10 gói backup; vui lòng xóa bớt trước khi tạo mới.",
        "The maximum of 10 backup packages has been reached; delete some before creating another.",
    ),
    (
        "operations.only_application_octet_stream_is_accepted_for_restore_packages",
        "Chỉ chấp nhận application/octet-stream cho gói restore.",
        "Only application/octet-stream is accepted for restore packages.",
    ),
    (
        "operations.the_backup_package_exceeds_the_allowed_size",
        "Kích thước gói backup vượt quá giới hạn cho phép.",
        "The backup package exceeds the allowed size.",
    ),
    (
        "operations.the_restore_package_upload_timed_out",
        "Quá thời gian tải lên gói restore.",
        "The restore package upload timed out.",
    ),
    (
        "plugin.plugin_settings_data_exceeds_64_kib",
        "Dữ liệu cài đặt plugin vượt quá 64 KiB.",
        "Plugin settings data exceeds 64 KiB.",
    ),
    (
        "plugin.the_declarative_ui_schema_exceeds_the_maximum_depth_8",
        "Declarative UI schema vượt quá độ sâu tối đa (8).",
        "The declarative UI schema exceeds the maximum depth (8).",
    ),
    (
        "plugin.the_declarative_ui_schema_exceeds_the_maximum_node_count_200",
        "Declarative UI schema vượt quá số lượng node tối đa (200).",
        "The declarative UI schema exceeds the maximum node count (200).",
    ),
    (
        "plugin.invalid_node_id",
        "Node id không hợp lệ.",
        "Invalid node ID.",
    ),
    (
        "plugin.labels_must_not_contain_html_js",
        "Label không được chứa HTML/JS.",
        "Labels must not contain HTML/JS.",
    ),
    (
        "plugin.page_size_exceeds_the_limit_of_100",
        "page_size vượt quá giới hạn 100.",
        "page_size exceeds the limit of 100.",
    ),
    (
        "plugin.a_button_must_have_an_action_id",
        "Button bắt buộc phải gắn action_id.",
        "A button must have an action_id.",
    ),
    (
        "plugin.the_button_references_an_action_id_that_does_not_exist_in_the_manifest",
        "Button tham chiếu action_id không tồn tại trong manifest.",
        "The button references an action_id that does not exist in the manifest.",
    ),
    (
        "plugin.the_field_type_is_unsupported_or_prohibited_secret_html_js_css_are_not_supported",
        "Field type không được hỗ trợ hoặc bị cấm (không hỗ trợ secret/html/js/css).",
        "The field type is unsupported or prohibited (secret/html/js/css are not supported).",
    ),
    (
        "plugin.the_node_kind_is_not_in_schema_v1",
        "Node kind không thuộc schema v1.",
        "The node kind is not in schema v1.",
    ),
    (
        "plugin.action_id_does_not_exist_in_the_manifest",
        "action_id không tồn tại trong manifest.",
        "action_id does not exist in the manifest.",
    ),
    (
        "plugin.only_declarative_ui_schema_version_1_is_supported",
        "Chỉ hỗ trợ declarative UI schema_version 1.",
        "Only declarative UI schema_version 1 is supported.",
    ),
    (
        "plugin.the_screen_count_exceeds_the_limit_of_20",
        "Số lượng screen vượt quá giới hạn 20.",
        "The screen count exceeds the limit of 20.",
    ),
    (
        "plugin.invalid_screen",
        "Screen không hợp lệ.",
        "Invalid screen.",
    ),
    (
        "storage.busy",
        "SQLite đang bận, vui lòng thử lại.",
        "SQLite is busy; please try again.",
    ),
    (
        "settings.public_settings_data_exceeds_4_kib",
        "Dữ liệu cài đặt công khai vượt quá 4 KiB.",
        "Public settings data exceeds 4 KiB.",
    ),
    (
        "settings.permalink_configuration_exceeds_16_kib",
        "Cấu hình permalink vượt quá 16 KiB.",
        "Permalink configuration exceeds 16 KiB.",
    ),
    (
        "settings.site_wide_options_data_exceeds_64_kib",
        "Dữ liệu tùy chọn toàn trang vượt quá 64 KiB.",
        "Site-wide options data exceeds 64 KiB.",
    ),
    (
        "seo.custom_paths_must_contain_2_to_256_bytes",
        "Đường dẫn tùy chỉnh phải từ 2 đến 256 bytes.",
        "Custom paths must contain 2 to 256 bytes.",
    ),
    (
        "seo.custom_paths_must_start_with_and_not_end_with",
        "Đường dẫn tùy chỉnh phải bắt đầu bằng '/' và không có '/' ở cuối.",
        "Custom paths must start with '/' and not end with '/'.",
    ),
    (
        "seo.custom_paths_must_not_contain_whitespace_or_control_characters",
        "Đường dẫn tùy chỉnh không được chứa khoảng trắng hoặc ký tự điều khiển.",
        "Custom paths must not contain whitespace or control characters.",
    ),
    (
        "seo.custom_paths_must_not_contain_or_dots",
        "Đường dẫn tùy chỉnh không được chứa ?, #, %, \\ hoặc dấu chấm.",
        "Custom paths must not contain ?, #, %, \\\\ or dots.",
    ),
    (
        "seo.custom_paths_may_contain_only_1_to_5_segments",
        "Đường dẫn tùy chỉnh chỉ được có từ 1 đến 5 đoạn (segments).",
        "Custom paths may contain only 1 to 5 segments.",
    ),
    (
        "seo.each_path_segment_must_be_ascii_kebab_case_1_64_bytes",
        "Mỗi đoạn đường dẫn phải là ASCII kebab-case (1–64 bytes).",
        "Each path segment must be ASCII kebab-case (1–64 bytes).",
    ),
    (
        "seo.the_path_must_not_conflict_with_a_reserved_system_prefix",
        "Đường dẫn không được trùng với tiền tố hệ thống được bảo vệ.",
        "The path must not conflict with a reserved system prefix.",
    ),
    (
        "seo.each_json_ld_element_must_be_a_json_object",
        "Mỗi phần tử json_ld bắt buộc phải là JSON Object.",
        "Each json_ld element must be a JSON object.",
    ),
    (
        "seo.a_json_ld_block_exceeds_the_maximum_depth_of_8_levels",
        "Block json_ld vượt quá độ sâu tối đa 8 tầng.",
        "A json_ld block exceeds the maximum depth of 8 levels.",
    ),
    (
        "seo.a_json_ld_block_exceeds_16_kib",
        "Block json_ld vượt quá giới hạn 16 KiB.",
        "A json_ld block exceeds 16 KiB.",
    ),
    (
        "seo.json_ld_exceeds_the_maximum_of_4_blocks_per_page",
        "json_ld vượt quá tối đa 4 block trên một trang.",
        "json_ld exceeds the maximum of 4 blocks per page.",
    ),
    (
        "seo.the_total_json_ld_size_exceeds_16_kib",
        "Tổng dung lượng json_ld vượt quá 16 KiB.",
        "The total json_ld size exceeds 16 KiB.",
    ),
    (
        "smtp.invalid_port",
        "invalid_port: Cổng không hợp lệ.",
        "invalid_port",
    ),
    (
        "smtp.could_not_send_the_test_email_through_the_smtp_server",
        "Không thể gửi email kiểm tra qua máy chủ SMTP.",
        "Could not send the test email through the SMTP server.",
    ),
    (
        "smtp.the_smtp_connection_timed_out_10s",
        "Hết thời gian chờ kết nối SMTP (10s).",
        "The SMTP connection timed out (10s).",
    ),
    (
        "entries.missing_schema_token_expected_schema_token_is_required",
        "missing_schema_token: Thiếu expected_schema_token.",
        "missing_schema_token: expected_schema_token is required.",
    ),
    (
        "entries.missing_expected_version_expected_version_is_required_when_updating",
        "missing_expected_version: Thiếu expected_version khi cập nhật.",
        "missing_expected_version: expected_version is required when updating.",
    ),
    (
        "import.sample_media_ref_is_only_valid_with_image_ref_null_and_declared_media",
        "Sample media_ref chỉ dùng với image_ref, null và media có khai báo.",
        "Sample media_ref is only valid with image_ref, null and declared media.",
    ),
    (
        "import.invalid_sample_key",
        "sample_key không hợp lệ.",
        "Invalid sample_key.",
    ),
    (
        "import.the_sample_data_item_count_exceeds_the_allowed_limit",
        "Số lượng phần tử dữ liệu mẫu vượt quá giới hạn cho phép.",
        "The sample data item count exceeds the allowed limit.",
    ),
    (
        "import.the_sample_media_object_key_is_invalid_or_duplicated",
        "object_key của sample media không hợp lệ hoặc trùng lặp.",
        "The sample media object_key is invalid or duplicated.",
    ),
    (
        "import.invalid_sample_media_path",
        "Đường dẫn media trong sample không hợp lệ.",
        "Invalid sample media path.",
    ),
    (
        "import.the_media_file_declared_in_the_sample_does_not_exist_in_the_theme_revision",
        "File media được khai báo trong sample không tồn tại trong revision theme.",
        "The media file declared in the sample does not exist in the theme revision.",
    ),
    (
        "import.the_sample_page_object_key_is_invalid_or_duplicated",
        "object_key của sample page không hợp lệ hoặc trùng lặp.",
        "The sample page object_key is invalid or duplicated.",
    ),
    (
        "import.invalid_sample_page_slug_title_content_type",
        "Thông tin slug/title/content_type của sample page không hợp lệ.",
        "Invalid sample page slug/title/content_type.",
    ),
    (
        "import.the_sample_page_references_a_media_ref_that_does_not_exist_in_the_sample",
        "Sample page tham chiếu tới media_ref không tồn tại trong sample.",
        "The sample page references a media_ref that does not exist in the sample.",
    ),
    (
        "import.invalid_or_duplicate_sample_menu_declaration",
        "Khai báo sample menu không hợp lệ hoặc trùng lặp.",
        "Invalid or duplicate sample menu declaration.",
    ),
    (
        "import.the_entry_menu_item_is_missing_target_object_key",
        "Menu entry kiểu 'entry' thiếu target_object_key.",
        "The 'entry' menu item is missing target_object_key.",
    ),
    (
        "import.the_sample_menu_references_a_page_that_does_not_exist_in_the_sample",
        "Sample menu tham chiếu tới page không tồn tại trong sample.",
        "The sample menu references a page that does not exist in the sample.",
    ),
    (
        "import.the_external_menu_item_is_missing_url",
        "Menu entry kiểu 'external' thiếu url.",
        "The 'external' menu item is missing url.",
    ),
    (
        "import.unsupported_sample_menu_target_type",
        "Loại target menu sample không được hỗ trợ.",
        "Unsupported sample menu target type.",
    ),
    (
        "import.invalid_or_duplicate_sample_setting_declaration",
        "Khai báo sample setting không hợp lệ hoặc trùng lặp.",
        "Invalid or duplicate sample setting declaration.",
    ),
    (
        "import.the_sample_setting_references_a_media_ref_that_does_not_exist_in_the_sample",
        "Sample setting tham chiếu tới media_ref không tồn tại trong sample.",
        "The sample setting references a media_ref that does not exist in the sample.",
    ),
    (
        "import.invalid_sample_declaration_format_in_theme_json",
        "Khai báo sample trong theme.json không đúng định dạng.",
        "Invalid sample declaration format in theme.json.",
    ),
    (
        "import.the_plan_still_has_unselected_dependencies",
        "Kế hoạch vẫn còn dependency chưa được chọn.",
        "The plan still has unselected dependencies.",
    ),
    (
        "import.confirm_public_display_changes_menus_options_before_executing",
        "Cần xác nhận thay đổi hiển thị công khai (menu/options) trước khi thực thi.",
        "Confirm public display changes (menus/options) before executing.",
    ),
    (
        "plugin.the_plugin_revision_directory_does_not_exist_on_disk",
        "Thư mục revision plugin trên đĩa không tồn tại.",
        "The plugin revision directory does not exist on disk.",
    ),
    (
        "plugin.the_plugin_revision_s_complete_marker_does_not_match_its_digest",
        "Marker .complete của revision plugin không khớp digest.",
        "The plugin revision's .complete marker does not match its digest.",
    ),
    (
        "plugin.the_admin_extension_revision_is_no_longer_canonical",
        "Admin extension revision không còn canonical.",
        "The Admin extension revision is no longer canonical.",
    ),
    (
        "plugin.invalid_plugin_revision_blocks_json",
        "Revision plugin blocks.json không hợp lệ.",
        "Invalid plugin revision blocks.json.",
    ),
    (
        "plugin.the_slug_in_plugin_revision_blocks_json_does_not_match",
        "Revision plugin blocks.json có slug không khớp.",
        "The slug in plugin revision blocks.json does not match.",
    ),
    (
        "plugin.plugin_revision_blocks_json_is_not_canonical",
        "Revision plugin blocks.json không canonical.",
        "Plugin revision blocks.json is not canonical.",
    ),
    (
        "plugin.the_plugin_revision_contents_on_disk_are_corrupted_or_do_not_match_the_digest",
        "Nội dung revision plugin trên đĩa bị hỏng hoặc lệch digest.",
        "The plugin revision contents on disk are corrupted or do not match the digest.",
    ),
    (
        "plugin.invalid_plugin_staging_run_uuid",
        "run_uuid staging plugin không hợp lệ.",
        "Invalid plugin staging run_uuid.",
    ),
    (
        "plugin.error",
        "Plugin không tồn tại.",
        "Plugin does not exist.",
    ),
    (
        "plugin_projections.the_filter_entry_view_output_must_be_a_json_object",
        "Đầu ra filter:entry/view bắt buộc phải là JSON Object.",
        "The filter:entry/view output must be a JSON object.",
    ),
    (
        "plugin_projections.the_filter_entry_view_output_exceeds_64_keys",
        "Đầu ra filter:entry/view vượt quá tối đa 64 keys.",
        "The filter:entry/view output exceeds 64 keys.",
    ),
    (
        "plugin_projections.the_filter_entry_view_output_exceeds_the_maximum_depth_of_6_levels",
        "Đầu ra filter:entry/view vượt quá độ sâu tối đa 6 tầng.",
        "The filter:entry/view output exceeds the maximum depth of 6 levels.",
    ),
    (
        "plugin_projections.the_filter_entry_view_output_exceeds_32_kib",
        "Đầu ra filter:entry/view vượt quá giới hạn 32 KiB.",
        "The filter:entry/view output exceeds 32 KiB.",
    ),
    (
        "locale.the_content_language_must_be_a_valid_ascii_bcp47_code_at_most_35_bytes_e_g_vi_or_en_us",
        "Ngôn ngữ nội dung phải là mã BCP47 ASCII hợp lệ (tối đa 35 bytes, ví dụ vi hoặc en-US).",
        "The content language must be a valid ASCII BCP47 code (at most 35 bytes, e.g. vi or en-US).",
    ),
    (
        "locale.the_language_code_conflicts_with_a_reserved_system_prefix",
        "Mã ngôn ngữ trùng với tiền tố hệ thống được bảo vệ.",
        "The language code conflicts with a reserved system prefix.",
    ),
    (
        "locale.language_labels_must_not_contain_control_characters",
        "Nhãn ngôn ngữ không được chứa ký tự điều khiển.",
        "Language labels must not contain control characters.",
    ),
    (
        "locale.the_language_label_must_contain_1_to_80_characters",
        "Nhãn ngôn ngữ phải có từ 1 đến 80 ký tự.",
        "The language label must contain 1 to 80 characters.",
    ),
    (
        "locale.language_direction_only_accepts_ltr_or_rtl",
        "Hướng hiển thị ngôn ngữ chỉ chấp nhận 'ltr' hoặc 'rtl'.",
        "Language direction only accepts 'ltr' or 'rtl'.",
    ),
    (
        "settings.the_site_title_or_tagline_contains_control_characters",
        "Tên website hoặc tagline chứa ký tự điều khiển.",
        "The site title or tagline contains control characters.",
    ),
    (
        "settings.the_site_title_must_contain_1_120_unicode_characters_without_control_characters",
        "Tên website phải có 1–120 ký tự Unicode, không chứa ký tự điều khiển.",
        "The site title must contain 1–120 Unicode characters without control characters.",
    ),
    (
        "settings.the_tagline_must_contain_at_most_240_unicode_characters_without_control_characters",
        "Tagline phải có tối đa 240 ký tự Unicode, không chứa ký tự điều khiển.",
        "The tagline must contain at most 240 Unicode characters without control characters.",
    ),
    (
        "settings.invalid_timezone",
        "Múi giờ không hợp lệ.",
        "Invalid time zone.",
    ),
    (
        "settings.the_site_wide_option_does_not_exist_in_the_schema",
        "Option toàn trang không tồn tại trong schema.",
        "The site-wide option does not exist in the schema.",
    ),
    (
        "settings.a_media_option_must_be_a_valid_sha_256_media_id_or_empty",
        "Option media phải là media_id SHA-256 hợp lệ hoặc rỗng.",
        "A media option must be a valid SHA-256 media_id or empty.",
    ),
    (
        "settings.a_required_media_option_must_not_be_empty",
        "Option media bắt buộc không được để trống.",
        "A required media option must not be empty.",
    ),
    (
        "settings.a_media_option_must_be_a_media_id_string_or_null",
        "Option media phải là chuỗi media_id hoặc null.",
        "A media option must be a media_id string or null.",
    ),
    (
        "settings.a_string_option_exceeds_4_096_characters_or_contains_control_characters",
        "Option string vượt quá 4,096 ký tự hoặc chứa ký tự điều khiển.",
        "A string option exceeds 4,096 characters or contains control characters.",
    ),
    (
        "settings.a_number_option_must_be_a_finite_number",
        "Option number phải là số hữu hạn.",
        "A number option must be a finite number.",
    ),
    (
        "settings.the_number_option_is_below_its_minimum_value",
        "Option number nhỏ hơn giá trị min.",
        "The number option is below its minimum value.",
    ),
    (
        "settings.the_number_option_is_above_its_maximum_value",
        "Option number lớn hơn giá trị max.",
        "The number option is above its maximum value.",
    ),
    (
        "settings.a_boolean_option_must_be_true_or_false",
        "Option kiểu boolean phải là true hoặc false.",
        "A boolean option must be true or false.",
    ),
    (
        "settings.the_select_value_is_not_in_the_choices_list",
        "Giá trị select không nằm trong danh sách choices.",
        "The select value is not in the choices list.",
    ),
    (
        "settings.an_image_ref_option_must_be_a_valid_media_id_or_null",
        "Option image_ref phải là media_id hợp lệ hoặc null.",
        "An image_ref option must be a valid media_id or null.",
    ),
    (
        "settings.the_option_is_missing_a_modern_schema_spec",
        "Option thiếu schema spec hiện đại.",
        "The option is missing a modern schema spec.",
    ),
    (
        "settings.invalid_options_declaration_structure_in_theme_json",
        "Khai báo options trong theme.json không đúng cấu trúc.",
        "Invalid options declaration structure in theme.json.",
    ),
    (
        "settings.the_option_s_public_property_must_be_boolean",
        "Thuộc tính public của option phải là boolean.",
        "The option's public property must be boolean.",
    ),
    (
        "settings.the_option_s_translatable_property_must_be_boolean",
        "Thuộc tính translatable của option phải là boolean.",
        "The option's translatable property must be boolean.",
    ),
    (
        "settings.option_key_must_be_snake_case_1_64_characters",
        "option_key phải là snake_case (1–64 ký tự).",
        "option_key must be snake_case (1–64 characters).",
    ),
    (
        "settings.duplicate_option_key_in_theme_json",
        "Trùng lặp option_key trong theme.json.",
        "Duplicate option_key in theme.json.",
    ),
    (
        "settings.invalid_option_label",
        "Nhãn option không hợp lệ.",
        "Invalid option label.",
    ),
    (
        "settings.invalid_option_minimum",
        "Giá trị min của option không hợp lệ.",
        "Invalid option minimum.",
    ),
    (
        "settings.invalid_option_maximum",
        "Giá trị max của option không hợp lệ.",
        "Invalid option maximum.",
    ),
    (
        "settings.min_must_not_be_greater_than_max",
        "min không được lớn hơn max.",
        "min must not be greater than max.",
    ),
    (
        "settings.a_select_option_must_have_1_64_choices",
        "Option select phải có 1–64 choices.",
        "A select option must have 1–64 choices.",
    ),
    (
        "settings.a_select_option_choice_is_invalid_or_duplicated",
        "Choice trong option select không hợp lệ hoặc bị trùng.",
        "A select option choice is invalid or duplicated.",
    ),
    (
        "settings.only_string_or_url_options_may_declare_translatable_true",
        "Chỉ option kiểu string hoặc url mới được khai báo translatable: true.",
        "Only string or url options may declare translatable: true.",
    ),
    (
        "settings.the_theme_exceeds_the_limit_of_64_options",
        "Theme vượt quá giới hạn 64 options.",
        "The theme exceeds the limit of 64 options.",
    ),
    (
        "settings.public_origin_must_not_contain_whitespace_or_control_characters",
        "Public origin không được chứa khoảng trắng hoặc ký tự điều khiển.",
        "Public origin must not contain whitespace or control characters.",
    ),
    (
        "settings.public_origin_must_not_contain_a_query_or_fragment",
        "Public origin không được chứa query hoặc fragment.",
        "Public origin must not contain a query or fragment.",
    ),
    (
        "settings.public_origin_only_supports_http_or_https",
        "Public origin chỉ hỗ trợ giao thức http hoặc https.",
        "Public origin only supports http or https.",
    ),
    (
        "settings.public_origin_must_not_contain_credentials",
        "Public origin không được chứa thông tin đăng nhập.",
        "Public origin must not contain credentials.",
    ),
    (
        "settings.public_origin_must_not_contain_a_subpath",
        "Public origin không được chứa đường dẫn con.",
        "Public origin must not contain a subpath.",
    ),
    (
        "settings.public_origin_must_have_a_valid_hostname",
        "Public origin phải có tên miền hợp lệ.",
        "Public origin must have a valid hostname.",
    ),
    (
        "template_sec.composition_fuel_is_invalid_or_exhausted",
        "Fuel composition không hợp lệ hoặc đã hết.",
        "Composition fuel is invalid or exhausted.",
    ),
    (
        "template_sec.invalid_composition_fuel",
        "Fuel composition không hợp lệ.",
        "Invalid composition fuel.",
    ),
    (
        "template_sec.the_theme_content_nonce_does_not_occur_exactly_once",
        "Nonce nội dung của theme không xuất hiện đúng một lần.",
        "The theme content nonce does not occur exactly once.",
    ),
    (
        "template_sec.html_after_content_insertion_exceeds_512_kib",
        "HTML sau khi chèn nội dung vượt quá giới hạn 512 KiB.",
        "HTML after content insertion exceeds 512 KiB.",
    ),
    (
        "template_sec.a_template_part_must_not_contain_od_content",
        "Template part không được chứa od_content().",
        "A template part must not contain od_content().",
    ),
    (
        "template_sec.the_template_or_part_exceeds_512_kib",
        "Template hoặc part vượt giới hạn 512 KiB.",
        "The template or part exceeds 512 KiB.",
    ),
    (
        "template_sec.theme_html_contains_a_prohibited_internal_marker",
        "Theme HTML chứa marker nội bộ bị cấm.",
        "Theme HTML contains a prohibited internal marker.",
    ),
    (
        "template_sec.preview_fuel_is_invalid_or_exhausted",
        "Fuel preview không hợp lệ hoặc đã hết.",
        "Preview fuel is invalid or exhausted.",
    ),
    (
        "template_sec.od_part_exceeds_32_expansions",
        "od_part vượt quá 32 lần mở rộng.",
        "od_part exceeds 32 expansions.",
    ),
    (
        "template_sec.od_part_depth_exceeds_3",
        "Độ sâu od_part vượt giới hạn 3.",
        "od_part depth exceeds 3.",
    ),
    (
        "template_sec.part_html_exceeds_512_kib",
        "HTML part vượt quá giới hạn 512 KiB.",
        "Part HTML exceeds 512 KiB.",
    ),
    (
        "template_sec.duplicate_preview_nonce",
        "Nonce preview bị trùng.",
        "Duplicate preview nonce.",
    ),
    (
        "template_sec.preview_html_exceeds_512_kib",
        "HTML preview vượt quá giới hạn 512 KiB.",
        "Preview HTML exceeds 512 KiB.",
    ),
    (
        "template_sec.the_preview_nonce_appears_more_than_once",
        "Nonce preview xuất hiện nhiều hơn một lần.",
        "The preview nonce appears more than once.",
    ),
    (
        "template_sec.the_preview_nonce_is_not_in_the_html_body",
        "Nonce preview không nằm trong thân HTML.",
        "The preview nonce is not in the HTML body.",
    ),
    (
        "template_sec.the_template_must_not_be_empty",
        "Template không được để trống.",
        "The template must not be empty.",
    ),
    (
        "template_sec.invalid_part_key",
        "Part key không hợp lệ.",
        "Invalid part key.",
    ),
    (
        "template_sec.an_entry_template_must_contain_exactly_one_od_content",
        "Template bản ghi phải có đúng một od_content().",
        "An entry template must contain exactly one od_content().",
    ),
    (
        "template_sec.index_may_contain_at_most_one_od_content",
        "Index chỉ được có tối đa một od_content().",
        "Index may contain at most one od_content().",
    ),
    (
        "template_sec.non_entry_templates_must_not_contain_od_content",
        "Template không phải bản ghi cấm od_content().",
        "Non-entry templates must not contain od_content().",
    ),
    (
        "template_sec.template_composition_exceeds_512_kib",
        "Template composition vượt giới hạn 512 KiB.",
        "Template composition exceeds 512 KiB.",
    ),
    (
        "template_sec.an_od_part_cycle_was_detected",
        "Phát hiện vòng lặp od_part.",
        "An od_part cycle was detected.",
    ),
    (
        "template_sec.invalid_od_part_od_content_position",
        "Vị trí od_part/od_content không hợp lệ.",
        "Invalid od_part/od_content position.",
    ),
    (
        "template_sec.od_part_od_content_must_not_be_inside_a_for_or_set_block",
        "od_part/od_content không được nằm trong for hoặc set block.",
        "od_part/od_content must not be inside a for or set block.",
    ),
    (
        "template_sec.od_part_od_content_may_appear_only_in_the_html_body",
        "od_part/od_content chỉ được xuất hiện ở phần thân HTML.",
        "od_part/od_content may appear only in the HTML body.",
    ),
    (
        "template_sec.the_composition_expression_contains_a_token_outside_the_allowlist",
        "Biểu thức composition chứa token ngoài allowlist.",
        "The composition expression contains a token outside the allowlist.",
    ),
    (
        "template_sec.c_sec_prohibits_modifier_filters_in_set_blocks",
        "C-SEC cấm modifier filter trong khối set.",
        "C-SEC prohibits modifier filters in set blocks.",
    ),
    (
        "template_sec.od_content_accepts_no_arguments",
        "od_content() không nhận tham số.",
        "od_content() accepts no arguments.",
    ),
    (
        "template_sec.od_part_accepts_only_a_string_literal",
        "od_part() chỉ nhận một string literal.",
        "od_part() accepts only a string literal.",
    ),
    (
        "template_sec.invalid_od_part_key",
        "od_part key không hợp lệ.",
        "Invalid od_part key.",
    ),
    (
        "template_sec.c_sec_prohibits_function_calls_other_than_od_part_od_content",
        "C-SEC cấm lời gọi hàm Call ngoài od_part/od_content.",
        "C-SEC prohibits function calls other than od_part/od_content.",
    ),
    (
        "template_sec.too_many_html_states_at_the_composition_point",
        "Quá nhiều trạng thái HTML tại điểm composition.",
        "Too many HTML states at the composition point.",
    ),
    (
        "template_sec.css_violates_c_sec_policy_contains_import_expression_binding_breakout",
        "CSS vi phạm chính sách C-SEC (chứa @import/expression/binding/breakout).",
        "CSS violates C-SEC policy (contains @import/expression/binding/breakout).",
    ),
    (
        "template_sec.invalid_css_url",
        "CSS url(...) không hợp lệ.",
        "Invalid CSS url(...).",
    ),
    (
        "template_sec.css_url_may_only_point_to_theme_or_media",
        "CSS url(...) chỉ được phép trỏ tới /theme/... hoặc /media/....",
        "CSS url(...) may only point to /theme/... or /media/....",
    ),
    (
        "template_sec.scoped_block_css_contains_prohibited_characters_or_comments",
        "Scoped block CSS chứa ký tự hoặc comment bị cấm.",
        "Scoped block CSS contains prohibited characters or comments.",
    ),
    (
        "template_sec.scoped_block_css_violates_c_sec_policy",
        "Scoped block CSS vi phạm chính sách C-SEC.",
        "Scoped block CSS violates C-SEC policy.",
    ),
    (
        "template_sec.scoped_block_css_url_is_invalid_or_prohibited",
        "Scoped block CSS url(...) không hợp lệ hoặc không được phép.",
        "Scoped block CSS url(...) is invalid or prohibited.",
    ),
    (
        "template_sec.a_template_url_contains_prohibited_control_characters",
        "URL trong template chứa ký tự điều khiển bị cấm.",
        "A template URL contains prohibited control characters.",
    ),
    (
        "template_sec.invalid_template_url",
        "URL trong template không hợp lệ.",
        "Invalid template URL.",
    ),
    (
        "template_sec.relative_urls_must_not_contain_or",
        "URL tương đối không được chứa // hoặc ..",
        "Relative URLs must not contain // or ..",
    ),
    (
        "template_sec.only_http_s_urls_or_paths_are_allowed",
        "Chỉ cho phép URL http(s) hoặc đường dẫn /...",
        "Only http(s) URLs or /... paths are allowed.",
    ),
    (
        "template_sec.urls_must_not_contain_credentials",
        "URL không được chứa thông tin đăng nhập.",
        "URLs must not contain credentials.",
    ),
    (
        "template_sec.an_href_url_contains_prohibited_control_characters",
        "URL href chứa ký tự điều khiển bị cấm.",
        "An href URL contains prohibited control characters.",
    ),
    (
        "template_sec.the_url_in_a_href_is_invalid_or_unsafe",
        "URL trong <a href> không hợp lệ hoặc không an toàn.",
        "The URL in <a href> is invalid or unsafe.",
    ),
    (
        "template_sec.theme_urls_must_contain_1_4096_unicode_characters",
        "Theme URL phải có 1–4096 ký tự Unicode.",
        "Theme URLs must contain 1–4096 Unicode characters.",
    ),
    (
        "template_sec.the_theme_url_contains_unsafe_characters",
        "Theme URL chứa ký tự không an toàn.",
        "The theme URL contains unsafe characters.",
    ),
    (
        "template_sec.the_template_uses_a_minijinja_statement_prohibited_by_c_sec",
        "Template sử dụng câu lệnh MiniJinja bị C-SEC cấm.",
        "The template uses a MiniJinja statement prohibited by C-SEC.",
    ),
    (
        "template_sec.c_sec_prohibits_reassignment_of_system_context_variables_in_themes",
        "C-SEC cấm gán lại biến ngữ cảnh hệ thống trong theme.",
        "C-SEC prohibits reassignment of system context variables in themes.",
    ),
    (
        "template_sec.c_sec_prohibits_access_to_dunder_attributes",
        "C-SEC cấm truy cập thuộc tính dunder (__).",
        "C-SEC prohibits access to dunder (__) attributes.",
    ),
    (
        "template_sec.c_sec_prohibits_dynamic_t_access_use_t_key",
        "C-SEC cấm truy cập t[...] động; hãy dùng t.<key>.",
        "C-SEC prohibits dynamic t[...] access; use t.<key>.",
    ),
    (
        "template_sec.the_template_uses_a_minijinja_filter_outside_the_c_sec_allowlist",
        "Template sử dụng filter MiniJinja không nằm trong whitelist C-SEC.",
        "The template uses a MiniJinja filter outside the C-SEC allowlist.",
    ),
    (
        "template_sec.the_template_uses_a_minijinja_test_outside_the_c_sec_allowlist",
        "Template sử dụng test MiniJinja không nằm trong whitelist C-SEC.",
        "The template uses a MiniJinja test outside the C-SEC allowlist.",
    ),
    (
        "template_sec.c_sec_prohibits_function_calls_in_templates",
        "C-SEC cấm lời gọi hàm Call trong template.",
        "C-SEC prohibits function calls in templates.",
    ),
    (
        "template_sec.the_html_tag_lacks_a_valid_name",
        "Thẻ HTML không có tên hợp lệ.",
        "The HTML tag lacks a valid name.",
    ),
    (
        "template_sec.an_html_closing_tag_is_invalid_at_this_position",
        "Thẻ đóng HTML không hợp lệ ở vị trí này.",
        "An HTML closing tag is invalid at this position.",
    ),
    (
        "template_sec.the_template_contains_an_html_tag_prohibited_by_c_sec",
        "Template chứa thẻ HTML bị C-SEC cấm.",
        "The template contains an HTML tag prohibited by C-SEC.",
    ),
    (
        "template_sec.the_html_tag_contains_duplicate_attributes",
        "Thẻ HTML chứa thuộc tính bị trùng lặp.",
        "The HTML tag contains duplicate attributes.",
    ),
    (
        "template_sec.the_template_contains_an_html_attribute_prohibited_by_c_sec",
        "Template chứa thuộc tính HTML bị C-SEC cấm.",
        "The template contains an HTML attribute prohibited by C-SEC.",
    ),
    (
        "template_sec.c_sec_allows_only_meta_charset_or_meta_name_viewport",
        "C-SEC chỉ cho phép <meta charset> hoặc <meta name=\"viewport\">.",
        "C-SEC allows only <meta charset> or <meta name=\"viewport\">.",
    ),
    (
        "template_sec.invalid_url_list_attribute",
        "Thuộc tính URL danh sách không hợp lệ.",
        "Invalid URL list attribute.",
    ),
    (
        "template_sec.a_dynamic_url_in_an_attribute_must_be_a_single_expression",
        "URL động trong thuộc tính chỉ được phép là biểu thức {{ ... }} đơn.",
        "A dynamic URL in an attribute must be a single {{ ... }} expression.",
    ),
    (
        "template_sec.c_sec_allows_only_link_rel_stylesheet_href_theme",
        "C-SEC chỉ cho phép <link rel=\"stylesheet\" href=\"/theme/...\">.",
        "C-SEC allows only <link rel=\"stylesheet\" href=\"/theme/...\">.",
    ),
    (
        "template_sec.templates_must_not_contain_processing_instructions_or_bogus_comments",
        "Template không được chứa processing instruction hoặc bogus comment `<?`.",
        "Templates must not contain processing instructions or bogus comments `<?`.",
    ),
    (
        "template_sec.html_closing_tags_must_start_with_an_ascii_letter",
        "Thẻ đóng HTML phải bắt đầu bằng ký tự chữ cái ASCII.",
        "HTML closing tags must start with an ASCII letter.",
    ),
    (
        "template_sec.templates_allow_only_html_comments_or_doctype",
        "Template chỉ cho phép HTML comment <!-- hoặc <!DOCTYPE>.",
        "Templates allow only HTML comments <!-- or <!DOCTYPE>.",
    ),
    (
        "template_sec.a_part_must_not_contain_doctype",
        "Part không được chứa DOCTYPE.",
        "A part must not contain DOCTYPE.",
    ),
    (
        "template_sec.invalid_doctype",
        "<!DOCTYPE> không hợp lệ.",
        "Invalid <!DOCTYPE>.",
    ),
    (
        "template_sec.doctype_contains_invalid_characters",
        "<!DOCTYPE> chứa ký tự không hợp lệ.",
        "<!DOCTYPE> contains invalid characters.",
    ),
    (
        "template_sec.the_style_block_exceeds_512_kib",
        "Khối <style> vượt giới hạn 512 KiB.",
        "The <style> block exceeds 512 KiB.",
    ),
    (
        "template_sec.the_style_closing_tag_must_not_contain_whitespace_or",
        "Thẻ đóng </style> không được chứa khoảng trắng hoặc dấu /.",
        "The </style> closing tag must not contain whitespace or /.",
    ),
    (
        "template_sec.the_title_closing_tag_must_not_contain_whitespace_or",
        "Thẻ đóng </title> không được chứa khoảng trắng hoặc dấu /.",
        "The </title> closing tag must not contain whitespace or /.",
    ),
    (
        "template_sec.the_html_tag_contains_a_prohibited_nul_byte",
        "Thẻ HTML chứa byte NUL bị cấm.",
        "The HTML tag contains a prohibited NUL byte.",
    ),
    (
        "template_sec.the_html_tag_name_exceeds_32_characters",
        "Tên thẻ HTML vượt giới hạn 32 ký tự.",
        "The HTML tag name exceeds 32 characters.",
    ),
    (
        "template_sec.invalid_self_closing_slash_in_the_html_tag",
        "Dấu tự đóng `/` trên thẻ HTML không hợp lệ.",
        "Invalid self-closing slash in the HTML tag.",
    ),
    (
        "template_sec.the_html_tag_name_contains_invalid_characters",
        "Tên thẻ HTML chứa ký tự không hợp lệ.",
        "The HTML tag name contains invalid characters.",
    ),
    (
        "template_sec.html_closing_tags_must_not_contain_attributes_or",
        "Thẻ đóng HTML không được chứa thuộc tính hoặc /.",
        "HTML closing tags must not contain attributes or /.",
    ),
    (
        "template_sec.style_title_tags_must_not_be_self_closing",
        "Thẻ <style>/<title> không được tự đóng.",
        "<style>/<title> tags must not be self-closing.",
    ),
    (
        "template_sec.html_attributes_must_start_with_an_ascii_letter",
        "Thuộc tính HTML phải bắt đầu bằng chữ cái ASCII.",
        "HTML attributes must start with an ASCII letter.",
    ),
    (
        "template_sec.the_html_attribute_name_exceeds_32_characters",
        "Tên thuộc tính HTML vượt giới hạn 32 ký tự.",
        "The HTML attribute name exceeds 32 characters.",
    ),
    (
        "template_sec.the_html_attribute_name_contains_invalid_characters",
        "Tên thuộc tính HTML chứa ký tự không hợp lệ.",
        "The HTML attribute name contains invalid characters.",
    ),
    (
        "template_sec.invalid_syntax_after_the_html_attribute_name",
        "Cú pháp sau tên thuộc tính HTML không hợp lệ.",
        "Invalid syntax after the HTML attribute name.",
    ),
    (
        "template_sec.the_html_attribute_value_contains_invalid_characters",
        "Giá trị thuộc tính HTML chứa ký tự không hợp lệ.",
        "The HTML attribute value contains invalid characters.",
    ),
    (
        "template_sec.the_static_attribute_value_exceeds_4096_bytes",
        "Giá trị thuộc tính tĩnh vượt quá 4096 bytes.",
        "The static attribute value exceeds 4096 bytes.",
    ),
    (
        "template_sec.the_unquoted_html_attribute_value_contains_invalid_characters",
        "Giá trị thuộc tính HTML không ngoặc kép chứa ký tự không hợp lệ.",
        "The unquoted HTML attribute value contains invalid characters.",
    ),
    (
        "template_sec.a_slash_at_the_end_of_an_opening_tag_must_be_followed_by",
        "Sau `/` ở cuối thẻ mở phải là `>`.",
        "A slash at the end of an opening tag must be followed by >.",
    ),
    (
        "template_sec.c_sec_prohibits_assigning_attributes_elements_with_getattr_or_getitem",
        "C-SEC cấm gán thuộc tính/phần tử bằng GetAttr hoặc GetItem.",
        "C-SEC prohibits assigning attributes/elements with GetAttr or GetItem.",
    ),
    (
        "template_sec.the_template_must_end_in_the_html_body_state_all_tags_comments_style_title_closed",
        "Template phải kết thúc ở trạng thái thân HTML (đóng đủ thẻ/comment/style/title).",
        "The template must end in the HTML body state (all tags/comments/style/title closed).",
    ),
    (
        "template_sec.too_many_html_states_in_the_theme_template",
        "Quá nhiều trạng thái HTML trong theme template.",
        "Too many HTML states in the theme template.",
    ),
    (
        "template_sec.od_part_od_content_may_appear_only_in_the_html_body",
        "od_part/od_content chỉ được xuất hiện trong thân HTML.",
        "od_part/od_content may appear only in the HTML body.",
    ),
    (
        "template_sec.c_sec_prohibits_jinja_interpolation_in_comments_style_doctype_or_html_tag_boundaries",
        "C-SEC cấm nội suy Jinja ở comment, <style>, DOCTYPE hoặc biên thẻ HTML.",
        "C-SEC prohibits Jinja interpolation in comments, <style>, DOCTYPE or HTML tag boundaries.",
    ),
    (
        "template_sec.c_sec_prohibits_mixing_html_tag_and_body_contexts_at_an_interpolation_point",
        "C-SEC cấm trộn lẫn ngữ cảnh thẻ HTML và thân HTML tại điểm nội suy.",
        "C-SEC prohibits mixing HTML tag and body contexts at an interpolation point.",
    ),
    (
        "template_sec.c_sec_only_allows_interpolation_inside_double_quoted_attribute_values",
        "C-SEC chỉ cho phép nội suy bên trong giá trị thuộc tính có dấu ngoặc kép.",
        "C-SEC only allows interpolation inside double-quoted attribute values.",
    ),
    (
        "template_sec.c_sec_prohibits_interpolation_inside_security_or_event_attributes",
        "C-SEC cấm nội suy bên trong thuộc tính bảo mật hoặc sự kiện HTML.",
        "C-SEC prohibits interpolation inside security or event attributes.",
    ),
    (
        "template_sec.c_sec_prohibits_dynamic_url_interpolation_on_this_attribute_or_tag",
        "C-SEC cấm nội suy URL động trên thuộc tính hoặc thẻ này.",
        "C-SEC prohibits dynamic URL interpolation on this attribute or tag.",
    ),
    (
        "template_sec.url_attribute_groups_must_be_consistent_across_branches",
        "Nhóm thuộc tính URL phải đồng nhất qua các nhánh rẽ.",
        "URL attribute groups must be consistent across branches.",
    ),
    (
        "template_sec.the_dynamic_url_attribute_is_not_in_the_typed_url_allowlist",
        "URL động trong thuộc tính không nằm trong whitelist typed URL.",
        "The dynamic URL attribute is not in the typed URL allowlist.",
    ),
    (
        "template_sec.url_attributes_and_ordinary_attributes_cannot_be_mixed_across_branches",
        "Không thể trộn thuộc tính URL và thuộc tính thường qua các nhánh.",
        "URL attributes and ordinary attributes cannot be mixed across branches.",
    ),
    (
        "template_sec.too_many_html_states_at_a_conditional_branch",
        "Quá nhiều trạng thái HTML tại nhánh điều kiện.",
        "Too many HTML states at a conditional branch.",
    ),
    (
        "template_sec.c_sec_prohibits_reassigning_the_entry_context_variable_in_with",
        "C-SEC cấm gán lại biến ngữ cảnh `entry` trong with.",
        "C-SEC prohibits reassigning the entry context variable in with.",
    ),
    (
        "template_sec.c_sec_prohibits_reassigning_the_entry_context_variable_through_set",
        "C-SEC cấm gán lại biến ngữ cảnh `entry` qua set.",
        "C-SEC prohibits reassigning the entry context variable through set.",
    ),
    (
        "template_sec.c_sec_prohibits_reassigning_the_entry_context_variable_through_a_set_block",
        "C-SEC cấm gán lại biến ngữ cảnh `entry` qua set block.",
        "C-SEC prohibits reassigning the entry context variable through a set block.",
    ),
    (
        "template_sec.a_set_block_must_close_all_its_html_tags_comments",
        "Khối set block phải đóng đủ thẻ/comment HTML bên trong.",
        "A set block must close all its HTML tags/comments.",
    ),
    (
        "template_sec.c_sec_prohibits_reassigning_the_entry_context_variable_from_an_untrusted_source",
        "C-SEC cấm gán lại biến ngữ cảnh `entry` từ nguồn không tin cậy.",
        "C-SEC prohibits reassigning the entry context variable from an untrusted source.",
    ),
    (
        "template_sec.too_many_html_states_inside_a_loop",
        "Quá nhiều trạng thái HTML trong vòng lặp.",
        "Too many HTML states inside a loop.",
    ),
    (
        "template_sec.html_url_states_cannot_converge_inside_the_loop",
        "Không thể hội tụ trạng thái HTML/URL trong vòng lặp.",
        "HTML/URL states cannot converge inside the loop.",
    ),
    (
        "template_sec.too_many_html_states_after_a_loop",
        "Quá nhiều trạng thái HTML sau vòng lặp.",
        "Too many HTML states after a loop.",
    ),
    (
        "template_sec.unclosed_html_comment",
        "HTML comment không đóng.",
        "Unclosed HTML comment.",
    ),
    (
        "template_sec.templates_must_not_contain_processing_instructions",
        "Template không được chứa processing instruction `<?`.",
        "Templates must not contain processing instructions `<?`.",
    ),
    (
        "template_sec.template_blocks_must_not_contain_doctype",
        "Template block không được chứa DOCTYPE.",
        "Template blocks must not contain DOCTYPE.",
    ),
    (
        "template_sec.unclosed_doctype",
        "DOCTYPE không đóng.",
        "Unclosed DOCTYPE.",
    ),
    (
        "template_sec.html_allows_only_comments_or_doctype",
        "HTML chỉ cho phép comment <!-- hoặc <!DOCTYPE>.",
        "HTML allows only comments <!-- or <!DOCTYPE>.",
    ),
    (
        "template_sec.invalid_html_tag_name",
        "Tên thẻ HTML không hợp lệ.",
        "Invalid HTML tag name.",
    ),
    (
        "template_sec.the_html_tag_is_not_closed_with_valid_syntax",
        "Thẻ HTML không đóng đúng cú pháp.",
        "The HTML tag is not closed with valid syntax.",
    ),
    (
        "template_sec.template_blocks_must_not_contain_direct_style_tags_use_css_var",
        "Template block không được chứa thẻ <style> trực tiếp (dùng css_var).",
        "Template blocks must not contain direct <style> tags (use css_var).",
    ),
    (
        "template_sec.c_sec_prohibits_interpolation_inside_style",
        "C-SEC cấm nội suy {{ ... }} bên trong <style>.",
        "C-SEC prohibits {{ ... }} interpolation inside <style>.",
    ),
    (
        "template_sec.c_sec_prohibits_interpolation_inside_style_attributes",
        "C-SEC cấm nội suy {{ ... }} bên trong thuộc tính style.",
        "C-SEC prohibits {{ ... }} interpolation inside style attributes.",
    ),
    (
        "template_sec.c_sec_prohibits_interpolation_in_srcset_ping_attributes",
        "C-SEC cấm nội suy {{ ... }} trong thuộc tính srcset/ping.",
        "C-SEC prohibits {{ ... }} interpolation in srcset/ping attributes.",
    ),
    (
        "template_sec.a_dynamic_url_in_a_template_block_must_match_a_url_variable_href_or_media_key_url_src_poster",
        "URL động trong template block phải khớp đúng biến kiểu url (href) hoặc <media_key>_url (src/poster).",
        "A dynamic URL in a template block must match a url variable (href) or <media_key>_url (src/poster).",
    ),
    (
        "template_sec.invalid_srcset_attribute",
        "Thuộc tính srcset không hợp lệ.",
        "Invalid srcset attribute.",
    ),
    (
        "template_sec.invalid_ping_attribute",
        "Thuộc tính ping không hợp lệ.",
        "Invalid ping attribute.",
    ),
    (
        "template_sec.unclosed_template_expression_inside_an_html_tag",
        "Biểu thức template trong thẻ HTML không đóng.",
        "Unclosed template expression inside an HTML tag.",
    ),
    (
        "template_sec.the_html_attribute_value_is_missing_a_closing_quote",
        "Giá trị thuộc tính HTML thiếu dấu đóng ngoặc.",
        "The HTML attribute value is missing a closing quote.",
    ),
    (
        "template_sec.c_sec_prohibits_getitem_access_in_template_blocks",
        "C-SEC cấm truy cập GetItem ([...]) trong template block.",
        "C-SEC prohibits GetItem ([...]) access in template blocks.",
    ),
    (
        "template_sec.c_sec_prohibits_applying_filters_to_richtext_url_media_repeater_variables",
        "C-SEC cấm áp dụng filter lên biến richtext/url/media/repeater.",
        "C-SEC prohibits applying filters to richtext/url/media/repeater variables.",
    ),
    (
        "template_sec.c_sec_prohibits_slicing_sensitive_variables",
        "C-SEC cấm cắt lát (slice) trên biến nhạy cảm.",
        "C-SEC prohibits slicing sensitive variables.",
    ),
    (
        "template_sec.c_sec_prohibits_function_calls_in_template_blocks",
        "C-SEC cấm lời gọi hàm Call trong template block.",
        "C-SEC prohibits function calls in template blocks.",
    ),
    (
        "template_sec.a_richtext_variable_may_only_be_placed_in_the_html_body_htmlscanstate_body",
        "Biến richtext chỉ được phép đặt trong phần thân HTML (HtmlScanState::Body).",
        "A richtext variable may only be placed in the HTML body (HtmlScanState::Body).",
    ),
    (
        "template_sec.a_url_media_key_url_variable_may_only_be_emitted_inside_a_quoted_url_attribute",
        "Biến url / <media_key>_url chỉ được xuất bên trong thuộc tính URL được trích dẫn.",
        "A url / <media_key>_url variable may only be emitted inside a quoted URL attribute.",
    ),
    (
        "template_sec.raw_media_id_variables_or_repeater_objects_must_not_be_emitted_directly",
        "Không được xuất trực tiếp biến media ID thô hoặc đối tượng repeater.",
        "Raw media ID variables or repeater objects must not be emitted directly.",
    ),
    (
        "template_sec.richtext_url_media_variables_must_not_be_concatenated_or_transformed_in_output_expressions",
        "Biến richtext/url/media không được phép ghép chuỗi hoặc biến đổi trong biểu thức xuất.",
        "richtext/url/media variables must not be concatenated or transformed in output expressions.",
    ),
    (
        "template_sec.a_for_loop_variable_must_not_shadow_a_sensitive_variable_name",
        "Biến vòng lặp for không được che khuất tên biến nhạy cảm.",
        "A for loop variable must not shadow a sensitive variable name.",
    ),
    (
        "template_sec.do_not_iterate_directly_over_richtext_url_media_variables",
        "Không được lặp trực tiếp trên biến richtext/url/media.",
        "Do not iterate directly over richtext/url/media variables.",
    ),
    (
        "template_sec.a_repeater_for_loop_must_use_the_syntax",
        "Vòng lặp for trên repeater bắt buộc dùng cú pháp `{% for item in items %}`.",
        "A repeater for loop must use the syntax `{% for item in items %}`.",
    ),
    (
        "template_sec.could_not_determine_the_html_position_after_a_loop_in_the_template_block",
        "Không thể xác định vị trí HTML sau vòng lặp trong template block.",
        "Could not determine the HTML position after a loop in the template block.",
    ),
    (
        "template_sec.c_sec_prohibits_assigning_set_or_aliasing_richtext_url_media_repeater_variables",
        "C-SEC cấm gán (set) hoặc tạo bí danh cho biến richtext/url/media/repeater.",
        "C-SEC prohibits assigning (set) or aliasing richtext/url/media/repeater variables.",
    ),
    (
        "template_sec.c_sec_prohibits_overwriting_sensitive_variables_through_a_set_block",
        "C-SEC cấm ghi đè biến nhạy cảm qua khối set.",
        "C-SEC prohibits overwriting sensitive variables through a set block.",
    ),
    (
        "template_sec.c_sec_prohibits_wrapping_richtext_url_media_variables_in_a_set_block",
        "C-SEC cấm bọc biến richtext/url/media trong khối set.",
        "C-SEC prohibits wrapping richtext/url/media variables in a set block.",
    ),
    (
        "template_sec.c_sec_prohibits_aliasing_richtext_url_media_repeater_variables_through_a_with_block",
        "C-SEC cấm tạo bí danh cho biến richtext/url/media/repeater qua khối with.",
        "C-SEC prohibits aliasing richtext/url/media/repeater variables through a with block.",
    ),
    (
        "template_sec.the_template_block_uses_a_minijinja_statement_prohibited_by_c_sec",
        "Template block sử dụng câu lệnh MiniJinja bị C-SEC cấm.",
        "The template block uses a MiniJinja statement prohibited by C-SEC.",
    ),
    (
        "template_sec.the_template_block_must_not_be_empty",
        "Template block không được để trống.",
        "The template block must not be empty.",
    ),
    (
        "template_sec.the_template_block_exceeds_512_kib",
        "Template block vượt giới hạn 512 KiB.",
        "The template block exceeds 512 KiB.",
    ),
    (
        "template_sec.the_template_block_contains_a_prohibited_internal_nonce_string",
        "Template block chứa chuỗi nonce nội bộ bị cấm.",
        "The template block contains a prohibited internal nonce string.",
    ),
    (
        "template_sec.the_template_block_must_end_in_the_html_body_state_all_tags_comments_closed",
        "Template block phải kết thúc ở trạng thái thân HTML (đóng đủ thẻ/comment).",
        "The template block must end in the HTML body state (all tags/comments closed).",
    ),
    (
        "template_sec.rendered_block_html_exceeds_512_kib",
        "HTML render của block vượt quá giới hạn 512 KiB.",
        "Rendered block HTML exceeds 512 KiB.",
    ),
    (
        "theme.invalid_v3_theme_blocks_slug_path",
        "Đường dẫn blocks/<slug>/ của theme v3 không hợp lệ.",
        "Invalid v3 theme blocks/<slug>/ path.",
    ),
    (
        "theme.invalid_v3_theme_directory_name",
        "Tên thư mục theme v3 không hợp lệ.",
        "Invalid v3 theme directory name.",
    ),
    (
        "theme.invalid_v3_theme_file_name",
        "Tên file theme v3 không hợp lệ.",
        "Invalid v3 theme file name.",
    ),
    (
        "theme.the_v3_theme_assets_path_does_not_match_schema_d01",
        "Đường dẫn assets theme v3 không đúng schema D01.",
        "The v3 theme assets path does not match schema D01.",
    ),
    (
        "theme.the_v3_theme_assets_format_is_prohibited",
        "Định dạng assets theme v3 không được cho phép.",
        "The v3 theme assets format is prohibited.",
    ),
    (
        "theme.the_v3_theme_package_contains_a_file_outside_the_root_or_four_allowed_directories",
        "Gói theme v3 chứa file ngoài root hoặc bốn thư mục được phép.",
        "The v3 theme package contains a file outside the root or four allowed directories.",
    ),
    (
        "theme.invalid_block_path_in_the_package",
        "Đường dẫn block trong gói không hợp lệ.",
        "Invalid block path in the package.",
    ),
    (
        "theme.the_package_contains_a_block_path_outside_the_contract",
        "Gói chứa đường dẫn block ngoài hợp đồng.",
        "The package contains a block path outside the contract.",
    ),
    (
        "theme.each_package_may_declare_at_most_32_blocks",
        "Mỗi gói chỉ được khai báo tối đa 32 blocks.",
        "Each package may declare at most 32 blocks.",
    ),
    (
        "theme.a_package_id_declaring_blocks_must_be_kebab_case_with_at_most_32_characters",
        "Package ID khai báo block phải là kebab-case tối đa 32 ký tự.",
        "A package ID declaring blocks must be kebab-case with at most 32 characters.",
    ),
    (
        "theme.the_block_id_does_not_match_the_blocks_slug_directory",
        "ID block không khớp thư mục blocks/<slug>.",
        "The block ID does not match the blocks/<slug> directory.",
    ),
    (
        "theme.the_locale_dictionary_file_exceeds_512_keys",
        "File từ điển locale vượt quá giới hạn 512 khóa.",
        "The locale dictionary file exceeds 512 keys.",
    ),
    (
        "theme.locale_dictionary_keys_must_be_snake_case_with_at_most_64_characters",
        "Khóa trong từ điển locale phải là snake_case tối đa 64 ký tự.",
        "Locale dictionary keys must be snake_case with at most 64 characters.",
    ),
    (
        "theme.locale_dictionary_translation_strings_exceed_500_characters",
        "Chuỗi dịch trong từ điển locale vượt quá 500 ký tự.",
        "Locale dictionary translation strings exceed 500 characters.",
    ),
    (
        "theme.locale_dictionary_translation_strings_must_not_contain_control_characters",
        "Chuỗi dịch trong từ điển locale không được chứa ký tự điều khiển.",
        "Locale dictionary translation strings must not contain control characters.",
    ),
    (
        "theme.the_v3_theme_exceeds_the_file_count_limit",
        "Theme v3 vượt quá giới hạn số lượng file.",
        "The v3 theme exceeds the file count limit.",
    ),
    (
        "theme.the_v3_theme_exceeds_the_total_decoded_size_limit_of_32_mib",
        "Theme v3 vượt quá giới hạn tổng kích thước giải mã 32 MiB.",
        "The v3 theme exceeds the total decoded size limit of 32 MiB.",
    ),
    (
        "theme.invalid_declarations_format_in_theme_json",
        "Khai báo declarations trong theme.json không đúng định dạng.",
        "Invalid declarations format in theme.json.",
    ),
    (
        "theme.do_not_declare_field_groups_at_both_the_root_and_inside_declarations_in_theme_json",
        "Không được khai báo trùng field_groups ở cả cấp gốc và trong declarations của theme.json.",
        "Do not declare field_groups at both the root and inside declarations in theme.json.",
    ),
    (
        "theme.the_theme_declares_duplicate_taxonomy_keys",
        "Theme khai báo taxonomy trùng key.",
        "The theme declares duplicate taxonomy keys.",
    ),
    (
        "theme.duplicate_content_type_key_in_declarations",
        "Khóa content_type bị trùng lặp trong declarations.",
        "Duplicate content_type key in declarations.",
    ),
    (
        "theme.invalid_theme_taxonomy_eligibility_key",
        "Theme taxonomy eligibility key không hợp lệ.",
        "Invalid theme taxonomy eligibility key.",
    ),
    (
        "theme.invalid_content_type_key_in_taxonomy_eligibility",
        "Khóa content_type trong taxonomy_eligibility không hợp lệ.",
        "Invalid content_type key in taxonomy_eligibility.",
    ),
    (
        "theme.a_theme_may_declare_at_most_16_field_groups",
        "Theme chỉ được khai báo tối đa 16 field_groups.",
        "A theme may declare at most 16 field_groups.",
    ),
    (
        "theme.duplicate_panel_id_duplicate_local_key_in_the_theme_field_groups_list",
        "duplicate_panel_id: Trùng local_key trong danh sách field_groups của theme.",
        "duplicate_panel_id: Duplicate local_key in the theme field_groups list.",
    ),
    (
        "id.the_plugin_id_must_contain_3_to_64_characters",
        "ID plugin phải từ 3 đến 64 ký tự.",
        "The plugin ID must contain 3 to 64 characters.",
    ),
    (
        "id.the_package_id_must_not_use_a_reserved_system_name",
        "ID gói không được dùng tên dành riêng của hệ thống.",
        "The package ID must not use a reserved system name.",
    ),
    (
        "id.the_plugin_id_may_contain_only_lowercase_letters_digits_and_hyphens",
        "ID plugin chỉ được chứa chữ thường, số và dấu gạch ngang.",
        "The plugin ID may contain only lowercase letters, digits and hyphens.",
    ),
    (
        "id.the_plugin_id_must_not_start_or_end_with_a_hyphen",
        "ID plugin không được bắt đầu hoặc kết thúc bằng dấu gạch ngang.",
        "The plugin ID must not start or end with a hyphen.",
    ),
    (
        "id.scopedresourceid_local_key_must_be_valid_snake_case_1_64_characters",
        "local_key trong ScopedResourceId phải là snake_case hợp lệ (1..=64 ký tự).",
        "ScopedResourceId local_key must be valid snake_case (1..=64 characters).",
    ),
    (
        "id.scopedresourceid_owner_id_must_be_a_valid_package_id_3_64_characters",
        "owner_id trong ScopedResourceId phải là package_id hợp lệ (3..=64 ký tự).",
        "ScopedResourceId owner_id must be a valid package_id (3..=64 characters).",
    ),
    (
        "id.reserved_panel_namespace_the_layout_prefix_is_reserved_for_site_level_layout_configuration",
        "reserved_panel_namespace: Tiền tố 'layout:' chỉ dành riêng cho cấu hình bố cục cấp site.",
        "reserved_panel_namespace: The 'layout:' prefix is reserved for site-level layout configuration.",
    ),
    (
        "id.scopedresourceid_is_not_in_the_standard_format",
        "Định danh ScopedResourceId không đúng định dạng chuẩn.",
        "ScopedResourceId is not in the standard format.",
    ),
    (
        "theme.invalid_block_id",
        "ID block không hợp lệ.",
        "Invalid block ID.",
    ),
    (
        "theme.invalid_field_declaration",
        "Khai báo field không hợp lệ.",
        "Invalid field declaration.",
    ),
    (
        "theme.the_input_value_does_not_match_the_block_declaration",
        "Giá trị nhập không khớp khai báo block.",
        "The input value does not match the block declaration.",
    ),
    (
        "theme.invalid_template_candidate_identifier",
        "Định danh ứng viên template không hợp lệ.",
        "Invalid template candidate identifier",
    ),
    (
        "theme.invalid_single_candidate_dimensions",
        "Kích thước ứng viên single không hợp lệ.",
        "Invalid single candidate dimensions",
    ),
    (
        "theme.invalid_taxonomy_candidate_dimensions",
        "Kích thước ứng viên taxonomy không hợp lệ.",
        "Invalid taxonomy candidate dimensions",
    ),
    (
        "theme.invalid_template_candidate_dimensions",
        "Kích thước ứng viên template không hợp lệ.",
        "Invalid template candidate dimensions",
    ),
    (
        "theme.layout_and_part_declarations_use_separate_manifest_fields",
        "Khai báo layout và part dùng trường manifest riêng biệt.",
        "Layout and part declarations use separate manifest fields",
    ),
    (
        "theme.template_file_must_be_a_root_level_html_file",
        "Tệp template phải là tệp HTML ở thư mục gốc.",
        "Template file must be a root-level HTML file",
    ),
    (
        "theme.invalid_custom_template_label_or_allowed_types",
        "Nhãn custom template hoặc loại cho phép không hợp lệ.",
        "Invalid custom template label or allowed types",
    ),
    (
        "theme.custom_template_fields_are_only_valid_for_single_custom",
        "Các trường custom template chỉ hợp lệ với single_custom.",
        "Custom template fields are only valid for single_custom",
    ),
    (
        "theme.theme_manifest_version_must_be_3",
        "manifest_version của theme phải là 3.",
        "Theme manifest_version must be 3",
    ),
    (
        "theme.invalid_theme_name_or_semver_version",
        "Tên theme hoặc phiên bản SemVer không hợp lệ.",
        "Invalid theme name or SemVer version",
    ),
    (
        "theme.invalid_theme_default_locale",
        "default_locale của theme không hợp lệ.",
        "Invalid theme default_locale",
    ),
    (
        "theme.duplicate_canonical_template_target",
        "Đích template canonical bị trùng lặp.",
        "Duplicate canonical template target",
    ),
    (
        "theme.invalid_theme_part_key_or_file",
        "Khóa hoặc tệp theme part không hợp lệ.",
        "Invalid theme part key or file",
    ),
    (
        "theme.invalid_or_duplicate_page_layout",
        "Page layout không hợp lệ hoặc bị trùng.",
        "Invalid or duplicate page layout",
    ),
    (
        "theme.invalid_or_duplicate_theme_pattern",
        "Theme pattern không hợp lệ hoặc bị trùng.",
        "Invalid or duplicate theme pattern",
    ),
    (
        "theme.invalid_or_duplicate_theme_asset_path",
        "Đường dẫn asset theme không hợp lệ hoặc bị trùng.",
        "Invalid or duplicate theme asset path",
    ),
    (
        "builder_blocks.block_node_id_must_be_a_string",
        "ID node block phải là chuỗi.",
        "Block node id must be a string",
    ),
    (
        "builder_blocks.unknown_opendash_comment_block_tag",
        "Tag comment block OpenDash không xác định.",
        "Unknown OpenDash comment block tag",
    ),
    (
        "builder_blocks.missing_json_payload_in_comment_block",
        "Thiếu payload JSON trong comment block.",
        "Missing JSON payload in comment block",
    ),
    (
        "backup.unsupported_config_site_json_version",
        "Phiên bản config/site.json không được hỗ trợ.",
        "Unsupported config/site.json version.",
    ),
    (
        "backup.listen_ip_only_allows_127_0_0_1_or_1",
        "listen_ip chỉ cho phép 127.0.0.1 hoặc ::1.",
        "listen_ip only allows 127.0.0.1 or ::1.",
    ),
    (
        "backup.port_must_not_be_zero",
        "port phải khác 0.",
        "port must not be zero.",
    ),
    (
        "backup.invalid_log_level",
        "log_level không hợp lệ.",
        "Invalid log_level.",
    ),
    (
        "backup.trusted_public_origin_is_not_a_valid_url",
        "trusted_public_origin không phải URL hợp lệ.",
        "trusted_public_origin is not a valid URL.",
    ),
    (
        "backup.trusted_public_origin_must_use_http_https_and_have_a_host",
        "trusted_public_origin phải dùng http/https và có host.",
        "trusted_public_origin must use http/https and have a host.",
    ),
    (
        "backup.trusted_public_origin_must_not_contain_credentials",
        "trusted_public_origin không được chứa thông tin xác thực.",
        "trusted_public_origin must not contain credentials.",
    ),
    (
        "backup.trusted_public_origin_must_not_contain_a_path",
        "trusted_public_origin không được chứa path.",
        "trusted_public_origin must not contain a path.",
    ),
    (
        "backup.trusted_public_origin_must_not_contain_a_query_or_fragment",
        "trusted_public_origin không được chứa query hoặc fragment.",
        "trusted_public_origin must not contain a query or fragment.",
    ),
    (
        "backup.the_entry_index_exceeds_the_limit_of_20_000_payloads",
        "Chỉ số entry vượt quá giới hạn 20.000 payload.",
        "The entry index exceeds the limit of 20,000 payloads.",
    ),
    (
        "backup.the_logical_key_must_be_non_empty_ascii_and_at_most_512_bytes",
        "Logical key phải là ASCII khác rỗng và <= 512 bytes.",
        "The logical key must be non-empty ASCII and at most 512 bytes.",
    ),
    (
        "backup.the_logical_key_contains_unsafe_path_characters",
        "Logical key chứa ký tự đường dẫn không an toàn.",
        "The logical key contains unsafe path characters.",
    ),
    (
        "backup.the_logical_key_contains_invalid_path_components",
        "Logical key chứa thành phần đường dẫn không hợp lệ.",
        "The logical key contains invalid path components.",
    ),
    (
        "backup.the_logical_key_contains_reserved_names_or_control_characters",
        "Logical key chứa tên dành riêng hoặc ký tự điều khiển.",
        "The logical key contains reserved names or control characters.",
    ),
    (
        "backup.the_database_logical_key_must_be_db_main_sqlite",
        "DB logical_key phải là db/main.sqlite.",
        "The database logical_key must be db/main.sqlite.",
    ),
    (
        "backup.the_configuration_logical_key_must_be_config_site_json",
        "Config logical_key phải là config/site.json.",
        "The configuration logical_key must be config/site.json.",
    ),
    (
        "backup.invalid_original_media_logical_key",
        "Media original logical_key không hợp lệ.",
        "Invalid original media logical_key.",
    ),
    (
        "backup.invalid_media_thumbnail_logical_key",
        "Media thumbnail logical_key không hợp lệ.",
        "Invalid media thumbnail logical_key.",
    ),
    (
        "backup.the_secret_logical_key_must_be_64_hexadecimal_characters",
        "Secret logical_key phải là 64-hex.",
        "The secret logical_key must be 64 hexadecimal characters.",
    ),
    (
        "backup.invalid_quarantine_logical_key",
        "Quarantine logical_key không hợp lệ.",
        "Invalid quarantine logical_key.",
    ),
    (
        "backup.invalid_theme_logical_key",
        "Theme logical_key không hợp lệ.",
        "Invalid theme logical_key.",
    ),
    (
        "backup.invalid_plugin_logical_key",
        "Plugin logical_key không hợp lệ.",
        "Invalid plugin logical_key.",
    ),
    (
        "backup.unsupported_format_version",
        "format_version không được hỗ trợ.",
        "Unsupported format_version.",
    ),
    (
        "backup.unsupported_restore_policy_version",
        "restore_policy_version không được hỗ trợ.",
        "Unsupported restore_policy_version.",
    ),
    (
        "backup.the_manifest_file_count_exceeds_20_000",
        "Số lượng tệp trong manifest vượt quá 20.000.",
        "The manifest file count exceeds 20,000.",
    ),
    (
        "backup.invalid_entry_name_in_the_manifest",
        "entry_name trong manifest không hợp lệ.",
        "Invalid entry_name in the manifest.",
    ),
    (
        "backup.duplicate_entry_name_in_the_manifest",
        "entry_name bị trùng lặp trong manifest.",
        "Duplicate entry_name in the manifest.",
    ),
    (
        "backup.invalid_sha256_in_the_manifest",
        "sha256 trong manifest không hợp lệ.",
        "Invalid sha256 in the manifest.",
    ),
    (
        "backup.manifest_files_must_be_sorted_in_ascending_order_and_unique",
        "files trong manifest phải được sắp xếp tăng dần và không trùng.",
        "Manifest files must be sorted in ascending order and unique.",
    ),
    (
        "backup.case_conflict_in_logical_key",
        "Xung đột chữ hoa/thường trong logical_key.",
        "Case conflict in logical_key.",
    ),
    (
        "backup.the_database_size_exceeds_1_gib",
        "Kích thước DB vượt giới hạn 1 GiB.",
        "The database size exceeds 1 GiB.",
    ),
    (
        "backup.a_file_exceeds_512_mib",
        "Kích thước một tệp vượt giới hạn 512 MiB.",
        "A file exceeds 512 MiB.",
    ),
    (
        "backup.total_backup_size_exceeds_4_gib",
        "Tổng kích thước backup vượt giới hạn 4 GiB.",
        "Total backup size exceeds 4 GiB.",
    ),
    (
        "backup.the_manifest_lacks_a_db_main_sqlite_record",
        "Manifest thiếu bản ghi db/main.sqlite.",
        "The manifest lacks a db/main.sqlite record.",
    ),
    (
        "richtext.richtext_input_exceeds_64_kib",
        "Nội dung RichText vượt quá giới hạn 64 KiB đầu vào.",
        "RichText input exceeds 64 KiB.",
    ),
    (
        "richtext.richtext_must_not_contain_nul_characters",
        "Nội dung RichText không được chứa ký tự NUL.",
        "RichText must not contain NUL characters.",
    ),
    (
        "richtext.richtext_exceeds_32_kib_after_sanitization",
        "Nội dung RichText vượt quá giới hạn 32 KiB sau làm sạch.",
        "RichText exceeds 32 KiB after sanitization.",
    ),
    (
        "contact.the_form_schema_version_must_be_1",
        "Phiên bản schema form phải là 1.",
        "The form schema version must be 1.",
    ),
    (
        "contact.reply_to_field_must_reference_a_valid_email_field_in_the_form",
        "reply_to_field phải trỏ tới một trường kiểu email hợp lệ trong form.",
        "reply_to_field must reference a valid email field in the form.",
    ),
    (
        "admin_extensions.admin_extensions_only_supports_schema_version_1_and_theme_plugin_owners",
        "admin_extensions chỉ hỗ trợ schema_version 1 và owner theme/plugin.",
        "admin_extensions only supports schema_version 1 and theme/plugin owners.",
    ),
    (
        "admin_extensions.admin_extensions_exceeds_16_columns_or_16_term_panels_per_owner",
        "admin_extensions vượt giới hạn 16 cột hoặc 16 term panel/owner.",
        "admin_extensions exceeds 16 columns or 16 term panels per owner.",
    ),
    (
        "admin_extensions.admin_extensions_local_key_is_duplicated_or_conflicts_with_core_layout_namespaces",
        "admin_extensions local_key trùng hoặc chiếm namespace core/layout.",
        "admin_extensions local_key is duplicated or conflicts with core/layout namespaces.",
    ),
    (
        "admin_extensions.invalid_admin_extensions_column_label_field_key",
        "admin_extensions column label/field_key không hợp lệ.",
        "Invalid admin_extensions column label/field_key.",
    ),
    (
        "admin_extensions.admin_extensions_column_type_keys_must_contain_1_to_32_items",
        "admin_extensions column type_keys phải có từ 1 đến 32 mục.",
        "admin_extensions column type_keys must contain 1 to 32 items.",
    ),
    (
        "admin_extensions.invalid_or_duplicate_admin_extensions_type_key",
        "admin_extensions type_key không hợp lệ hoặc trùng.",
        "Invalid or duplicate admin_extensions type_key.",
    ),
    (
        "admin_extensions.only_plugins_may_reference_a_declared_action_id",
        "Chỉ plugin được tham chiếu action_id đã khai báo.",
        "Only plugins may reference a declared action_id.",
    ),
    (
        "admin_extensions.invalid_admin_extensions_term_panel_label_kind",
        "admin_extensions term panel label/kind không hợp lệ.",
        "Invalid admin_extensions term panel label/kind.",
    ),
    (
        "admin_extensions.an_admin_extensions_term_panel_requires_1_16_fields",
        "admin_extensions term panel cần 1..16 fields.",
        "An admin_extensions term panel requires 1..16 fields.",
    ),
    (
        "admin_extensions.invalid_or_duplicate_admin_extensions_term_field_key_label",
        "admin_extensions term field key/label không hợp lệ hoặc trùng.",
        "Invalid or duplicate admin_extensions term field key/label.",
    ),
    (
        "block_schema.invalid_block_field_key_snake_case_at_most_32_characters",
        "Khóa field block không hợp lệ (snake_case tối đa 32 ký tự).",
        "Invalid block field key (snake_case, at most 32 characters).",
    ),
    (
        "block_schema.a_field_key_must_not_conflict_with_a_media_field_s_generated_media_key_variables_in_the_same_scope",
        "Khóa field không được trùng biến tổng hợp <media_key>_{url,alt,width,height} của field Media cùng phạm vi.",
        "A field key must not conflict with a Media field's generated <media_key>_{url,alt,width,height} variables in the same scope.",
    ),
    (
        "block_schema.a_field_key_must_not_conflict_with_a_link_field_s_generated_link_key_variables_in_the_same_scope",
        "Khóa field không được trùng biến tổng hợp <link_key>_{url,target,rel,new_tab,nofollow} của field Link cùng phạm vi.",
        "A field key must not conflict with a Link field's generated <link_key>_{url,target,rel,new_tab,nofollow} variables in the same scope.",
    ),
    (
        "block_schema.a_field_key_must_not_conflict_with_a_spacing_field_s_generated_spacing_key_classes_variable_in_the_same_scope",
        "Khóa field không được trùng biến tổng hợp <spacing_key>_classes của field Spacing cùng phạm vi.",
        "A field key must not conflict with a Spacing field's generated <spacing_key>_classes variable in the same scope.",
    ),
    (
        "block_schema.only_url_or_link_fields_may_use_the_name_url_or_the_url_suffix",
        "Chỉ field kiểu Url hoặc Link mới được dùng tên 'url' hoặc hậu tố '_url'.",
        "Only Url or Link fields may use the name 'url' or the '_url' suffix.",
    ),
    (
        "block_schema.the_block_field_label_must_be_non_empty_and_at_most_80_characters",
        "Nhãn field block phải không rỗng và tối đa 80 ký tự.",
        "The block field label must be non-empty and at most 80 characters.",
    ),
    (
        "block_schema.the_css_variable_name_must_match_od_a_z0_9",
        "Tên biến CSS phải khớp ^--od-[a-z0-9-]{1,48}$.",
        "The CSS variable name must match ^--od-[a-z0-9-]{1,48}$.",
    ),
    (
        "block_schema.css_variables_must_not_be_duplicated_within_a_block",
        "Biến CSS không được trùng lặp trong cùng một block.",
        "CSS variables must not be duplicated within a block.",
    ),
    (
        "block_schema.text_field_max_chars_must_be_in_1_2000",
        "max_chars của field Text phải nằm trong 1..=2000.",
        "Text field max_chars must be in 1..=2000.",
    ),
    (
        "block_schema.the_default_text_value_exceeds_max_chars",
        "Giá trị mặc định Text vượt quá max_chars.",
        "The default Text value exceeds max_chars.",
    ),
    (
        "block_schema.the_placeholder_exceeds_120_characters",
        "Placeholder vượt quá 120 ký tự.",
        "The placeholder exceeds 120 characters.",
    ),
    (
        "block_schema.textarea_field_max_chars_must_be_in_1_16384",
        "max_chars của field Textarea phải nằm trong 1..=16384.",
        "Textarea field max_chars must be in 1..=16384.",
    ),
    (
        "block_schema.the_default_textarea_value_exceeds_max_chars",
        "Giá trị mặc định Textarea vượt quá max_chars.",
        "The default Textarea value exceeds max_chars.",
    ),
    (
        "block_schema.richtext_field_max_bytes_must_be_in_1_32768",
        "max_bytes của field Richtext phải nằm trong 1..=32768.",
        "Richtext field max_bytes must be in 1..=32768.",
    ),
    (
        "block_schema.the_default_richtext_value_exceeds_max_bytes_after_sanitization",
        "Giá trị mặc định Richtext vượt quá max_bytes sau làm sạch.",
        "The default Richtext value exceeds max_bytes after sanitization.",
    ),
    (
        "block_schema.fields_inside_a_repeater_do_not_support_responsive_or_css_var",
        "Field bên trong Repeater không hỗ trợ responsive hoặc css_var.",
        "Fields inside a Repeater do not support responsive or css_var.",
    ),
    (
        "block_schema.the_number_minimum_must_be_finite",
        "Giá trị min của Number phải hữu hạn.",
        "The Number minimum must be finite.",
    ),
    (
        "block_schema.the_number_maximum_must_be_finite",
        "Giá trị max của Number phải hữu hạn.",
        "The Number maximum must be finite.",
    ),
    (
        "block_schema.number_has_a_minimum_greater_than_its_maximum",
        "Number có min lớn hơn max.",
        "Number has a minimum greater than its maximum.",
    ),
    (
        "block_schema.number_step_must_be_finite_and_0",
        "step của Number phải hữu hạn và > 0.",
        "Number step must be finite and > 0.",
    ),
    (
        "block_schema.the_default_number_value_must_be_finite",
        "Giá trị mặc định Number phải hữu hạn.",
        "The default Number value must be finite.",
    ),
    (
        "block_schema.the_default_number_value_is_outside_min_max",
        "Giá trị mặc định Number nằm ngoài [min, max].",
        "The default Number value is outside [min, max].",
    ),
    (
        "block_schema.invalid_range_min_max_bounds",
        "Khoảng min/max của Range không hợp lệ.",
        "Invalid Range min/max bounds.",
    ),
    (
        "block_schema.range_step_must_be_finite_and_0",
        "step của Range phải hữu hạn và > 0.",
        "Range step must be finite and > 0.",
    ),
    (
        "block_schema.the_default_range_value_is_outside_min_max",
        "Giá trị mặc định Range nằm ngoài [min, max].",
        "The default Range value is outside [min, max].",
    ),
    (
        "block_schema.the_select_radiobuttons_option_count_is_outside_the_valid_range",
        "Số lượng lựa chọn của Select/RadioButtons vượt giới hạn hợp lệ.",
        "The Select/RadioButtons option count is outside the valid range.",
    ),
    (
        "block_schema.select_option_values_must_be_non_empty_and_64_characters",
        "Giá trị option Select phải không rỗng và <= 64 ký tự.",
        "Select option values must be non-empty and <= 64 characters.",
    ),
    (
        "block_schema.select_option_values_must_be_unique",
        "Giá trị option Select không được trùng lặp.",
        "Select option values must be unique.",
    ),
    (
        "block_schema.option_icon_names_may_contain_only_a_z0_9_1_32_characters",
        "Tên icon option chỉ được gồm [a-z0-9-] (1..=32 ký tự).",
        "Option icon names may contain only [a-z0-9-] (1..=32 characters).",
    ),
    (
        "block_schema.select_options_with_css_var_must_match_a_z0_9",
        "Option Select có css_var phải khớp ^[a-z0-9_-]{1,32}$.",
        "Select options with css_var must match ^[a-z0-9_-]{1,32}$.",
    ),
    (
        "block_schema.the_default_select_value_is_not_in_the_options_list",
        "Giá trị mặc định Select không có trong danh sách options.",
        "The default Select value is not in the options list.",
    ),
    (
        "block_schema.invalid_spacing_min_max_bounds",
        "Khoảng min/max của Spacing không hợp lệ.",
        "Invalid Spacing min/max bounds.",
    ),
    (
        "block_schema.the_default_spacing_value_is_outside_min_max",
        "Giá trị mặc định Spacing nằm ngoài [min, max].",
        "The default Spacing value is outside [min, max].",
    ),
    (
        "block_schema.fields_inside_a_repeater_do_not_support_css_var",
        "Field bên trong Repeater không hỗ trợ css_var.",
        "Fields inside a Repeater do not support css_var.",
    ),
    (
        "block_schema.invalid_default_color",
        "Màu mặc định không hợp lệ.",
        "Invalid default color.",
    ),
    (
        "block_schema.a_media_field_with_css_var_must_have_media_kind_image",
        "Field Media gắn css_var bắt buộc có media_kind: \"image\".",
        "A Media field with css_var must have media_kind: \"image\".",
    ),
    (
        "block_schema.the_default_media_id_must_be_a_64_character_hexadecimal_sha_256_string",
        "Media ID mặc định phải là chuỗi SHA-256 64 ký tự hex.",
        "The default Media ID must be a 64-character hexadecimal SHA-256 string.",
    ),
    (
        "block_schema.invalid_default_block_url",
        "URL mặc định của block không hợp lệ.",
        "Invalid default block URL.",
    ),
    (
        "block_schema.the_url_placeholder_exceeds_120_characters",
        "Placeholder Url vượt quá 120 ký tự.",
        "The Url placeholder exceeds 120 characters.",
    ),
    (
        "block_schema.invalid_default_url_in_the_link_field",
        "URL mặc định trong field Link không hợp lệ.",
        "Invalid default URL in the Link field.",
    ),
    (
        "block_schema.the_link_placeholder_exceeds_120_characters",
        "Placeholder Link vượt quá 120 ký tự.",
        "The Link placeholder exceeds 120 characters.",
    ),
    (
        "block_schema.group_repeater_cannot_be_nested_here",
        "Không thể lồng Group/Repeater tại đây.",
        "Group/Repeater cannot be nested here.",
    ),
    (
        "block_schema.block_json_only_supports_schema_version_1",
        "block.json chỉ hỗ trợ schema_version == 1.",
        "block.json only supports schema_version == 1.",
    ),
    (
        "block_schema.block_json_must_declare_render_render_html",
        "block.json bắt buộc khai báo render == \"render.html\".",
        "block.json must declare render == \"render.html\".",
    ),
    (
        "block_schema.the_block_title_must_be_non_empty_and_80_characters",
        "Tiêu đề block phải không rỗng và <= 80 ký tự.",
        "The block title must be non-empty and <= 80 characters.",
    ),
    (
        "block_schema.block_icon_names_may_contain_only_a_z0_9_1_32_characters",
        "Tên icon block chỉ được gồm [a-z0-9-] (1..=32 ký tự).",
        "Block icon names may contain only [a-z0-9-] (1..=32 characters).",
    ),
    (
        "block_schema.the_block_id_must_have_a_valid_owner_slug_format",
        "ID block phải có dạng <owner>/<slug> hợp lệ.",
        "The block ID must have a valid <owner>/<slug> format.",
    ),
    (
        "block_schema.the_block_id_is_missing_the_owner_prefix",
        "ID block thiếu tiền tố owner.",
        "The block ID is missing the owner prefix.",
    ),
    (
        "block_schema.the_block_id_does_not_match_the_owning_package_s_prefix",
        "ID block không khớp tiền tố owner của gói sở hữu.",
        "The block ID does not match the owning package's prefix.",
    ),
    (
        "block_schema.allowed_parents_allowed_children_must_have_1_32_items_when_declared",
        "allowed_parents / allowed_children phải có 1..=32 phần tử khi được khai báo.",
        "allowed_parents / allowed_children must have 1..=32 items when declared.",
    ),
    (
        "block_schema.an_allowed_parents_allowed_children_item_contains_invalid_characters",
        "Phần tử trong allowed_parents / allowed_children chứa ký tự không hợp lệ.",
        "An allowed_parents / allowed_children item contains invalid characters.",
    ),
    (
        "block_schema.invalid_group_key",
        "Khóa Group không hợp lệ.",
        "Invalid Group key.",
    ),
    (
        "block_schema.group_keys_must_not_use_the_name_url_or_the_url_suffix",
        "Khóa Group không được dùng tên 'url' hoặc hậu tố '_url'.",
        "Group keys must not use the name 'url' or the '_url' suffix.",
    ),
    (
        "block_schema.duplicate_field_key_in_the_block",
        "Khóa field trùng lặp trong block.",
        "Duplicate field key in the block.",
    ),
    (
        "block_schema.group_must_not_be_empty",
        "Group không được rỗng.",
        "Group must not be empty.",
    ),
    (
        "block_schema.nesting_group_inside_group_is_not_allowed",
        "Không cho phép lồng Group trong Group.",
        "Nesting Group inside Group is not allowed.",
    ),
    (
        "block_schema.the_block_exceeds_the_maximum_of_32_actual_top_level_fields",
        "Block vượt quá giới hạn tối đa 32 field thực top-level.",
        "The block exceeds the maximum of 32 actual top-level fields.",
    ),
    (
        "block_schema.group_keys_must_not_conflict_with_generated_media_key_variables",
        "Khóa Group không được trùng biến tổng hợp <media_key>_{url,alt,width,height}.",
        "Group keys must not conflict with generated <media_key>_{url,alt,width,height} variables.",
    ),
    (
        "block_schema.group_keys_must_not_conflict_with_generated_link_key_variables",
        "Khóa Group không được trùng biến tổng hợp <link_key>_{url,target,rel,new_tab,nofollow}.",
        "Group keys must not conflict with generated <link_key>_{url,target,rel,new_tab,nofollow} variables.",
    ),
    (
        "block_schema.group_keys_must_not_conflict_with_the_generated_spacing_key_classes_variable",
        "Khóa Group không được trùng biến tổng hợp <spacing_key>_classes.",
        "Group keys must not conflict with the generated <spacing_key>_classes variable.",
    ),
    (
        "block_schema.repeater_item_counts_must_satisfy_1_max_items_32_and_min_max",
        "Số phần tử Repeater phải thỏa 1 <= max_items <= 32 và min <= max.",
        "Repeater item counts must satisfy 1 <= max_items <= 32 and min <= max.",
    ),
    (
        "block_schema.the_total_max_items_of_repeaters_in_the_block_exceeds_64",
        "Tổng max_items của các Repeater trong block vượt quá 64.",
        "The total max_items of Repeaters in the block exceeds 64.",
    ),
    (
        "block_schema.a_repeater_must_contain_at_least_one_sub_field",
        "Repeater bắt buộc có ít nhất 1 sub-field.",
        "A Repeater must contain at least one sub-field.",
    ),
    (
        "block_schema.a_repeater_may_contain_only_primitive_field_types",
        "Repeater chỉ được chứa các kiểu field nguyên thủy.",
        "A Repeater may contain only primitive field types.",
    ),
    (
        "block_schema.duplicate_sub_field_key_in_the_repeater",
        "Khóa sub-field bị trùng trong Repeater.",
        "Duplicate sub-field key in the Repeater.",
    ),
    (
        "block_schema.item_label_field_does_not_exist_in_repeater_fields",
        "item_label_field không tồn tại trong Repeater.fields.",
        "item_label_field does not exist in Repeater.fields.",
    ),
    (
        "block_schema.item_label_field_may_only_reference_text_select_radiobuttons_number_url",
        "item_label_field chỉ được trỏ tới Text/Select/RadioButtons/Number/Url.",
        "item_label_field may only reference Text/Select/RadioButtons/Number/Url.",
    ),
    (
        "block_schema.the_total_number_of_leaf_fields_in_the_block_including_repeaters_exceeds_48",
        "Tổng số field lá của block (kể cả trong Repeater) vượt quá 48.",
        "The total number of leaf fields in the block (including Repeaters) exceeds 48.",
    ),
    (
        "block_schema.the_block_props_values_exceeds_the_limit_of_64_keys",
        "Số lượng khóa props.values của block vượt quá giới hạn 64.",
        "The block props.values exceeds the limit of 64 keys.",
    ),
    (
        "block_schema.the_block_props_values_data_exceeds_64_kib",
        "Kích thước dữ liệu props.values của block vượt quá 64 KiB.",
        "The block props.values data exceeds 64 KiB.",
    ),
    (
        "block_schema.show_if_must_not_reference_itself",
        "show_if không được tự tham chiếu chính mình.",
        "show_if must not reference itself.",
    ),
    (
        "block_schema.show_if_must_reference_an_existing_top_level_field",
        "show_if phải trỏ tới field top-level tồn tại.",
        "show_if must reference an existing top-level field.",
    ),
    (
        "block_schema.show_if_must_not_reference_group_repeater_spacing_or_link",
        "show_if không được trỏ tới Group, Repeater, Spacing hoặc Link.",
        "show_if must not reference Group, Repeater, Spacing or Link.",
    ),
    (
        "block_schema.show_if_must_not_reference_a_field_with_responsive_true",
        "show_if không được trỏ tới field có responsive: true.",
        "show_if must not reference a field with responsive: true.",
    ),
    (
        "block_schema.the_repeater_has_fewer_than_min_items_items",
        "Repeater chưa đạt số phần tử tối thiểu min_items.",
        "The Repeater has fewer than min_items items.",
    ),
    (
        "block_schema.the_repeater_exceeds_max_items_items",
        "Repeater vượt quá số phần tử tối đa max_items.",
        "The Repeater exceeds max_items items.",
    ),
    (
        "block_schema.each_repeater_item_must_be_a_json_object",
        "Mỗi phần tử trong Repeater phải là JSON object.",
        "Each Repeater item must be a JSON object.",
    ),
    (
        "block_schema.a_repeater_item_exceeds_64_keys",
        "Phần tử Repeater vượt quá 64 khóa.",
        "A Repeater item exceeds 64 keys.",
    ),
    (
        "block_schema.the_repeater_value_must_be_a_json_array",
        "Giá trị Repeater phải là mảng JSON.",
        "The Repeater value must be a JSON array.",
    ),
    (
        "block_schema.missing_value_for_a_required_field",
        "Thiếu giá trị cho field bắt buộc.",
        "Missing value for a required field.",
    ),
    (
        "block_schema.missing_value_for_a_required_responsive_field",
        "Thiếu giá trị cho field responsive bắt buộc.",
        "Missing value for a required responsive field.",
    ),
    (
        "block_schema.invalid_responsive_breakpoint_key",
        "Khóa breakpoint responsive không hợp lệ.",
        "Invalid responsive breakpoint key.",
    ),
    (
        "block_schema.a_required_responsive_field_must_have_at_least_one_breakpoint_value",
        "Field responsive bắt buộc phải có ít nhất một giá trị breakpoint.",
        "A required responsive field must have at least one breakpoint value.",
    ),
    (
        "block_schema.the_text_value_exceeds_the_character_limit",
        "Giá trị Text vượt quá giới hạn ký tự.",
        "The Text value exceeds the character limit.",
    ),
    (
        "block_schema.a_required_text_field_must_not_be_empty",
        "Field Text bắt buộc không được để trống.",
        "A required Text field must not be empty.",
    ),
    (
        "block_schema.the_textarea_value_exceeds_the_character_limit",
        "Giá trị Textarea vượt quá giới hạn ký tự.",
        "The Textarea value exceeds the character limit.",
    ),
    (
        "block_schema.a_required_textarea_field_must_not_be_empty",
        "Field Textarea bắt buộc không được để trống.",
        "A required Textarea field must not be empty.",
    ),
    (
        "block_schema.the_richtext_value_exceeds_max_bytes_after_sanitization",
        "Giá trị Richtext vượt quá max_bytes sau làm sạch.",
        "The Richtext value exceeds max_bytes after sanitization.",
    ),
    (
        "block_schema.a_required_richtext_field_must_not_be_empty",
        "Field Richtext bắt buộc không được để trống.",
        "A required Richtext field must not be empty.",
    ),
    (
        "block_schema.the_number_value_is_invalid_or_outside_min_max",
        "Giá trị Number không hợp lệ hoặc nằm ngoài [min, max].",
        "The Number value is invalid or outside [min, max].",
    ),
    (
        "block_schema.the_range_value_is_outside_min_max",
        "Giá trị Range nằm ngoài [min, max].",
        "The Range value is outside [min, max].",
    ),
    (
        "block_schema.the_boolean_value_must_be_true_or_false",
        "Giá trị Boolean phải là true hoặc false.",
        "The Boolean value must be true or false.",
    ),
    (
        "block_schema.the_select_radiobuttons_value_is_not_in_the_options_list",
        "Giá trị Select/RadioButtons không nằm trong danh sách options.",
        "The Select/RadioButtons value is not in the options list.",
    ),
    (
        "block_schema.invalid_spacing_side_key",
        "Khóa cạnh Spacing không hợp lệ.",
        "Invalid Spacing side key.",
    ),
    (
        "block_schema.the_spacing_side_value_is_outside_min_max",
        "Giá trị cạnh Spacing nằm ngoài [min, max].",
        "The Spacing side value is outside [min, max].",
    ),
    (
        "block_schema.a_required_color_field_must_not_be_empty",
        "Field Color bắt buộc không được để trống.",
        "A required Color field must not be empty.",
    ),
    (
        "block_schema.invalid_color_value",
        "Giá trị Color không hợp lệ.",
        "Invalid Color value.",
    ),
    (
        "block_schema.a_required_media_field_must_not_be_empty",
        "Field Media bắt buộc không được để trống.",
        "A required Media field must not be empty.",
    ),
    (
        "block_schema.a_block_media_id_must_be_a_64_character_hexadecimal_sha_256_string",
        "Media ID trong block phải là SHA-256 64 ký tự hex.",
        "A block Media ID must be a 64-character hexadecimal SHA-256 string.",
    ),
    (
        "block_schema.a_required_url_field_must_not_be_empty",
        "Field Url bắt buộc không được để trống.",
        "A required Url field must not be empty.",
    ),
    (
        "block_schema.the_block_url_is_unsafe_or_malformed",
        "URL trong block không an toàn hoặc sai cú pháp.",
        "The block URL is unsafe or malformed.",
    ),
    (
        "block_schema.link_url_must_be_a_string",
        "Thuộc tính Link.url phải là chuỗi.",
        "Link.url must be a string.",
    ),
    (
        "block_schema.link_properties_must_be_boolean",
        "Thuộc tính Link.{new_tab,nofollow} phải là boolean.",
        "Link.{new_tab,nofollow} properties must be boolean.",
    ),
    (
        "block_schema.invalid_key_in_the_link_object",
        "Khóa không hợp lệ trong object Link.",
        "Invalid key in the Link object.",
    ),
    (
        "block_schema.the_link_value_must_be_a_url_string_or_object",
        "Giá trị Link phải là chuỗi URL hoặc object {url, new_tab, nofollow}.",
        "The Link value must be a URL string or object {url, new_tab, nofollow}.",
    ),
    (
        "block_schema.a_required_link_field_must_not_have_an_empty_url",
        "Field Link bắt buộc không được để trống URL.",
        "A required Link field must not have an empty URL.",
    ),
    (
        "block_schema.the_link_field_url_is_unsafe_or_malformed",
        "URL trong field Link không an toàn hoặc sai cú pháp.",
        "The Link field URL is unsafe or malformed.",
    ),
    (
        "block_schema.invalid_nested_field_structure",
        "Cấu trúc field lồng không hợp lệ.",
        "Invalid nested field structure.",
    ),
    (
        "plugin.scopedfieldgrant_field_keys_must_contain_1_to_64_field_keys",
        "ScopedFieldGrant.field_keys phải có từ 1 đến 64 khóa trường.",
        "ScopedFieldGrant.field_keys must contain 1 to 64 field keys.",
    ),
    (
        "plugin.scopedfieldgrant_field_keys_does_not_allow_the_wildcard_keys_must_be_valid_snake_case_and_unique",
        "ScopedFieldGrant.field_keys không cho phép wildcard '*', phải là snake_case hợp lệ và không trùng lặp.",
        "ScopedFieldGrant.field_keys does not allow the '*' wildcard; keys must be valid snake_case and unique.",
    ),
    (
        "plugin.admin_extensions_requires_manifest_schema_2",
        "admin_extensions yêu cầu manifest_schema >= 2.",
        "admin_extensions requires manifest_schema >= 2.",
    ),
    (
        "plugin.invalid_declarations_field_in_plugin_json",
        "Trường declarations trong plugin.json không hợp lệ.",
        "Invalid declarations field in plugin.json.",
    ),
    (
        "plugin.do_not_declare_field_groups_at_both_the_root_and_inside_declarations",
        "Không được khai báo trùng field_groups ở cả cấp gốc và trong declarations.",
        "Do not declare field_groups at both the root and inside declarations.",
    ),
    (
        "plugin.only_api_version_1_is_supported",
        "Chỉ hỗ trợ api_version 1.",
        "Only api_version 1 is supported.",
    ),
    (
        "plugin.unsupported_manifest_schema_only_1_2_or_3_are_supported",
        "manifest_schema không được hỗ trợ (chỉ hỗ trợ 1, 2 hoặc 3).",
        "Unsupported manifest_schema (only 1, 2 or 3 are supported).",
    ),
    (
        "plugin.field_groups_in_plugin_json_is_only_valid_when_manifest_schema_2",
        "field_groups trong plugin.json chỉ hợp lệ khi manifest_schema >= 2.",
        "field_groups in plugin.json is only valid when manifest_schema >= 2.",
    ),
    (
        "plugin.editor_panels_in_plugin_json_is_only_valid_when_manifest_schema_2",
        "editor_panels trong plugin.json chỉ hợp lệ khi manifest_schema >= 2.",
        "editor_panels in plugin.json is only valid when manifest_schema >= 2.",
    ),
    (
        "plugin.plugin_json_must_not_declare_more_than_16_editor_panels",
        "plugin.json không được khai báo quá 16 editor_panels.",
        "plugin.json must not declare more than 16 editor_panels.",
    ),
    (
        "plugin.content_read_private_and_content_write_private_are_only_valid_when_manifest_schema_2_and_min_host_version_is_declared",
        "content_read_private và content_write_private chỉ hợp lệ khi manifest_schema >= 2 và có khai báo min_host_version.",
        "content_read_private and content_write_private are only valid when manifest_schema >= 2 and min_host_version is declared.",
    ),
    (
        "plugin.extended_v3_features_filters_settings_schema_v3_permissions_v3_lifecycle_hooks_require_manifest_schema_3_and_min_host_version",
        "Tính năng mở rộng v3 (filters, settings_schema, quyền v3, lifecycle hooks v3) yêu cầu manifest_schema >= 3 và khai báo min_host_version.",
        "Extended v3 features (filters, settings_schema, v3 permissions, v3 lifecycle hooks) require manifest_schema >= 3 and min_host_version.",
    ),
    (
        "plugin.min_host_version_is_not_valid_semver_2_0_0",
        "min_host_version không đúng định dạng SemVer 2.0.0.",
        "min_host_version is not valid SemVer 2.0.0.",
    ),
    (
        "plugin.the_current_host_version_is_below_the_required_min_host_version",
        "Phiên bản host hiện tại thấp hơn min_host_version yêu cầu.",
        "The current host version is below the required min_host_version.",
    ),
    (
        "plugin.the_plugin_version_is_not_valid_semver_2_0_0",
        "Phiên bản plugin không đúng định dạng SemVer 2.0.0.",
        "The plugin version is not valid SemVer 2.0.0.",
    ),
    (
        "plugin.invalid_plugin_name",
        "Tên plugin không hợp lệ.",
        "Invalid plugin name.",
    ),
    (
        "plugin.the_plugin_description_is_too_long",
        "Mô tả plugin quá dài.",
        "The plugin description is too long.",
    ),
    (
        "plugin.entrypoint_must_be_plugin_wasm",
        "Entrypoint phải là 'plugin.wasm'.",
        "Entrypoint must be 'plugin.wasm'.",
    ),
    (
        "plugin.an_event_in_the_hooks_list_is_not_supported",
        "Sự kiện hook không được hỗ trợ trong danh sách hooks.",
        "An event in the hooks list is not supported.",
    ),
    (
        "plugin.the_hooks_list_must_not_contain_duplicate_events",
        "Danh sách hooks không được chứa sự kiện trùng lặp.",
        "The hooks list must not contain duplicate events.",
    ),
    (
        "plugin.only_filter_events_can_be_declared_in_the_filters_list",
        "Chỉ các filter:* mới được khai báo trong danh sách filters.",
        "Only filter:* events can be declared in the filters list.",
    ),
    (
        "plugin.the_filters_list_must_not_contain_duplicate_events",
        "Danh sách filters không được chứa sự kiện trùng lặp.",
        "The filters list must not contain duplicate events.",
    ),
    (
        "plugin.settings_schema_must_not_exceed_64_fields",
        "settings_schema không được vượt quá 64 trường.",
        "settings_schema must not exceed 64 fields.",
    ),
    (
        "plugin.field_keys_in_settings_schema_must_be_snake_case_and_unique",
        "Khóa trường trong settings_schema phải là snake_case và không trùng lặp.",
        "Field keys in settings_schema must be snake_case and unique.",
    ),
    (
        "plugin.invalid_field_label_in_settings_schema",
        "Nhãn trường trong settings_schema không hợp lệ.",
        "Invalid field label in settings_schema.",
    ),
    (
        "plugin.secret_settings_fields_must_not_have_a_non_null_default",
        "Trường settings kiểu secret không được đặt giá trị mặc định khác null.",
        "Secret settings fields must not have a non-null default.",
    ),
    (
        "plugin.invalid_settings_schema_min",
        "settings_schema min không hợp lệ.",
        "Invalid settings_schema min.",
    ),
    (
        "plugin.invalid_settings_schema_max",
        "settings_schema max không hợp lệ.",
        "Invalid settings_schema max.",
    ),
    (
        "plugin.settings_schema_requires_min_max",
        "settings_schema yêu cầu min <= max.",
        "settings_schema requires min <= max.",
    ),
    (
        "plugin.a_select_field_in_settings_schema_must_have_1_64_options",
        "Trường select trong settings_schema phải có 1..=64 lựa chọn.",
        "A select field in settings_schema must have 1..=64 options.",
    ),
    (
        "plugin.invalid_option_in_a_settings_schema_select_field",
        "Lựa chọn của trường select trong settings_schema không hợp lệ.",
        "Invalid option in a settings_schema select field.",
    ),
    (
        "plugin.invalid_content_read_permission",
        "Quyền content_read không hợp lệ.",
        "Invalid content_read permission.",
    ),
    (
        "plugin.invalid_content_write_permission",
        "Quyền content_write không hợp lệ.",
        "Invalid content_write permission.",
    ),
    (
        "plugin.invalid_content_delete_permission",
        "Quyền content_delete không hợp lệ.",
        "Invalid content_delete permission.",
    ),
    (
        "plugin.plugin_secrets_read_must_list_valid_secret_keys_from_settings_schema_the_wildcard_is_prohibited",
        "plugin_secrets_read phải liệt kê các khóa secret hợp lệ tồn tại trong settings_schema (cấm wildcard '*').",
        "plugin_secrets_read must list valid secret keys from settings_schema (the '*' wildcard is prohibited).",
    ),
    (
        "plugin.invalid_sqlite_permission_only_kv_read_and_kv_write_are_supported",
        "Quyền SQLite không hợp lệ (chỉ hỗ trợ 'kv_read' và 'kv_write').",
        "Invalid SQLite permission (only 'kv_read' and 'kv_write' are supported).",
    ),
    (
        "plugin.network_scope_scheme_only_supports_http_or_https",
        "Network scope scheme chỉ hỗ trợ http hoặc https.",
        "Network scope scheme only supports http or https.",
    ),
    (
        "plugin.invalid_network_scope_host",
        "Network scope host không hợp lệ.",
        "Invalid network scope host.",
    ),
    (
        "plugin.network_scope_port_must_be_greater_than_0",
        "Network scope port phải lớn hơn 0.",
        "Network scope port must be greater than 0.",
    ),
    (
        "plugin.network_scope_methods_must_not_be_empty",
        "Network scope methods không được rỗng.",
        "Network scope methods must not be empty.",
    ),
    (
        "plugin.unsupported_network_scope_method",
        "Network scope method không được hỗ trợ.",
        "Unsupported network scope method.",
    ),
    (
        "plugin.invalid_network_scope_path_prefix",
        "Network scope path_prefix không hợp lệ.",
        "Invalid network scope path_prefix.",
    ),
    (
        "plugin.invalid_action_id_in_the_manifest",
        "Action id trong manifest không hợp lệ.",
        "Invalid action ID in the manifest.",
    ),
    (
        "plugin.duplicate_action_id_in_the_manifest",
        "Action id trong manifest bị trùng lặp.",
        "Duplicate action ID in the manifest.",
    ),
    (
        "plugin.action_required_role_only_supports_admin_or_editor",
        "Action required_role chỉ hỗ trợ 'admin' hoặc 'editor'.",
        "Action required_role only supports 'admin' or 'editor'.",
    ),
    (
        "plugin.action_capability_is_not_in_the_allowlist",
        "Action capability không nằm trong allowlist.",
        "Action capability is not in the allowlist.",
    ),
    (
        "plugin.an_action_must_not_combine_kv_write_kv_delete_content_write_or_content_delete",
        "Action không được kết hợp kv_write/kv_delete, content_write hoặc content_delete trong cùng một action.",
        "An action must not combine kv_write/kv_delete, content_write or content_delete.",
    ),
    (
        "plugin.an_action_declaring_content_read_private_must_include_content_read_and_permissions_content_read_private",
        "Action khai báo content_read_private phải kèm content_read và permissions.content_read_private.",
        "An action declaring content_read_private must include content_read and permissions.content_read_private.",
    ),
    (
        "plugin.an_action_declaring_content_write_private_must_include_content_write_and_permissions_content_write_private",
        "Action khai báo content_write_private phải kèm content_write và permissions.content_write_private.",
        "An action declaring content_write_private must include content_write and permissions.content_write_private.",
    ),
    (
        "plugin.local_key_in_editor_panels_is_duplicated_or_conflicts_with_field_groups",
        "local_key trong editor_panels bị trùng lặp hoặc trùng với field_groups.",
        "local_key in editor_panels is duplicated or conflicts with field_groups.",
    ),
    (
        "plugin.editor_panels_references_an_action_id_that_does_not_exist_in_actions",
        "editor_panels tham chiếu action_id không tồn tại trong actions.",
        "editor_panels references an action_id that does not exist in actions.",
    ),
    (
        "plugin.editor_panels_has_a_min_edit_role_below_the_action_s_required_role",
        "editor_panels có min_edit_role thấp hơn required_role của action.",
        "editor_panels has a min_edit_role below the action's required_role.",
    ),
    (
        "builder.unknown_or_malformed_builder_element",
        "Phần tử Builder không xác định hoặc sai định dạng.",
        "Unknown or malformed Builder element",
    ),
    (
        "builder.invalid_heading_level_or_text",
        "Cấp Heading hoặc nội dung không hợp lệ.",
        "Invalid Heading level or text",
    ),
    (
        "builder.invalid_text_content",
        "Nội dung Text không hợp lệ.",
        "Invalid Text content",
    ),
    (
        "builder.invalid_image_media_id_or_alt_text",
        "ID media hoặc văn bản alt của Image không hợp lệ.",
        "Invalid Image media ID or alt text",
    ),
    (
        "builder.invalid_button_label_or_url",
        "Nhãn hoặc URL của Button không hợp lệ.",
        "Invalid Button label or URL",
    ),
    (
        "builder.invalid_legacy_block_id",
        "ID block cũ không hợp lệ.",
        "Invalid legacy block ID",
    ),
    (
        "builder.invalid_plugin_block_id",
        "ID block plugin không hợp lệ.",
        "Invalid plugin block id",
    ),
    (
        "builder.plugin_block_props_exceed_limit",
        "Props block plugin vượt giới hạn.",
        "Plugin block props exceed limit",
    ),
    (
        "builder.plugin_block_provider_is_not_active",
        "Provider block plugin không hoạt động.",
        "Plugin block provider is not active",
    ),
    (
        "builder.plugin_block_props_exceed_64_kib",
        "Props block plugin vượt quá 64 KiB.",
        "Plugin block props exceed 64 KiB",
    ),
    (
        "builder.invalid_video_media_id",
        "ID media Video không hợp lệ.",
        "Invalid Video media ID",
    ),
    (
        "builder.invalid_video_poster_image_id",
        "ID ảnh poster Video không hợp lệ.",
        "Invalid Video poster image ID",
    ),
    (
        "builder.invalid_builder_contact_form_configuration",
        "Cấu hình biểu mẫu liên hệ Builder không hợp lệ.",
        "Invalid Builder contact form configuration",
    ),
    (
        "builder.invalid_pattern_reference_id",
        "ID tham chiếu Pattern không hợp lệ.",
        "Invalid Pattern reference ID",
    ),
    (
        "builder.invalid_builder_template_part_key",
        "Khóa template part Builder không hợp lệ.",
        "Invalid Builder template part key",
    ),
    (
        "builder.invalid_builder_list_configuration",
        "Cấu hình danh sách Builder không hợp lệ.",
        "Invalid Builder list configuration",
    ),
    (
        "content.invalid_field_acl_min_edit_role_must_not_be_below_min_view_role",
        "invalid_field_acl: min_edit_role không được thấp hơn min_view_role.",
        "invalid_field_acl: min_edit_role must not be below min_view_role.",
    ),
    (
        "content.invalid_field_acl_a_field_with_min_view_role_admin_must_not_have_public_exposure_public",
        "invalid_field_acl: Trường có min_view_role = admin không được đặt public_exposure = public.",
        "invalid_field_acl: A field with min_view_role = admin must not have public_exposure = public.",
    ),
    (
        "content.field_key_collision_the_custom_field_key_conflicts_with_a_protected_system_field",
        "field_key_collision: Khóa custom field trùng với trường hệ thống được bảo vệ.",
        "field_key_collision: The custom field key conflicts with a protected system field.",
    ),
    (
        "content.invalid_custom_field_key",
        "Khóa custom field không hợp lệ.",
        "Invalid custom field key.",
    ),
    (
        "content.invalid_custom_field_name",
        "Tên custom field không hợp lệ.",
        "Invalid custom field name.",
    ),
    (
        "content.only_choice_fields_may_declare_options",
        "Chỉ field lựa chọn mới được khai báo options.",
        "Only choice fields may declare options.",
    ),
    (
        "content.a_choice_field_requires_1_to_100_options",
        "Field lựa chọn cần từ 1 đến 100 phương án.",
        "A choice field requires 1 to 100 options.",
    ),
    (
        "content.an_option_is_invalid_or_has_a_duplicate_value",
        "Phương án lựa chọn không hợp lệ hoặc trùng giá trị.",
        "An option is invalid or has a duplicate value.",
    ),
    (
        "content.invalid_field_default_default_value_is_invalid_or_prohibited_for_this_field_type",
        "invalid_field_default: Giá trị default_value không hợp lệ hoặc bị cấm cho kiểu trường này.",
        "invalid_field_default: default_value is invalid or prohibited for this field type.",
    ),
    (
        "content.admin_required_field_missing_default_a_required_field_with_min_edit_role_admin_must_declare_a_valid_default_value",
        "admin_required_field_missing_default: Trường bắt buộc có min_edit_role = admin phải khai báo default_value hợp lệ.",
        "admin_required_field_missing_default: A required field with min_edit_role = admin must declare a valid default_value.",
    ),
    (
        "content.supports_must_not_contain_duplicate_features",
        "supports không được chứa tính năng trùng lặp.",
        "supports must not contain duplicate features.",
    ),
    (
        "content.invalid_content_type_name",
        "Tên loại nội dung không hợp lệ.",
        "Invalid content type name.",
    ),
    (
        "content.each_content_type_supports_at_most_64_custom_fields",
        "Mỗi loại nội dung hỗ trợ tối đa 64 custom fields.",
        "Each content type supports at most 64 custom fields.",
    ),
    (
        "content.field_key_collision_duplicate_custom_field_key_in_the_content_type",
        "field_key_collision: Trùng khóa custom field trong loại nội dung.",
        "field_key_collision: Duplicate custom field key in the content type.",
    ),
    (
        "content.section_is_a_core_key_superseded_by_pattern",
        "Section là khóa Core đã được thay thế bằng Pattern.",
        "Section is a Core key superseded by Pattern.",
    ),
    (
        "content.supports_is_not_normalized",
        "supports chưa được chuẩn hóa.",
        "supports is not normalized.",
    ),
    (
        "content.post_page_must_use_draft_saving_and_publishing",
        "post/page phải sử dụng cơ chế Lưu nháp và Xuất bản.",
        "post/page must use draft saving and publishing.",
    ),
    (
        "content.pattern_must_always_be_private_and_support_only_builder",
        "Pattern phải luôn riêng tư và chỉ hỗ trợ Builder.",
        "Pattern must always be private and support only Builder.",
    ),
    (
        "content.reserved_panel_namespace_the_core_or_layout_system_prefix_must_not_be_declared",
        "reserved_panel_namespace: Không được khai báo tiền tố hệ thống 'core:' hoặc 'layout:'.",
        "reserved_panel_namespace: The 'core:' or 'layout:' system prefix must not be declared.",
    ),
    (
        "content.field_group_local_key_must_be_valid_snake_case_1_64_characters",
        "local_key của field_group phải là snake_case hợp lệ (1..=64 ký tự).",
        "field_group local_key must be valid snake_case (1..=64 characters).",
    ),
    (
        "content.invalid_field_group_title",
        "Tiêu đề của field_group không hợp lệ.",
        "Invalid field_group title.",
    ),
    (
        "content.the_field_group_description_exceeds_the_limit_or_contains_control_characters",
        "Mô tả của field_group vượt quá giới hạn hoặc chứa ký tự điều khiển.",
        "The field_group description exceeds the limit or contains control characters.",
    ),
    (
        "content.field_group_target_types_must_contain_1_to_16_content_types",
        "target_types của field_group phải có từ 1 đến 16 loại nội dung.",
        "field_group target_types must contain 1 to 16 content types.",
    ),
    (
        "content.invalid_or_duplicate_target_types_in_field_group",
        "target_types trong field_group không hợp lệ hoặc bị trùng lặp.",
        "Invalid or duplicate target_types in field_group.",
    ),
    (
        "content.invalid_panel_region_field_group_default_region_must_not_be_header_bar",
        "invalid_panel_region: default_region của field_group không được đặt vào header_bar.",
        "invalid_panel_region: field_group default_region must not be header_bar.",
    ),
    (
        "content.field_group_default_order_must_be_in_1000_1000",
        "default_order của field_group phải nằm trong khoảng [-1000..=1000].",
        "field_group default_order must be in [-1000..=1000].",
    ),
    (
        "content.each_field_group_must_declare_1_to_64_custom_fields",
        "Mỗi field_group phải khai báo từ 1 đến 64 custom fields.",
        "Each field_group must declare 1 to 64 custom fields.",
    ),
    (
        "content.field_key_collision_duplicate_custom_field_key_in_the_same_field_group",
        "field_key_collision: Trùng khóa custom field trong cùng field_group.",
        "field_key_collision: Duplicate custom field key in the same field_group.",
    ),
    (
        "content.the_user_field_group_group_id_and_local_key_do_not_match",
        "group_id và local_key của User Field Group không khớp nhau.",
        "The User Field Group group_id and local_key do not match.",
    ),
    (
        "content.the_user_api_may_only_create_or_edit_field_groups_under_user",
        "User API chỉ được phép tạo hoặc sửa nhóm trường thuộc 'user:'.",
        "The User API may only create or edit field groups under 'user:'.",
    ),
    (
        "content.local_key_or_group_id_is_required_for_the_field_group",
        "Cần cung cấp local_key hoặc group_id cho nhóm trường.",
        "local_key or group_id is required for the field group.",
    ),
    (
        "content.group_id_in_the_body_does_not_match_the_url_path",
        "group_id trong body không khớp với đường dẫn URL.",
        "group_id in the body does not match the URL path.",
    ),
    (
        "content.each_package_supports_at_most_64_taxonomy_definitions",
        "Mỗi gói hỗ trợ tối đa 64 taxonomy_definitions.",
        "Each package supports at most 64 taxonomy_definitions.",
    ),
    (
        "content.duplicate_taxonomy_key_in_declarations",
        "Khóa taxonomy bị trùng lặp trong declarations.",
        "Duplicate taxonomy key in declarations.",
    ),
    (
        "content.core_post_page_pattern_do_not_support_a_provider_specific_rewrite_base",
        "Core Post/Page/Pattern không hỗ trợ rewrite_base riêng của provider.",
        "Core Post/Page/Pattern do not support a provider-specific rewrite_base.",
    ),
    (
        "content.invalid_content_type_key_in_taxonomy_eligibility",
        "Khóa loại nội dung trong taxonomy_eligibility không hợp lệ.",
        "Invalid content type key in taxonomy_eligibility.",
    ),
    (
        "content.duplicate_content_type_taxonomy_association_in_declarations",
        "Trùng association content_type/taxonomy trong declarations.",
        "Duplicate content_type/taxonomy association in declarations.",
    ),
    (
        "content.each_package_supports_at_most_16_field_groups_in_declarations",
        "Mỗi gói hỗ trợ tối đa 16 field_groups trong declarations.",
        "Each package supports at most 16 field_groups in declarations.",
    ),
    (
        "content.duplicate_panel_id_duplicate_local_key_in_the_field_groups_list",
        "duplicate_panel_id: Trùng local_key trong danh sách field_groups.",
        "duplicate_panel_id: Duplicate local_key in the field_groups list.",
    ),
    (
        "content.rewrite_base_must_contain_1_to_3_valid_kebab_case_segments",
        "rewrite_base phải có từ 1 đến 3 đoạn kebab-case hợp lệ.",
        "rewrite_base must contain 1 to 3 valid kebab-case segments.",
    ),
    (
        "content.each_content_type_supports_at_most_64_taxonomy_associations",
        "Mỗi content type hỗ trợ tối đa 64 taxonomy associations.",
        "Each content type supports at most 64 taxonomy associations.",
    ),
    (
        "content.invalid_taxonomy_key_in_eligibility",
        "Khóa taxonomy trong eligibility không hợp lệ.",
        "Invalid taxonomy key in eligibility.",
    ),
    (
        "content.duplicate_taxonomy_key_in_eligibility",
        "Khóa taxonomy trùng lặp trong eligibility.",
        "Duplicate taxonomy key in eligibility.",
    ),
    (
        "content.the_taxonomy_key_belongs_to_a_protected_core_namespace",
        "Khóa taxonomy thuộc namespace Core được bảo vệ.",
        "The taxonomy key belongs to a protected Core namespace.",
    ),
    (
        "content.invalid_taxonomy_name_at_most_100_characters",
        "Tên taxonomy không hợp lệ (tối đa 100 ký tự).",
        "Invalid taxonomy name (at most 100 characters).",
    ),
    (
        "content.at_least_one_taxonomy_property_is_required_for_an_update",
        "Cần ít nhất một thuộc tính taxonomy để cập nhật.",
        "At least one taxonomy property is required for an update.",
    ),
    (
        "content.invalid_content_title",
        "Tiêu đề nội dung không hợp lệ.",
        "Invalid content title.",
    ),
    (
        "content.invalid_content_slug",
        "Slug nội dung không hợp lệ.",
        "Invalid content slug.",
    ),
    (
        "content.term_ids_must_be_a_list_of_unique_positive_ids_at_most_96",
        "term_ids phải là danh sách ID dương, duy nhất (tối đa 96).",
        "term_ids must be a list of unique positive IDs (at most 96).",
    ),
    (
        "content.the_values_contain_an_undeclared_custom_field",
        "Giá trị chứa custom field chưa được khai báo.",
        "The values contain an undeclared custom field.",
    ),
    (
        "content.a_required_custom_field_is_missing",
        "Thiếu custom field bắt buộc.",
        "A required custom field is missing.",
    ),
    (
        "content.the_custom_field_value_has_the_wrong_type_or_exceeds_the_limit",
        "Giá trị custom field không đúng kiểu hoặc vượt giới hạn.",
        "The custom field value has the wrong type or exceeds the limit.",
    ),
    (
        "content.cannot_remove_field_with_data_a_field_cannot_be_deleted_or_its_key_changed_while_existing_data_remains",
        "cannot_remove_field_with_data: Không thể xóa hoặc đổi key field khi đã có dữ liệu cũ.",
        "cannot_remove_field_with_data: A field cannot be deleted or its key changed while existing data remains.",
    ),
    (
        "content.incompatible_schema_change_an_existing_custom_field_s_data_type_cannot_be_changed",
        "incompatible_schema_change: Không thể đổi kiểu dữ liệu (type) của custom field đã tồn tại.",
        "incompatible_schema_change: An existing custom field's data type cannot be changed.",
    ),
    (
        "content.incompatible_schema_change_an_optional_field_cannot_become_required_while_existing_data_remains",
        "incompatible_schema_change: Không thể đổi field tùy chọn thành bắt buộc (required) khi đã có dữ liệu.",
        "incompatible_schema_change: An optional field cannot become required while existing data remains.",
    ),
    (
        "content.incompatible_schema_change_a_select_option_cannot_be_removed_while_existing_data_remains",
        "incompatible_schema_change: Không thể xóa lựa chọn (select option) khi đã có dữ liệu cũ.",
        "incompatible_schema_change: A select option cannot be removed while existing data remains.",
    ),
    (
        "content.incompatible_schema_change_a_new_required_field_cannot_be_added_while_existing_data_remains",
        "incompatible_schema_change: Không thể thêm field bắt buộc (required) mới khi đã có dữ liệu.",
        "incompatible_schema_change: A new required field cannot be added while existing data remains.",
    ),
    (
        "content.reserved_panel_namespace_the_core_or_layout_prefix_must_not_be_declared_in_editor_panels",
        "reserved_panel_namespace: Không được khai báo tiền tố 'core:' hoặc 'layout:' trong editor_panels.",
        "reserved_panel_namespace: The 'core:' or 'layout:' prefix must not be declared in editor_panels.",
    ),
    (
        "content.editor_panel_local_key_must_be_valid_snake_case_1_64_characters",
        "local_key của editor_panel phải là snake_case hợp lệ (1..=64 ký tự).",
        "editor_panel local_key must be valid snake_case (1..=64 characters).",
    ),
    (
        "content.invalid_editor_panel_title_or_button_label",
        "Tiêu đề hoặc nhãn nút của editor_panel không hợp lệ.",
        "Invalid editor_panel title or button label.",
    ),
    (
        "content.invalid_editor_panel_confirm_message",
        "confirm_message của editor_panel không hợp lệ.",
        "Invalid editor_panel confirm_message.",
    ),
    (
        "content.editor_panel_applies_to_must_contain_1_to_16_content_types",
        "applies_to của editor_panel phải có từ 1 đến 16 loại nội dung.",
        "editor_panel applies_to must contain 1 to 16 content types.",
    ),
    (
        "content.invalid_or_duplicate_editor_panel_applies_to",
        "applies_to của editor_panel không hợp lệ hoặc bị trùng lặp.",
        "Invalid or duplicate editor_panel applies_to.",
    ),
    (
        "content.invalid_panel_region_allowed_regions_must_not_be_empty",
        "invalid_panel_region: allowed_regions không được rỗng.",
        "invalid_panel_region: allowed_regions must not be empty.",
    ),
    (
        "content.invalid_panel_region_an_action_panel_may_only_be_in_main_top_main_bottom_or_sidebar",
        "invalid_panel_region: Action panel chỉ được phép đặt trong main_top, main_bottom hoặc sidebar.",
        "invalid_panel_region: An action panel may only be in main_top, main_bottom or sidebar.",
    ),
    (
        "content.invalid_panel_region_default_region_must_be_in_allowed_regions",
        "invalid_panel_region: default_region phải nằm trong allowed_regions.",
        "invalid_panel_region: default_region must be in allowed_regions.",
    ),
    (
        "content.editor_panel_default_order_must_be_in_1000_1000",
        "default_order của editor_panel phải nằm trong [-1000..=1000].",
        "editor_panel default_order must be in [-1000..=1000].",
    ),
    (
        "content.invalid_panel_acl_min_edit_role_must_not_be_below_min_view_role",
        "invalid_panel_acl: min_edit_role không được thấp hơn min_view_role.",
        "invalid_panel_acl: min_edit_role must not be below min_view_role.",
    ),
    (
        "auth.could_not_hash_the_password",
        "Không thể băm mật khẩu.",
        "Could not hash the password.",
    ),
    (
        "package.could_not_determine_the_backup_zip_file_size",
        "Không xác định được kích thước tệp backup ZIP.",
        "Could not determine the backup ZIP file size.",
    ),
    (
        "package.could_not_read_the_backup_zip_eocd",
        "Không đọc được EOCD của backup ZIP.",
        "Could not read the backup ZIP EOCD.",
    ),
    (
        "package.could_not_read_the_entire_backup_zip_eocd",
        "Không đọc đủ EOCD của backup ZIP.",
        "Could not read the entire backup ZIP EOCD.",
    ),
    (
        "package.could_not_check_the_zip64_locator",
        "Không kiểm tra được ZIP64 locator.",
        "Could not check the ZIP64 locator.",
    ),
    (
        "package.could_not_read_the_zip64_locator",
        "Không đọc được ZIP64 locator.",
        "Could not read the ZIP64 locator.",
    ),
    (
        "package.could_not_seek_to_the_backup_zip_central_directory",
        "Không seek được tới Central Directory của backup ZIP.",
        "Could not seek to the backup ZIP Central Directory.",
    ),
    (
        "package.could_not_read_the_entire_backup_zip_central_directory",
        "Không đọc trọn vẹn Central Directory của backup ZIP.",
        "Could not read the entire backup ZIP Central Directory.",
    ),
    (
        "package.backup_zip_entry_offset_overflow",
        "Offset entry backup ZIP bị tràn số.",
        "Backup ZIP entry offset overflow.",
    ),
    (
        "package.could_not_rewind_the_backup_zip",
        "Không tua lại được backup ZIP.",
        "Could not rewind the backup ZIP.",
    ),
    (
        "package.could_not_open_the_backup_zip_after_preflight",
        "Không mở được backup ZIP sau preflight.",
        "Could not open the backup ZIP after preflight.",
    ),
    (
        "page.could_not_serialize_the_block_schema",
        "Không thể tuần tự hóa block schema.",
        "Could not serialize the block schema.",
    ),
    (
        "plugin.error_reading_the_http_response",
        "Lỗi khi đọc response HTTP.",
        "Error reading the HTTP response.",
    ),
    ("plugin.invalid_url", "URL không hợp lệ.", "Invalid URL."),
    (
        "plugin.could_not_initialize_the_http_client",
        "Không thể khởi tạo HTTP client.",
        "Could not initialize the HTTP client.",
    ),
    (
        "plugin.invalid_http_method",
        "Method HTTP không hợp lệ.",
        "Invalid HTTP method.",
    ),
    (
        "plugin.the_http_response_is_not_valid_utf_8",
        "Response HTTP không phải UTF-8 hợp lệ.",
        "The HTTP response is not valid UTF-8.",
    ),
    (
        "package.could_not_read_an_item_in_the_zip",
        "Không thể đọc mục trong ZIP.",
        "Could not read an item in the ZIP.",
    ),
    (
        "package.the_zip_contains_an_invalid_utf_8_path",
        "ZIP chứa đường dẫn UTF-8 không hợp lệ.",
        "The ZIP contains an invalid UTF-8 path.",
    ),
    (
        "package.the_zip_contains_a_file_not_supported_by_the_abi_v1_contract",
        "ZIP chứa file không được hỗ trợ trong hợp đồng ABI v1.",
        "The ZIP contains a file not supported by the ABI v1 contract.",
    ),
    (
        "package.the_extracted_size_exceeds_the_limit",
        "Dung lượng giải nén vượt quá giới hạn.",
        "The extracted size exceeds the limit.",
    ),
    (
        "package.manifest_json_is_not_valid_utf_8",
        "manifest.json không phải UTF-8 hợp lệ.",
        "manifest.json is not valid UTF-8.",
    ),
    (
        "package.the_zip_is_missing_manifest_json",
        "ZIP thiếu file manifest.json.",
        "The ZIP is missing manifest.json.",
    ),
    (
        "package.the_zip_is_missing_plugin_wasm",
        "ZIP thiếu file plugin.wasm.",
        "The ZIP is missing plugin.wasm.",
    ),
    (
        "package.admin_extensions_is_not_valid_json",
        "admin_extensions không đúng JSON.",
        "admin_extensions is not valid JSON.",
    ),
    (
        "package.invalid_admin_extensions_structure",
        "admin_extensions không đúng cấu trúc.",
        "Invalid admin_extensions structure.",
    ),
    (
        "package.the_webassembly_module_is_invalid_or_malformed",
        "Module WebAssembly không hợp lệ hoặc cấu trúc bị lỗi.",
        "The WebAssembly module is invalid or malformed.",
    ),
    (
        "plugin.the_ipc_frame_is_too_large_for_a_u32_length_prefix",
        "IPC frame quá lớn cho u32 length prefix.",
        "The IPC frame is too large for a u32 length prefix.",
    ),
    (
        "plugin.the_ipc_mutex_is_locked",
        "IPC mutex bị khóa.",
        "The IPC mutex is locked.",
    ),
    (
        "public_context.invalid_query_value",
        "Giá trị query không hợp lệ.",
        "Invalid query value.",
    ),
    (
        "seo.could_not_serialize_the_json_ld_block",
        "Không thể serialize block json_ld.",
        "Could not serialize the json_ld block.",
    ),
    (
        "import.the_sample_setting_option_key_is_outside_the_revision_schema",
        "Sample setting có option_key ngoài schema revision.",
        "The sample setting option_key is outside the revision schema.",
    ),
    (
        "plugin.the_plugin_revision_is_missing_the_complete_marker",
        "Revision plugin thiếu marker .complete.",
        "The plugin revision is missing the .complete marker.",
    ),
    (
        "plugin.the_plugin_revision_is_missing_manifest_json_on_disk",
        "Revision plugin thiếu manifest.json trên đĩa.",
        "The plugin revision is missing manifest.json on disk.",
    ),
    (
        "plugin.the_plugin_revision_is_missing_plugin_wasm_on_disk",
        "Revision plugin thiếu plugin.wasm trên đĩa.",
        "The plugin revision is missing plugin.wasm on disk.",
    ),
    (
        "plugin.could_not_read_admin_extensions_json_in_the_plugin_revision",
        "Revision plugin không đọc được admin-extensions.json.",
        "Could not read admin-extensions.json in the plugin revision.",
    ),
    (
        "plugin.invalid_admin_extension_revision",
        "Admin extension revision không hợp lệ.",
        "Invalid Admin extension revision.",
    ),
    (
        "plugin.could_not_serialize_the_admin_extension_revision",
        "Admin extension revision không serialize được.",
        "Could not serialize the Admin extension revision.",
    ),
    (
        "plugin.could_not_read_blocks_json_in_the_plugin_revision",
        "Revision plugin không đọc được blocks.json.",
        "Could not read blocks.json in the plugin revision.",
    ),
    (
        "plugin.invalid_plugin_revision_blocks_json_structure",
        "Revision plugin blocks.json không đúng cấu trúc.",
        "Invalid plugin revision blocks.json structure.",
    ),
    (
        "plugin.plugin_revision_blocks_json_is_empty",
        "Revision plugin blocks.json trống.",
        "Plugin revision blocks.json is empty.",
    ),
    (
        "plugin_projections.could_not_serialize_filter_entry_view",
        "Không thể serialize filter:entry/view.",
        "Could not serialize filter:entry/view.",
    ),
    (
        "settings.a_string_option_value_must_be_a_string",
        "Giá trị option kiểu string phải là chuỗi.",
        "A string option value must be a string.",
    ),
    (
        "settings.a_url_option_value_must_be_a_string",
        "Giá trị option kiểu url phải là chuỗi.",
        "A url option value must be a string.",
    ),
    (
        "settings.a_number_option_value_must_be_a_finite_number",
        "Giá trị option kiểu number phải là số hữu hạn.",
        "A number option value must be a finite number.",
    ),
    (
        "settings.a_select_option_must_be_a_choice_string",
        "Option kiểu select phải là chuỗi lựa chọn.",
        "A select option must be a choice string.",
    ),
    (
        "settings.invalid_theme_json",
        "theme.json không hợp lệ.",
        "Invalid theme.json.",
    ),
    (
        "settings.theme_json_is_not_valid_json",
        "theme.json không đúng JSON.",
        "theme.json is not valid JSON.",
    ),
    (
        "settings.public_origin_must_be_a_valid_http_s_url",
        "Public origin phải là URL HTTP(S) hợp lệ.",
        "Public origin must be a valid HTTP(S) URL.",
    ),
    (
        "template_sec.could_not_create_the_nonce",
        "Không tạo được nonce.",
        "Could not create the nonce.",
    ),
    (
        "template_sec.could_not_compile_theme_composition",
        "Không thể biên dịch theme composition.",
        "Could not compile theme composition.",
    ),
    (
        "template_sec.theme_composition_not_found",
        "Không tìm thấy theme composition.",
        "Theme composition not found.",
    ),
    (
        "template_sec.theme_composition_execution_error_fuel_recursion_undefined",
        "Lỗi thực thi theme composition (fuel/recursion/undefined).",
        "Theme composition execution error (fuel/recursion/undefined).",
    ),
    (
        "template_sec.composition_fuel_tracker_missing",
        "Thiếu bộ theo dõi fuel composition.",
        "Composition fuel tracker missing.",
    ),
    (
        "template_sec.could_not_compile_the_theme_preview",
        "Không thể biên dịch theme preview.",
        "Could not compile the theme preview.",
    ),
    (
        "template_sec.theme_preview_not_found",
        "Không tìm thấy theme preview.",
        "Theme preview not found.",
    ),
    (
        "template_sec.theme_preview_execution_error_fuel_recursion_undefined",
        "Lỗi thực thi theme preview (fuel/recursion/undefined).",
        "Theme preview execution error (fuel/recursion/undefined).",
    ),
    (
        "template_sec.preview_fuel_tracker_missing",
        "Thiếu bộ theo dõi fuel preview.",
        "Preview fuel tracker missing.",
    ),
    (
        "template_sec.invalid_preview_fuel",
        "Fuel preview không hợp lệ.",
        "Invalid preview fuel.",
    ),
    (
        "template_sec.could_not_create_the_preview_nonce",
        "Không tạo được nonce preview.",
        "Could not create the preview nonce.",
    ),
    (
        "template_sec.od_part_points_to_a_nonexistent_part",
        "od_part trỏ tới part không tồn tại.",
        "od_part points to a nonexistent part.",
    ),
    (
        "template_sec.invalid_preview_nonce_position",
        "Vị trí nonce preview không hợp lệ.",
        "Invalid preview nonce position.",
    ),
    (
        "template_sec.invalid_minijinja_template_syntax",
        "Cú pháp template MiniJinja không hợp lệ.",
        "Invalid MiniJinja template syntax.",
    ),
    (
        "template_sec.could_not_compile_the_template_in_a_secure_environment",
        "Không thể biên dịch template trong môi trường an toàn.",
        "Could not compile the template in a secure environment.",
    ),
    (
        "template_sec.compiled_template_not_found",
        "Không tìm thấy template đã biên dịch.",
        "Compiled template not found.",
    ),
    (
        "template_sec.template_execution_error_fuel_recursion_strict_undefined",
        "Lỗi thực thi template (fuel/recursion/strict undefined).",
        "Template execution error (fuel/recursion/strict undefined).",
    ),
    (
        "template_sec.validated_part_missing",
        "Thiếu part đã kiểm tra.",
        "Validated part missing.",
    ),
    (
        "template_sec.invalid_minijinja_theme_syntax",
        "Cú pháp theme MiniJinja không hợp lệ.",
        "Invalid MiniJinja theme syntax.",
    ),
    (
        "template_sec.invalid_composition_expression_position",
        "Vị trí biểu thức composition không hợp lệ.",
        "Invalid composition expression position.",
    ),
    (
        "template_sec.od_part_od_content_is_missing_a_closing_delimiter",
        "od_part/od_content thiếu dấu đóng.",
        "od_part/od_content is missing a closing delimiter.",
    ),
    (
        "template_sec.od_part_must_use_a_string_literal",
        "od_part() phải dùng string literal.",
        "od_part() must use a string literal.",
    ),
    (
        "template_sec.the_srcset_attribute_contains_an_empty_item",
        "Thuộc tính srcset chứa mục rỗng.",
        "The srcset attribute contains an empty item.",
    ),
    (
        "template_sec.invalid_template_block_minijinja_syntax",
        "Cú pháp MiniJinja của template block không hợp lệ.",
        "Invalid template block MiniJinja syntax.",
    ),
    (
        "template_sec.could_not_compile_the_template_block_in_the_c_sec_environment",
        "Không thể biên dịch template block trong môi trường C-SEC.",
        "Could not compile the template block in the C-SEC environment.",
    ),
    (
        "template_sec.could_not_create_the_richtext_nonce",
        "Không tạo được nonce RichText.",
        "Could not create the RichText nonce.",
    ),
    (
        "template_sec.could_not_compile_the_template_block",
        "Không thể biên dịch template block.",
        "Could not compile the template block.",
    ),
    (
        "template_sec.template_block_not_found",
        "Không tìm thấy template block.",
        "Template block not found.",
    ),
    (
        "template_sec.template_block_execution_error_fuel_recursion_undefined",
        "Lỗi thực thi template block (fuel/recursion/undefined).",
        "Template block execution error (fuel/recursion/undefined).",
    ),
    (
        "template_sec.template_block_fuel_tracker_missing",
        "Thiếu bộ theo dõi fuel cho template block.",
        "Template block fuel tracker missing.",
    ),
    (
        "theme.could_not_render_the_content_template",
        "Không thể render template nội dung.",
        "Could not render the content template.",
    ),
    (
        "theme.the_block_is_missing_block_json",
        "Block thiếu block.json.",
        "The block is missing block.json.",
    ),
    (
        "theme.block_json_must_be_valid_utf_8",
        "block.json phải là UTF-8 hợp lệ.",
        "block.json must be valid UTF-8.",
    ),
    (
        "theme.the_block_is_missing_render_html",
        "Block thiếu render.html.",
        "The block is missing render.html.",
    ),
    (
        "theme.render_html_must_be_valid_utf_8",
        "render.html phải là UTF-8 hợp lệ.",
        "render.html must be valid UTF-8.",
    ),
    (
        "theme.block_json_does_not_match_schema_v1",
        "block.json không đúng schema v1.",
        "block.json does not match schema v1.",
    ),
    (
        "theme.the_locale_dictionary_file_must_be_valid_utf_8",
        "File từ điển locale phải là UTF-8 hợp lệ.",
        "The locale dictionary file must be valid UTF-8.",
    ),
    (
        "theme.the_locale_dictionary_file_is_not_valid_json",
        "File từ điển locale không phải JSON hợp lệ.",
        "The locale dictionary file is not valid JSON.",
    ),
    (
        "theme.the_locale_dictionary_file_must_be_a_flat_json_object",
        "File từ điển locale phải là JSON object phẳng.",
        "The locale dictionary file must be a flat JSON object.",
    ),
    (
        "theme.locale_dictionary_values_must_be_plain_strings",
        "Giá trị trong từ điển locale phải là chuỗi phẳng.",
        "Locale dictionary values must be plain strings.",
    ),
    (
        "theme.invalid_v3_theme_manifest",
        "Manifest theme v3 không hợp lệ.",
        "Invalid v3 theme manifest.",
    ),
    (
        "backup.the_original_media_key_must_start_with_media",
        "Media original key phải bắt đầu bằng media/.",
        "The original media key must start with media/.",
    ),
    (
        "backup.the_original_media_key_is_missing_its_shard",
        "Media original key thiếu shard.",
        "The original media key is missing its shard.",
    ),
    (
        "backup.the_original_media_key_is_missing_its_extension",
        "Media original key thiếu phần mở rộng.",
        "The original media key is missing its extension.",
    ),
    (
        "backup.the_media_thumbnail_key_must_start_with_media",
        "Media thumbnail key phải bắt đầu bằng media/.",
        "The media thumbnail key must start with media/.",
    ),
    (
        "backup.the_media_thumbnail_key_is_missing_its_shard",
        "Media thumbnail key thiếu shard.",
        "The media thumbnail key is missing its shard.",
    ),
    (
        "backup.the_media_thumbnail_key_must_end_in_thumb_png",
        "Media thumbnail key phải kết thúc bằng .thumb.png.",
        "The media thumbnail key must end in .thumb.png.",
    ),
    (
        "backup.the_secret_logical_key_must_start_with_secrets_smtp",
        "Secret logical_key phải bắt đầu bằng secrets/smtp/.",
        "The secret logical_key must start with secrets/smtp/.",
    ),
    (
        "backup.the_secret_logical_key_must_end_in_json_or_bin",
        "Secret logical_key phải có đuôi .json hoặc .bin.",
        "The secret logical_key must end in .json or .bin.",
    ),
    (
        "backup.the_quarantine_logical_key_must_start_with_quarantine",
        "Quarantine logical_key phải bắt đầu bằng quarantine/.",
        "The quarantine logical_key must start with quarantine/.",
    ),
    (
        "backup.the_quarantine_logical_key_is_missing_job_id",
        "Quarantine logical_key thiếu job_id.",
        "The quarantine logical_key is missing job_id.",
    ),
    (
        "backup.total_backup_size_overflow",
        "Tổng kích thước backup bị tràn số.",
        "Total backup size overflow.",
    ),
    (
        "block_schema.could_not_serialize_block_data",
        "Không thể tuần tự hóa dữ liệu block.",
        "Could not serialize block data.",
    ),
    (
        "block_schema.the_text_value_must_be_a_string",
        "Giá trị Text phải là chuỗi.",
        "The Text value must be a string.",
    ),
    (
        "block_schema.the_textarea_value_must_be_a_string",
        "Giá trị Textarea phải là chuỗi.",
        "The Textarea value must be a string.",
    ),
    (
        "block_schema.the_richtext_value_must_be_a_string",
        "Giá trị Richtext phải là chuỗi.",
        "The Richtext value must be a string.",
    ),
    (
        "block_schema.the_number_value_must_be_numeric",
        "Giá trị Number phải là số.",
        "The Number value must be numeric.",
    ),
    (
        "block_schema.the_range_value_must_be_numeric",
        "Giá trị Range phải là số.",
        "The Range value must be numeric.",
    ),
    (
        "block_schema.the_select_radiobuttons_value_must_be_a_string",
        "Giá trị Select/RadioButtons phải là chuỗi.",
        "The Select/RadioButtons value must be a string.",
    ),
    (
        "block_schema.the_spacing_value_must_be_a_json_object",
        "Giá trị Spacing phải là JSON object {top, right, bottom, left}.",
        "The Spacing value must be a JSON object {top, right, bottom, left}.",
    ),
    (
        "block_schema.the_spacing_side_value_must_be_numeric",
        "Giá trị cạnh Spacing phải là số.",
        "The Spacing side value must be numeric.",
    ),
    (
        "block_schema.the_color_value_must_be_a_string",
        "Giá trị Color phải là chuỗi.",
        "The Color value must be a string.",
    ),
    (
        "block_schema.the_media_value_must_be_a_sha_256_string",
        "Giá trị Media phải là chuỗi SHA-256.",
        "The Media value must be a SHA-256 string.",
    ),
    (
        "block_schema.the_url_value_must_be_a_string",
        "Giá trị Url phải là chuỗi.",
        "The Url value must be a string.",
    ),
    (
        "builder.invalid_richtext_html_in_core_text",
        "HTML RichText trong core/text không hợp lệ.",
        "Invalid RichText HTML in core/text",
    ),
    (
        "builder.legacy_block_descriptor_unavailable",
        "Descriptor block cũ không khả dụng.",
        "Legacy block descriptor unavailable",
    ),
    (
        "builder.legacy_field_map_does_not_match_descriptor",
        "Bản đồ trường cũ không khớp descriptor.",
        "Legacy field map does not match descriptor",
    ),
    (
        "builder.unknown_plugin_block_descriptor",
        "Descriptor block plugin không xác định.",
        "Unknown plugin block descriptor",
    ),
    (
        "builder.plugin_block_props_invalid_json",
        "Props block plugin không phải JSON hợp lệ.",
        "Plugin block props invalid JSON",
    ),
    (
        "builder_blocks.invalid_block_json_payload",
        "Payload JSON block không hợp lệ.",
        "Invalid block JSON payload",
    ),
    (
        "builder_blocks.missing_block_node_id",
        "Thiếu ID node block.",
        "Missing block node id",
    ),
    (
        "builder_blocks.invalid_block_meta_payload",
        "Payload meta block không hợp lệ.",
        "Invalid block meta payload",
    ),
    (
        "builder_blocks.invalid_block_props_payload",
        "Payload props block không hợp lệ.",
        "Invalid block props payload",
    ),
    (
        "builder_blocks.invalid_unresolved_block_json",
        "JSON block chưa được phân giải không hợp lệ.",
        "Invalid unresolved block JSON",
    ),
    (
        "plugin.invalid_manifest_json",
        "JSON manifest không hợp lệ.",
        "Invalid manifest JSON.",
    ),
    (
        "plugin.invalid_url_in_http_permissions",
        "URL trong quyền HTTP không hợp lệ.",
        "Invalid URL in HTTP permissions.",
    ),
    (
        "settings.invalid_cms_locale",
        "Ngôn ngữ CMS không có trong danh mục hỗ trợ.",
        "The CMS language is not in the supported catalog.",
    ),
    (
        "settings.secondary_cms_locale_locked",
        "Ngôn ngữ CMS là cấu hình toàn cục và chỉ được thay đổi ở ngôn ngữ mặc định.",
        "The CMS language is a global setting and can only be changed in the default language.",
    ),
    (
        "request.bad_host",
        "Header Host không hợp lệ.",
        "Invalid Host header.",
    ),
    (
        "request.bad_origin",
        "Origin không hợp lệ.",
        "Invalid Origin.",
    ),
    (
        "request.method_not_allowed",
        "Phương thức không được phép.",
        "Method Not Allowed",
    ),
    (
        "request.method_not_allowed",
        "Phương thức không được phép.",
        "405 Method Not Allowed",
    ),
    (
        "request.unsupported_media_type",
        "Loại nội dung không được hỗ trợ.",
        "Unsupported Media Type",
    ),
    (
        "request.unsupported_media_type",
        "Loại nội dung không được hỗ trợ.",
        "415 Unsupported Media Type",
    ),
    (
        "request.body_too_large",
        "Nội dung yêu cầu vượt quá giới hạn kích thước.",
        "Payload Too Large",
    ),
    (
        "request.body_too_large",
        "Vượt quá giới hạn kích thước.",
        "length limit exceeded",
    ),
    (
        "request.body_too_large",
        "Không thể đọc nội dung yêu cầu: vượt quá giới hạn kích thước.",
        "Failed to buffer the request body: length limit exceeded",
    ),
    (
        "request.invalid_json",
        "Không thể phân tích cú pháp JSON của yêu cầu.",
        "Failed to parse the request body as JSON",
    ),
    (
        "request.invalid_json",
        "Không thể giải mã nội dung JSON thành kiểu đích.",
        "Failed to deserialize the JSON body into the target type",
    ),
    (
        "request.unsupported_media_type",
        "Yêu cầu có `Content-Type: application/json`.",
        "Expected request with `Content-Type: application/json`",
    ),
    (
        "request.unsupported_media_type",
        "Yêu cầu biểu mẫu phải có `Content-Type: application/x-www-form-urlencoded`.",
        "Form requests must have `Content-Type: application/x-www-form-urlencoded`",
    ),
    (
        "request.invalid_path",
        "Không tìm thấy tham số đường dẫn cho route đã khớp.",
        "No paths parameters found for matched route",
    ),
    (
        "server.error",
        "Không tìm thấy đường dẫn đã khớp.",
        "No matched path found",
    ),
    (
        "server.error",
        "Route đã khớp không phải route lồng nhau.",
        "The matched route is not nested",
    ),
    (
        "storage.busy",
        "Đã có phiên bảo trì độc quyền đang hoạt động.",
        "An exclusive maintenance session is already active.",
    ),
    (
        "plugin.error",
        "Plugin chưa được bật hoặc đã bị vô hiệu hóa.",
        "The plugin is not enabled or has been disabled.",
    ),
    (
        "plugin.error",
        "Plugin hết fuel (vượt quá giới hạn CPU).",
        "Plugin ran out of fuel (CPU limit exceeded).",
    ),
    (
        "plugin.error",
        "Quá thời gian thực thi (deadline 5 giây).",
        "Execution timed out (5-second deadline).",
    ),
    (
        "plugin.error",
        "Lỗi con trỏ bộ nhớ (Bad Pointer hoặc ngoài biên memory).",
        "Memory pointer error (bad pointer or out of bounds).",
    ),
    (
        "plugin.error",
        "Không thể lấy permit điều phối plugin.",
        "Could not acquire a plugin scheduling permit.",
    ),
    (
        "plugin.error",
        "Hệ thống đang ở safe mode sau khi khôi phục; plugin bị tạm khóa.",
        "The system is in safe mode after restoration; plugins are temporarily disabled.",
    ),
    (
        "plugin.error",
        "Lỗi khóa action lease.",
        "Action lease lock error.",
    ),
    (
        "plugin.error",
        "Không cho phép gọi đệ quy action trên cùng plugin.",
        "Recursive action calls on the same plugin are not allowed.",
    ),
    (
        "plugin.error",
        "Action không tồn tại trong manifest.",
        "The action does not exist in the manifest.",
    ),
    (
        "plugin.error",
        "Tài khoản không đủ quyền thực thi action này.",
        "The account does not have permission to execute this action.",
    ),
    (
        "plugin.error",
        "Plugin không có quyền ghi loại nội dung này.",
        "The plugin cannot write this content type.",
    ),
    (
        "plugin.error",
        "Plugin không có quyền xóa loại nội dung này.",
        "The plugin cannot delete this content type.",
    ),
    (
        "plugin.error",
        "Không tìm thấy plugin vừa cài đặt.",
        "The newly installed plugin was not found.",
    ),
    (
        "plugin.error",
        "Envelope kind route/cron chưa mở ở V1.",
        "Envelope kind route/cron is not available in V1.",
    ),
    (
        "plugin.error",
        "Không thể đọc độ dài wasm bytes từ stdin.",
        "Could not read the wasm byte length from stdin.",
    ),
    (
        "plugin.error",
        "Không thể đọc dữ liệu wasm bytes từ stdin.",
        "Could not read wasm bytes from stdin.",
    ),
    (
        "plugin.error",
        "Vượt quá giới hạn bộ nhớ 16 MB.",
        "The 16 MB memory limit was exceeded.",
    ),
    (
        "plugin.error",
        "Plugin không gọi host_write_output.",
        "The plugin did not call host_write_output.",
    ),
    (
        "hook.before_save_veto",
        "Không cho phép gọi lồng nhau trong content/before_save.",
        "Nested calls are not allowed in content/before_save.",
    ),
    (
        "hook.before_save_veto",
        "Không thể tra cứu plugin content/before_save.",
        "Could not look up the content/before_save plugin.",
    ),
    (
        "hook.before_save_veto",
        "Vượt quá giới hạn xử lý content/before_save.",
        "The content/before_save processing limit was exceeded.",
    ),
    (
        "hook.before_save_veto",
        "Đầu ra content/before_save không hợp lệ.",
        "Invalid content/before_save output.",
    ),
    (
        "hook.before_save_veto",
        "Plugin hook thất bại",
        "Plugin hook failed",
    ),
    (
        "hook.before_save_veto",
        "Không hỗ trợ điều phối lifecycle hoặc gọi lồng nhau",
        "Unsupported or nested lifecycle dispatch",
    ),
    (
        "hook.before_save_veto",
        "Điều phối lifecycle lồng nhau",
        "Nested lifecycle dispatch",
    ),
    (
        "seo.invalid_text",
        "Tiêu đề SEO không được chứa ký tự điều khiển.",
        "SEO title must not contain control characters.",
    ),
    (
        "seo.invalid_text",
        "Mô tả SEO không được chứa ký tự điều khiển.",
        "SEO description must not contain control characters.",
    ),
    (
        "smtp.invalid_address",
        "from_address không hợp lệ.",
        "Invalid from_address.",
    ),
    (
        "smtp.invalid_address",
        "from_address phải là đúng 1 địa chỉ email hợp lệ.",
        "from_address must be exactly one valid email address.",
    ),
    (
        "smtp.invalid_address",
        "recipient_address không hợp lệ.",
        "Invalid recipient_address.",
    ),
    (
        "smtp.invalid_address",
        "recipient_address phải là đúng 1 địa chỉ email hợp lệ.",
        "recipient_address must be exactly one valid email address.",
    ),
    (
        "smtp.invalid_address",
        "reply_to không hợp lệ.",
        "Invalid reply_to.",
    ),
    (
        "smtp.invalid_address",
        "reply_to phải là đúng 1 địa chỉ email hợp lệ.",
        "reply_to must be exactly one valid email address.",
    ),
];

const TEMPLATE_MATCHERS: &[(&str, &[&str], &[&str])] = &[
    (
        "content.cannot_remove_field_with_data_field_cannot_be_permanently_removed_while_it_contains_data_set_declaration_state_withdrawn",
        &[
            "cannot_remove_field_with_data: Không thể xóa hẳn trường '",
            "' khi đã có dữ liệu; hãy đặt declaration_state = withdrawn.",
        ],
        &[
            "cannot_remove_field_with_data: Field '",
            "' cannot be permanently removed while it contains data; set declaration_state = withdrawn.",
        ],
    ),
    (
        "settings.an_empty_value_for_secret_is_invalid_use_null_to_keep_it_or_to_delete_it",
        &[
            "Giá trị secret '",
            "' rỗng không hợp lệ; dùng null để giữ nguyên hoặc {\"clear\": true} để xóa.",
        ],
        &[
            "An empty value for secret '",
            "' is invalid; use null to keep it or {\"clear\": true} to delete it.",
        ],
    ),
    (
        "hook.the_plugin_or_current_role_cannot_write_field_via_filter_content_prepare_save",
        &[
            "Plugin hoặc vai trò hiện tại không có quyền ghi trường '",
            "' qua filter:content/prepare_save.",
        ],
        &[
            "The plugin or current role cannot write field '",
            "' via filter:content/prepare_save.",
        ],
    ),
    (
        "permalink.post_page_retain_their_separate_wordpress_style_routes_and_do_not_accept_rewrite_base",
        &[
            "Post/Page giữ route riêng theo chuẩn WordPress, không nhận rewrite_base: ",
            "",
        ],
        &[
            "Post/Page retain their separate WordPress-style routes and do not accept rewrite_base: ",
            "",
        ],
    ),
    (
        "storage.busy",
        &[
            "Hệ thống đang tạm khóa ghi để thực hiện bảo trì (",
            "). Vui lòng thử lại sau.",
        ],
        &[
            "Writes are temporarily locked for maintenance (",
            "). Please try again later.",
        ],
    ),
    (
        "media.related_json_data_is_malformed_media_references_cannot_be_verified",
        &[
            "Dữ liệu JSON liên quan (",
            ") bị lỗi định dạng; không thể xác minh tham chiếu media.",
        ],
        &[
            "Related JSON data (",
            ") is malformed; media references cannot be verified.",
        ],
    ),
    (
        "entries.field_copy_forbidden_the_current_role_cannot_copy_required_field",
        &[
            "field_copy_forbidden: Vai trò hiện tại không có quyền sao chép trường bắt buộc '",
            "'.",
        ],
        &[
            "field_copy_forbidden: The current role cannot copy required field '",
            "'.",
        ],
    ),
    (
        "entries.schema_version_conflict_current_schema_token_is_does_not_match",
        &[
            "schema_version_conflict: schema_token hiện tại là '",
            "', không khớp '",
            "'.",
        ],
        &[
            "schema_version_conflict: current schema_token is '",
            "', does not match '",
            "'.",
        ],
    ),
    (
        "media.media_asset_is_being_deleted_new_references_cannot_be_created",
        &[
            "Media asset '",
            "' đang trong quá trình xóa; không thể tạo tham chiếu mới.",
        ],
        &[
            "Media asset '",
            "' is being deleted; new references cannot be created.",
        ],
    ),
    (
        "entries.invalid_time_format_for_use_yyyy_mm_dd_hh_mm_ss_or_iso_8601",
        &[
            "Định dạng thời gian ",
            " không hợp lệ (dùng YYYY-MM-DD HH:MM:SS hoặc ISO-8601).",
        ],
        &[
            "Invalid time format for ",
            " (use YYYY-MM-DD HH:MM:SS or ISO-8601).",
        ],
    ),
    (
        "permalink.conflict",
        &[
            "permalink_category_root_collision: slug '",
            "' trùng gốc hệ thống.",
        ],
        &[
            "permalink_category_root_collision: slug '",
            "' conflicts with a system root.",
        ],
    ),
    (
        "permalink.conflict",
        &[
            "permalink_category_root_collision: slug '",
            "' trùng taxonomy '",
            "'.",
        ],
        &[
            "permalink_category_root_collision: slug '",
            "' conflicts with taxonomy '",
            "'.",
        ],
    ),
    (
        "content.the_target_content_type_in_field_group_does_not_exist_yet",
        &[
            "Loại nội dung mục tiêu '",
            "' trong field_group '",
            "' chưa tồn tại.",
        ],
        &[
            "The target content type '",
            "' in field_group '",
            "' does not exist yet.",
        ],
    ),
    (
        "entries.field_edit_forbidden_the_current_role_cannot_edit_field",
        &[
            "field_edit_forbidden: Vai trò hiện tại không có quyền sửa trường '",
            "'.",
        ],
        &[
            "field_edit_forbidden: The current role cannot edit field '",
            "'.",
        ],
    ),
    (
        "content.layout_version_conflict_for_current_version_is_v_if_match_is_v",
        &[
            "Xung đột phiên bản bố cục '",
            "': hiện tại là v",
            ", If-Match là v",
            ".",
        ],
        &[
            "Layout version conflict for '",
            "': current version is v",
            ", If-Match is v",
            ".",
        ],
    ),
    (
        "entries.unknown_field_key_field_does_not_belong_to_content_type",
        &[
            "unknown_field_key: Trường '",
            "' không thuộc loại nội dung '",
            "'.",
        ],
        &[
            "unknown_field_key: Field '",
            "' does not belong to content type '",
            "'.",
        ],
    ),
    (
        "permalink.conflict",
        &[
            "permalink_category_root_collision: slug '",
            "' trùng CPT '",
            "'.",
        ],
        &[
            "permalink_category_root_collision: slug '",
            "' conflicts with CPT '",
            "'.",
        ],
    ),
    (
        "package.non_regular_file_entries_are_not_allowed_in_plugins",
        &[
            "Không cho phép entry không phải file thường trong plugin: ",
            "",
        ],
        &["Non-regular file entries are not allowed in plugins: ", ""],
    ),
    (
        "permalink.conflict",
        &[
            "permalink_collision:đường dẫn=",
            " đích mới=",
            ":",
            " đích hiện có=archive:post",
        ],
        &[
            "permalink_collision:path=",
            " new=",
            ":",
            " existing=archive:post",
        ],
    ),
    (
        "entries.schema_version_conflict_current_schema_token_is",
        &["schema_version_conflict: schema_token hiện tại là '", "'."],
        &["schema_version_conflict: current schema_token is '", "'."],
    ),
    (
        "settings.secret_must_be_a_string_null_or",
        &[
            "Giá trị secret '",
            "' phải là chuỗi, null hoặc {\"clear\": true}.",
        ],
        &["Secret '", "' must be a string, null or {\"clear\": true}."],
    ),
    (
        "theme.non_regular_file_entries_are_not_allowed_in_themes",
        &[
            "Không cho phép entry không phải file thường trong theme: ",
            "",
        ],
        &["Non-regular file entries are not allowed in themes: ", ""],
    ),
    (
        "storage.busy",
        &["Hết thời gian chờ drain (", " tác vụ ghi vẫn đang chạy)."],
        &[
            "Timed out waiting for writers to drain (",
            " writes still running).",
        ],
    ),
    (
        "package.the_plugin_directory_exceeds_the_maximum_depth",
        &[
            "Thư mục plugin vượt quá giới hạn độ sâu tối đa (",
            "): ",
            "",
        ],
        &[
            "The plugin directory exceeds the maximum depth (",
            "): ",
            "",
        ],
    ),
    (
        "plugin.permission_denied_header_cannot_be_overridden",
        &["permission_denied: Không được phép ghi đè header '", "'."],
        &["permission_denied: Header '", "' cannot be overridden."],
    ),
    (
        "package.the_plugin_directory_contains_too_many_files_maximum_files",
        &["Thư mục plugin chứa quá nhiều file (tối đa ", " file)."],
        &[
            "The plugin directory contains too many files (maximum ",
            " files).",
        ],
    ),
    (
        "theme.the_theme_directory_exceeds_the_maximum_depth",
        &["Thư mục theme vượt quá giới hạn độ sâu tối đa (", "): ", ""],
        &["The theme directory exceeds the maximum depth (", "): ", ""],
    ),
    (
        "plugin.could_not_clean_old_staging_in_plugins_staging",
        &["Không thể dọn staging cũ trong plugins/.staging: ", ""],
        &["Could not clean old staging in plugins/.staging: ", ""],
    ),
    (
        "request.invalid_json",
        &["Không thể giải mã nội dung JSON thành kiểu đích: ", ""],
        &[
            "Failed to deserialize the JSON body into the target type: ",
            "",
        ],
    ),
    (
        "entries.invalid_time_format_for_date_or_time_missing",
        &[
            "Định dạng thời gian ",
            " không hợp lệ (thiếu ngày hoặc giờ).",
        ],
        &["Invalid time format for ", " (date or time missing)."],
    ),
    (
        "plugin.error",
        &["Plugin hết fuel (vượt quá giới hạn CPU ", " đơn vị)."],
        &["Plugin ran out of fuel (CPU limit of ", " units exceeded)."],
    ),
    (
        "permalink.conflict",
        &[
            "permalink_collision:đường dẫn=",
            " đích mới=",
            ":",
            " chủ redirect=",
            ":",
            "",
        ],
        &[
            "permalink_collision:path=",
            " new=",
            ":",
            " redirect_owner=",
            ":",
            "",
        ],
    ),
    (
        "hook.field_in_values_patch_does_not_exist_in",
        &["Trường '", "' trong values_patch không tồn tại ở '", "'."],
        &["Field '", "' in values_patch does not exist in '", "'."],
    ),
    (
        "plugin.ipc_frame_exceeds_the_limit_bytes_maximum",
        &["IPC frame vượt quá giới hạn (", " bytes, tối đa ", ")."],
        &["IPC frame exceeds the limit (", " bytes, maximum ", ")."],
    ),
    (
        "plugin.error",
        &["Plugin thiếu quyền manifest cho capability '", "'."],
        &[
            "The plugin manifest lacks permission for capability '",
            "'.",
        ],
    ),
    (
        "permalink.conflict",
        &[
            "permalink_collision:đường dẫn=",
            " đích mới=",
            ":",
            "(",
            ") đích hiện có=",
            ":",
            "(",
            ")",
        ],
        &[
            "permalink_collision:path=",
            " new=",
            ":",
            "(",
            ") existing=",
            ":",
            "(",
            ")",
        ],
    ),
    (
        "contact.only_select_fields_may_declare_options",
        &["Chỉ trường select mới được khai báo options ('", "')."],
        &["Only select fields may declare options ('", "')."],
    ),
    (
        "plugin.error",
        &["Trường '", "' không tồn tại trong schema của '", "'."],
        &["Field '", "' does not exist in the schema of '", "'."],
    ),
    (
        "plugin.error",
        &["Plugin hoặc tài khoản không có quyền ghi trường '", "'."],
        &["The plugin or account cannot write field '", "'."],
    ),
    (
        "server.error",
        &["Dữ liệu content_editor_layouts của '", "' không hợp lệ."],
        &["Invalid content_editor_layouts data for '", "'."],
    ),
    (
        "plugin.could_not_publish_the_plugin_revision_directory",
        &["Không thể publish thư mục revision plugin: ", ""],
        &["Could not publish the plugin revision directory: ", ""],
    ),
    (
        "permalink.conflict",
        &["permalink_collision:đường dẫn=", " chủ redirect=", ":", ""],
        &["permalink_collision:path=", " redirect_owner=", ":", ""],
    ),
    (
        "server.correct_syntax_opendash_path_or_path",
        &["Cú pháp đúng: opendash ", " <path> hoặc ", "<path>"],
        &["Correct syntax: opendash ", " <path> or ", "<path>"],
    ),
    (
        "package.duplicate_plugin_path_case_insensitive",
        &[
            "Đường dẫn plugin bị trùng lặp không phân biệt hoa thường: ",
            "",
        ],
        &["Duplicate plugin path (case-insensitive): ", ""],
    ),
    (
        "plugin.invalid_ipc_frame_size_bytes_maximum",
        &[
            "Kích thước IPC frame không hợp lệ (",
            " bytes, tối đa ",
            ").",
        ],
        &["Invalid IPC frame size (", " bytes, maximum ", ")."],
    ),
    (
        "request.invalid_json",
        &["Không thể phân tích cú pháp JSON của yêu cầu: ", ""],
        &["Failed to parse the request body as JSON: ", ""],
    ),
    (
        "package.path_is_not_a_valid_file_or_directory",
        &["Đường dẫn '", "' không phải file hoặc thư mục hợp lệ."],
        &["Path '", "' is not a valid file or directory."],
    ),
    (
        "package.symlinks_are_not_allowed_inside_plugins",
        &["Không cho phép symlink bên trong plugin: ", ""],
        &["Symlinks are not allowed inside plugins: ", ""],
    ),
    (
        "theme.duplicate_theme_path_case_insensitive",
        &[
            "Đường dẫn theme bị trùng lặp không phân biệt hoa thường: ",
            "",
        ],
        &["Duplicate theme path (case-insensitive): ", ""],
    ),
    (
        "plugin.could_not_create_the_plugins_revisions_directory",
        &["Không thể tạo thư mục plugins/revisions: ", ""],
        &["Could not create the plugins/revisions directory: ", ""],
    ),
    (
        "request.invalid_body",
        &["Nội dung yêu cầu không chứa UTF-8 hợp lệ: ", ""],
        &["Request body didn't contain valid UTF-8: ", ""],
    ),
    (
        "package.path_is_not_a_valid_plugin_directory",
        &["Đường dẫn '", "' không phải thư mục plugin hợp lệ."],
        &["Path '", "' is not a valid plugin directory."],
    ),
    (
        "package.file_exceeds_the_size_limit_bytes",
        &["File '", "' vượt quá giới hạn kích thước (", " bytes)."],
        &["File '", "' exceeds the size limit (", " bytes)."],
    ),
    (
        "theme.symlinks_are_not_allowed_inside_themes",
        &["Không cho phép symlink bên trong theme: ", ""],
        &["Symlinks are not allowed inside themes: ", ""],
    ),
    (
        "theme.file_exceeds_the_size_limit_bytes",
        &["File '", "' vượt quá giới hạn kích thước (", " bytes)."],
        &["File '", "' exceeds the size limit (", " bytes)."],
    ),
    (
        "theme.file_exceeds_the_size_limit_bytes",
        &["File '", "' vượt quá giới hạn kích thước (", " bytes)."],
        &["File '", "' exceeds the size limit (", " bytes)."],
    ),
    (
        "contact.select_field_must_have_1_to_options",
        &["Trường select '", "' phải có từ 1 đến ", " lựa chọn."],
        &["Select field '", "' must have 1 to ", " options."],
    ),
    (
        "plugin.could_not_read_the_plugins_staging_directory",
        &["Không thể đọc thư mục plugins/.staging: ", ""],
        &["Could not read the plugins/.staging directory: ", ""],
    ),
    (
        "plugin.could_not_create_the_plugins_staging_directory",
        &["Không thể tạo thư mục plugins/.staging: ", ""],
        &["Could not create the plugins/.staging directory: ", ""],
    ),
    (
        "permalink.permalink_invalid_claim_path_target",
        &[
            "permalink_invalid_claim: đường dẫn=",
            " đích=",
            ":",
            ":",
            "",
        ],
        &["permalink_invalid_claim:path=", " target=", ":", ":", ""],
    ),
    (
        "settings.required_secret_has_not_been_configured",
        &["Bí mật bắt buộc '", "' chưa được cấu hình."],
        &["Required secret '", "' has not been configured."],
    ),
    (
        "package.hidden_entries_are_not_allowed_in_plugins",
        &["Không cho phép entry ẩn trong plugin: ", ""],
        &["Hidden entries are not allowed in plugins: ", ""],
    ),
    (
        "plugin.could_not_create_the_plugin_staging_directory",
        &["Không thể tạo thư mục staging plugin: ", ""],
        &["Could not create the plugin staging directory: ", ""],
    ),
    (
        "server.error",
        &["Một phiên OpenDash khác đang giữ khóa ", ""],
        &["Another OpenDash instance already holds lock ", ""],
    ),
    (
        "entries.the_day_in_is_outside_the_valid_range",
        &["Ngày trong ", " nằm ngoài phạm vi hợp lệ."],
        &["The day in ", " is outside the valid range."],
    ),
    (
        "theme.hidden_entries_are_not_allowed_in_themes",
        &["Không cho phép entry ẩn trong theme: ", ""],
        &["Hidden entries are not allowed in themes: ", ""],
    ),
    (
        "plugin.could_not_initialize_the_plugins_directory",
        &["Không thể khởi tạo thư mục plugins/: ", ""],
        &["Could not initialize the plugins/ directory: ", ""],
    ),
    (
        "theme.could_not_access_theme_directory",
        &["Không thể truy cập thư mục theme '", "': ", ""],
        &["Could not access theme directory '", "': ", ""],
    ),
    (
        "seo.invalid_text",
        &["Tiêu đề SEO vượt quá giới hạn ", " ký tự."],
        &["SEO title exceeds the limit of ", " characters."],
    ),
    (
        "permalink.conflict",
        &["permalink_collision:đường dẫn=", " đích hiện có=", ":", ""],
        &["permalink_collision:path=", " existing=", ":", ""],
    ),
    (
        "permalink.taxonomy_is_missing_or_not_public",
        &["Taxonomy không tồn tại/không public: ", ""],
        &["Taxonomy is missing or not public: ", ""],
    ),
    (
        "seo.invalid_text",
        &["Mô tả SEO vượt quá giới hạn ", " ký tự."],
        &["SEO description exceeds the limit of ", " characters."],
    ),
    (
        "entries.invalid_hours_minutes_or_seconds_in",
        &["Giờ phút giây trong ", " không hợp lệ."],
        &["Invalid hours, minutes or seconds in ", "."],
    ),
    (
        "plugin.could_not_read_the_ipc_frame_contents",
        &["Không thể đọc nội dung IPC frame: ", ""],
        &["Could not read the IPC frame contents: ", ""],
    ),
    (
        "plugin.could_not_traverse_plugins_staging",
        &["Không thể duyệt plugins/.staging: ", ""],
        &["Could not traverse plugins/.staging: ", ""],
    ),
    (
        "request.invalid_query",
        &["Không thể giải mã chuỗi truy vấn: ", ""],
        &["Failed to deserialize query string: ", ""],
    ),
    (
        "plugin.error",
        &["Manifest trong database không hợp lệ: ", ""],
        &["Invalid manifest in the database: ", ""],
    ),
    (
        "server.error",
        &["Không thể serialize layout_json: ", ""],
        &["Could not serialize layout_json: ", ""],
    ),
    (
        "settings.required_secret_cannot_be_deleted",
        &["Không thể xóa bí mật bắt buộc '", "'."],
        &["Required secret '", "' cannot be deleted."],
    ),
    (
        "backup.setrlimit_rlimit_as_failed_with",
        &["setrlimit(RLIMIT_AS) thất bại với mã ", ""],
        &["setrlimit(RLIMIT_AS) failed with ", ""],
    ),
    (
        "contact.field_key_is_reserved_by_the_system",
        &["Mã trường '", "' là từ khóa hệ thống."],
        &["Field key '", "' is reserved by the system."],
    ),
    (
        "contact.an_option_in_field_is_invalid",
        &["Lựa chọn trong trường '", "' không hợp lệ."],
        &["An option in field '", "' is invalid."],
    ),
    (
        "plugin.could_not_sync_all_plugin_file",
        &["Không thể sync_all file plugin ", ": ", ""],
        &["Could not sync_all plugin file ", ": ", ""],
    ),
    (
        "request.invalid_form",
        &["Không thể giải mã nội dung biểu mẫu: ", ""],
        &["Failed to deserialize form body: ", ""],
    ),
    (
        "plugin.error",
        &["Lỗi đọc WorkerInvocationRequest: ", ""],
        &["Error reading WorkerInvocationRequest: ", ""],
    ),
    (
        "package.could_not_check_metadata_for",
        &["Không thể kiểm tra metadata '", "': ", ""],
        &["Could not check metadata for '", "': ", ""],
    ),
    (
        "plugin.could_not_read_the_ipc_frame_length",
        &["Không thể đọc độ dài IPC frame: ", ""],
        &["Could not read the IPC frame length: ", ""],
    ),
    (
        "request.invalid_body",
        &["Không thể đọc nội dung yêu cầu: ", ""],
        &["Failed to buffer the request body: ", ""],
    ),
    (
        "backup.missing_theme_revision_directory",
        &["Thiếu thư mục theme revision ", "/", "."],
        &["Missing theme revision directory ", "/", "."],
    ),
    (
        "permalink.invalid_taxonomy_rewrite_base",
        &["Taxonomy rewrite_base không hợp lệ: ", ""],
        &["Invalid taxonomy rewrite_base: ", ""],
    ),
    (
        "contact.select_field_is_missing_options",
        &["Trường select '", "' thiếu options."],
        &["Select field '", "' is missing options."],
    ),
    (
        "plugin.error",
        &["Không tìm thấy export opendash_run: ", ""],
        &["opendash_run export not found: ", ""],
    ),
    (
        "permalink.cpt_is_missing_or_not_public",
        &["CPT không tồn tại/không public: ", ""],
        &["CPT is missing or not public: ", ""],
    ),
    (
        "contact.the_form_must_contain_1_to_fields",
        &["Form phải có từ 1 đến ", " trường."],
        &["The form must contain 1 to ", " fields."],
    ),
    (
        "plugin.error_updating_plugin_status",
        &["Lỗi cập nhật trạng thái plugin: ", ""],
        &["Error updating plugin status: ", ""],
    ),
    (
        "media.media_asset_does_not_exist",
        &["Media asset '", "' không tồn tại."],
        &["Media asset '", "' does not exist."],
    ),
    (
        "server.missing_path_argument_for",
        &["Thiếu đối số đường dẫn cho '", "'."],
        &["Missing path argument for '", "'."],
    ),
    (
        "theme.the_theme_zip_exceeds_bytes",
        &["File ZIP theme vượt quá giới hạn ", " bytes."],
        &["The theme ZIP exceeds ", " bytes."],
    ),
    (
        "contact.duplicate_option_in_field",
        &["Lựa chọn trong trường '", "' bị trùng lặp."],
        &["Duplicate option in field '", "'."],
    ),
    (
        "plugin.error_saving_plugin_to_the_database",
        &["Lỗi lưu plugin vào database: ", ""],
        &["Error saving plugin to the database: ", ""],
    ),
    (
        "permalink.invalid_custom_path_entry",
        &["invalid_custom_path:entry:", ": ", ""],
        &["invalid_custom_path:entry:", ": ", ""],
    ),
    (
        "permalink.invalid_custom_path_entry",
        &["invalid_custom_path:entry:", ": ", ""],
        &["invalid_custom_path:entry:", ": ", ""],
    ),
    (
        "package.error_reading_a_plugin_directory_item",
        &["Lỗi đọc mục thư mục plugin: ", ""],
        &["Error reading a plugin directory item: ", ""],
    ),
    (
        "plugin.could_not_create_plugin_file",
        &["Không thể tạo file plugin ", ": ", ""],
        &["Could not create plugin file ", ": ", ""],
    ),
    (
        "plugin.could_not_write_plugin_file",
        &["Không thể ghi file plugin ", ": ", ""],
        &["Could not write plugin file ", ": ", ""],
    ),
    (
        "plugin.error_deleting_plugin_projections",
        &["Lỗi xóa plugin projections: ", ""],
        &["Error deleting plugin projections: ", ""],
    ),
    (
        "request.invalid_form",
        &["Không thể giải mã biểu mẫu: ", ""],
        &["Failed to deserialize form: ", ""],
    ),
    (
        "server.error",
        &["Lỗi I/O của bộ điều phối site: ", ""],
        &["Site coordinator I/O error: ", ""],
    ),
    (
        "plugin.error",
        &["Module WebAssembly không hợp lệ: ", ""],
        &["Invalid WebAssembly module: ", ""],
    ),
    (
        "plugin.error",
        &["Plugin kết thúc với mã lỗi ", "."],
        &["Plugin exited with error code ", "."],
    ),
    (
        "hook.before_save_veto",
        &["Không thể tìm subscription: ", ""],
        &["Subscription discovery failed: ", ""],
    ),
    (
        "theme.path_is_not_a_directory",
        &["Đường dẫn '", "' không phải thư mục."],
        &["Path '", "' is not a directory."],
    ),
    (
        "contact.invalid_label_for_field",
        &["Nhãn trường '", "' không hợp lệ."],
        &["Invalid label for field '", "'."],
    ),
    (
        "package.could_not_read_zip_file",
        &["Không thể đọc file ZIP '", "': ", ""],
        &["Could not read ZIP file '", "': ", ""],
    ),
    (
        "plugin.error_writing_ipc_frame_data",
        &["Lỗi ghi dữ liệu IPC frame: ", ""],
        &["Error writing IPC frame data: ", ""],
    ),
    (
        "plugin.database_connection_error",
        &["Lỗi kết nối cơ sở dữ liệu: ", ""],
        &["Database connection error: ", ""],
    ),
    (
        "theme.error_reading_a_theme_directory_item",
        &["Lỗi đọc mục thư mục theme: ", ""],
        &["Error reading a theme directory item: ", ""],
    ),
    (
        "server.error",
        &["Thiếu extension của yêu cầu: ", ""],
        &["Missing request extension: ", ""],
    ),
    (
        "server.error",
        &["Schema form lỗi trong DB: ", ""],
        &["Invalid form schema in the database: ", ""],
    ),
    (
        "server.error",
        &["Snapshot schema form lỗi: ", ""],
        &["Invalid form schema snapshot: ", ""],
    ),
    (
        "content.content_type_not_found",
        &["Không tìm thấy loại nội dung '", "'."],
        &["Content type '", "' not found."],
    ),
    (
        "permalink.invalid_cpt_rewrite_base",
        &["CPT rewrite_base không hợp lệ: ", ""],
        &["Invalid CPT rewrite_base: ", ""],
    ),
    (
        "package.could_not_open_zip_file",
        &["Không thể mở file ZIP '", "': ", ""],
        &["Could not open ZIP file '", "': ", ""],
    ),
    (
        "package.could_not_read_directory",
        &["Không thể đọc thư mục '", "': ", ""],
        &["Could not read directory '", "': ", ""],
    ),
    (
        "plugin.error_parsing_ipc_frame_json",
        &["Lỗi parse IPC frame JSON: ", ""],
        &["Error parsing IPC frame JSON: ", ""],
    ),
    (
        "plugin.error_writing_the_ipc_frame_length",
        &["Lỗi ghi độ dài IPC frame: ", ""],
        &["Error writing the IPC frame length: ", ""],
    ),
    (
        "plugin.error",
        &["Output không phải JSON hợp lệ: ", ""],
        &["Output is not valid JSON: ", ""],
    ),
    (
        "plugin.error_serializing_the_ipc_frame",
        &["Lỗi serialize IPC frame: ", ""],
        &["Error serializing the IPC frame: ", ""],
    ),
    (
        "request.invalid_json",
        &["Dữ liệu đầu vào không phải JSON hợp lệ: ", ""],
        &["Input is not valid JSON: ", ""],
    ),
    (
        "server.error",
        &["Dữ liệu submission lỗi: ", ""],
        &["Invalid submission data: ", ""],
    ),
    (
        "plugin.error",
        &["Lỗi serialize manifest: ", ""],
        &["Error serializing the manifest: ", ""],
    ),
    (
        "server.error",
        &["Không thể cách ly tệp: ", ""],
        &["Could not quarantine the file: ", ""],
    ),
    (
        "contact.duplicate_field_key",
        &["Mã trường '", "' bị trùng lặp."],
        &["Duplicate field key '", "'."],
    ),
    (
        "package.could_not_read_file",
        &["Không thể đọc file '", "': ", ""],
        &["Could not read file '", "': ", ""],
    ),
    (
        "plugin.error",
        &["Dữ liệu đầu ra từ plugin không hợp lệ: ", ""],
        &["Invalid plugin output: ", ""],
    ),
    (
        "backup.missing_referenced_file",
        &["Thiếu tệp tham chiếu: ", ""],
        &["Missing referenced file: ", ""],
    ),
    (
        "permalink.invalid_taxonomy_key",
        &["Taxonomy key không hợp lệ: ", ""],
        &["Invalid taxonomy key: ", ""],
    ),
    (
        "server.missing_path_after",
        &["Thiếu đường dẫn sau '", "'."],
        &["Missing path after '", "'."],
    ),
    (
        "package.could_not_open_file",
        &["Không thể mở file '", "': ", ""],
        &["Could not open file '", "': ", ""],
    ),
    (
        "plugin.error_querying_plugins",
        &["Lỗi truy vấn plugins: ", ""],
        &["Error querying plugins: ", ""],
    ),
    (
        "contact.invalid_field_key",
        &["Mã trường '", "' không hợp lệ."],
        &["Invalid field key '", "'."],
    ),
    (
        "plugin.error_flushing_the_ipc_frame",
        &["Lỗi flush IPC frame: ", ""],
        &["Error flushing the IPC frame: ", ""],
    ),
    (
        "plugin.error_querying_the_plugin",
        &["Lỗi truy vấn plugin: ", ""],
        &["Error querying the plugin: ", ""],
    ),
    (
        "plugin.error",
        &["Lỗi serialize input: ", ""],
        &["Error serializing input: ", ""],
    ),
    (
        "plugin.error",
        &["Lỗi khởi tạo module: ", ""],
        &["Module initialization error: ", ""],
    ),
    (
        "plugin.error",
        &["Lỗi máy chủ nội bộ: ", ""],
        &["Internal server error: ", ""],
    ),
    (
        "plugin.error",
        &["Không thể cấp fuel: ", ""],
        &["Could not allocate fuel: ", ""],
    ),
    (
        "request.invalid_json",
        &["Dữ liệu JSON không hợp lệ: ", ""],
        &["Invalid JSON data: ", ""],
    ),
    (
        "entries.invalid_minute_in",
        &["Phút trong ", " không hợp lệ."],
        &["Invalid minute in ", "."],
    ),
    (
        "entries.invalid_second_in",
        &["Giây trong ", " không hợp lệ."],
        &["Invalid second in ", "."],
    ),
    (
        "entries.invalid_month_in",
        &["Tháng trong ", " không hợp lệ."],
        &["Invalid month in ", "."],
    ),
    (
        "plugin.error",
        &["Manifest không hợp lệ: ", ""],
        &["Invalid manifest: ", ""],
    ),
    (
        "entries.invalid_year_in",
        &["Năm trong ", " không hợp lệ."],
        &["Invalid year in ", "."],
    ),
    (
        "entries.invalid_hour_in",
        &["Giờ trong ", " không hợp lệ."],
        &["Invalid hour in ", "."],
    ),
    (
        "permalink.invalid_cpt_key",
        &["CPT key không hợp lệ: ", ""],
        &["Invalid CPT key: ", ""],
    ),
    (
        "entries.invalid_day_in",
        &["Ngày trong ", " không hợp lệ."],
        &["Invalid day in ", "."],
    ),
    (
        "plugin.error",
        &["Lỗi cơ sở dữ liệu: ", ""],
        &["Database error: ", ""],
    ),
    (
        "request.invalid_path",
        &["URL không hợp lệ: ", ""],
        &["Invalid URL: ", ""],
    ),
    (
        "plugin.error",
        &["Lỗi thực thi guest: ", ""],
        &["Guest trap: ", ""],
    ),
    (
        "plugin.error",
        &["Lỗi linker: ", ""],
        &["Linker error: ", ""],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_encoding_falls_back_to_english() {
        for locale in ["en", "ja", "ar", "", "unknown"] {
            assert_eq!(encode_ui_locale(locale), 1);
        }
        assert_eq!(encode_ui_locale(" VI "), 2);
    }

    #[test]
    fn exact_table_has_valid_codes_and_bidirectional_coverage() {
        for &(code, vi, en) in EXACT_ERROR_ENTRIES {
            assert!(!code.is_empty() && code.contains('.'));
            assert!(
                code.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._".contains(&b)),
                "{code}"
            );
            for raw in [vi, en] {
                for locale in ["en", "vi"] {
                    assert_ne!(
                        translate_api_message(raw, locale).code,
                        UNKNOWN_ERROR_CODE,
                        "{raw}"
                    );
                }
            }
        }
    }

    // Inspect the actual Rust sources rather than only verifying that the
    // dictionary is internally consistent. Direct literal/format! messages
    // are the mechanically enumerable subset; indirect error values and
    // dependency rejections still need dedicated tests and review.
    #[test]
    fn direct_api_error_messages_in_source_are_covered() {
        use std::path::{Path, PathBuf};

        fn visit(path: &Path, count: &mut usize, missing: &mut Vec<String>) {
            if path.is_dir() {
                for item in std::fs::read_dir(path).unwrap() {
                    visit(&item.unwrap().path(), count, missing);
                }
                return;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs")
                || path.file_name().and_then(|n| n.to_str()) == Some("i18n.rs")
            {
                return;
            }
            let source = std::fs::read_to_string(path).unwrap();
            let constructors = [
                "bad_request",
                "unauthorized",
                "forbidden",
                "conflict",
                "not_found",
                "precondition_failed",
                "precondition_required",
                "unsupported_media_type",
                "internal",
                "unprocessable",
                "too_many_requests",
                "service_unavailable",
            ];
            for suffix in source.split("ApiError::").skip(1) {
                let Some((constructor, args)) = suffix.split_once('(') else {
                    continue;
                };
                if !constructors.contains(&constructor) {
                    continue;
                }
                let args = args.trim_start();
                let (args, dynamic) = if let Some(rest) = args.strip_prefix("format!(") {
                    (rest.trim_start(), true)
                } else {
                    (args, false)
                };
                let Some(mut raw) = read_literal(args) else {
                    continue;
                };
                if dynamic {
                    // The label placeholders in these validators are only
                    // used with a known set of system labels.
                    if raw.contains("{label}") {
                        let label = if path.file_name().unwrap() == "seo.rs" {
                            "Tiêu đề SEO"
                        } else {
                            "from_address"
                        };
                        raw = raw.replace("{label}", label);
                    }
                    raw = sample_format(&raw);
                }
                *count += 1;
                for locale in ["en", "vi"] {
                    if translate_api_message(&raw, locale).code == UNKNOWN_ERROR_CODE {
                        missing.push(format!("{}: {raw:?} ({locale})", path.display()));
                    }
                }
            }
        }

        fn read_literal(input: &str) -> Option<String> {
            let mut chars = input.strip_prefix('"')?.chars();
            let mut output = String::new();
            while let Some(ch) = chars.next() {
                match ch {
                    '"' => return Some(output),
                    '\\' => match chars.next()? {
                        '"' => output.push('"'),
                        '\\' => output.push('\\'),
                        'n' => output.push('\n'),
                        'r' => output.push('\r'),
                        't' => output.push('\t'),
                        '0' => output.push('\0'),
                        other => panic!("Unsupported escape in error literal: {other}"),
                    },
                    _ => output.push(ch),
                }
            }
            None
        }

        fn sample_format(raw: &str) -> String {
            let mut output = String::new();
            let mut rest = raw;
            let mut index = 0;
            while let Some(pos) = rest.find(['{', '}']) {
                output.push_str(&rest[..pos]);
                rest = &rest[pos..];
                if rest.starts_with("{{") || rest.starts_with("}}") {
                    output.push(rest.as_bytes()[0] as char);
                    rest = &rest[2..];
                } else if let Some(close) = rest.find('}') {
                    output.push_str(&format!("sample_{index}"));
                    index += 1;
                    rest = &rest[close + 1..];
                } else {
                    panic!("Malformed error format literal {raw:?}");
                }
            }
            output.push_str(rest);
            output
        }

        let root = option_env!("CARGO_MANIFEST_DIR")
            .map(|manifest| PathBuf::from(manifest).join("src"))
            .unwrap_or_else(|| PathBuf::from(file!()).parent().unwrap().to_path_buf());
        let mut count = 0;
        let mut missing = Vec::new();
        visit(&root, &mut count, &mut missing);
        assert!(count >= 1200, "Only {count} direct error sites scanned");
        assert!(
            missing.is_empty(),
            "{} uncovered messages:\n{}",
            missing.len(),
            missing.join("\n")
        );
        eprintln!("Covered {count} direct ApiError literal/format! call sites.");
    }

    #[test]
    fn template_table_preserves_parameters() {
        for &(code, vi, en) in TEMPLATE_MATCHERS {
            assert_eq!(vi.len(), en.len(), "{code}");
            let values: Vec<_> = (0..vi.len() - 1).map(|i| format!("sample_{i}")).collect();
            let render = |parts: &[&str]| {
                let mut msg = parts[0].to_owned();
                for (value, suffix) in values.iter().zip(&parts[1..]) {
                    msg.push_str(value);
                    msg.push_str(suffix);
                }
                msg
            };
            let vi = render(vi);
            let en = render(en);
            assert_eq!(translate_api_message(&vi, "en").message, en, "{code}");
            assert_eq!(translate_api_message(&en, "vi").message, vi, "{code}");
        }
    }

    #[test]
    fn unknown_user_message_remains_unchanged() {
        for raw in [
            "Nội dung khách hàng tự nhập — custom plugin message",
            "Đơn hàng tự tạo không hợp lệ.",
            "Invalid custom user content.",
        ] {
            for locale in ["en", "vi"] {
                let result = translate_api_message(raw, locale);
                assert_eq!(result.code, UNKNOWN_ERROR_CODE);
                assert_eq!(result.message, raw);
            }
        }
    }
}
const MAX_LOCALIZED_ERROR_BYTES: usize = 1_048_576;

fn is_localized_admin_path(path: &str) -> bool {
    path == "/admin"
        || path.starts_with("/admin/")
        || path == "/builder"
        || path.starts_with("/builder/")
        || path.starts_with("/auth/")
        || path.starts_with("/api/admin/")
        || path.starts_with("/api/themes/")
}

async fn effective_locale_for_error(
    db: &sqlx::SqlitePool,
    ui_locale: &std::sync::atomic::AtomicU8,
) -> &'static str {
    use std::sync::atomic::Ordering;
    let mut cached = ui_locale.load(Ordering::Acquire);
    if cached == 0
        && let Some(mut conn) = db.try_acquire()
        && let Ok(configured) = crate::storage::settings::get_cms_locale_in_conn(&mut conn).await
    {
        let encoded = encode_ui_locale(&configured);
        cached = match ui_locale.compare_exchange(0, encoded, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => encoded,
            Err(current) => current,
        };
    }
    if cached == 2 { "vi" } else { "en" }
}

fn status_error_code(status: axum::http::StatusCode) -> &'static str {
    use axum::http::StatusCode as S;
    match status {
        S::UNAUTHORIZED => "auth.unauthorized",
        S::FORBIDDEN => "auth.forbidden",
        S::METHOD_NOT_ALLOWED => "request.method_not_allowed",
        S::PAYLOAD_TOO_LARGE => "request.body_too_large",
        S::UNSUPPORTED_MEDIA_TYPE => "request.unsupported_media_type",
        S::PRECONDITION_REQUIRED => "request.if_match_required",
        S::PRECONDITION_FAILED => "request.precondition_failed",
        S::SERVICE_UNAVAILABLE => "storage.busy",
        _ => "server.error",
    }
}

fn empty_error_message(status: axum::http::StatusCode, guard: bool) -> &'static str {
    use axum::http::StatusCode as S;
    match (status, guard) {
        (S::BAD_REQUEST, true) => "Invalid Host header.",
        (S::FORBIDDEN, true) => "Origin không hợp lệ.",
        (S::METHOD_NOT_ALLOWED, _) => "Method Not Allowed",
        (S::PAYLOAD_TOO_LARGE, _) => "length limit exceeded",
        (S::UNSUPPORTED_MEDIA_TYPE, _) => "Unsupported Media Type",
        _ => status.canonical_reason().unwrap_or("Server error"),
    }
}

fn localize_json_error(value: &mut serde_json::Value, locale: &str) -> Option<&'static str> {
    let obj = value.as_object_mut()?;
    let is_veto = obj
        .get("field_errors")
        .is_some_and(serde_json::Value::is_object)
        && obj.get("code").is_some_and(serde_json::Value::is_string);
    let is_plugin = obj.contains_key("correlation_id") && obj.contains_key("success");
    let mut code = None;
    for key in ["error", "message"] {
        if let Some(raw) = obj.get(key).and_then(serde_json::Value::as_str) {
            let localized = translate_api_message(raw, locale);
            if localized.code != "server.unknown" {
                code.get_or_insert(localized.code);
                obj.insert(key.to_owned(), serde_json::Value::String(localized.message));
            }
        }
    }
    if let Some(fields) = obj
        .get_mut("field_errors")
        .and_then(serde_json::Value::as_object_mut)
    {
        for value in fields.values_mut() {
            if let Some(raw) = value.as_str() {
                let localized = translate_api_message(raw, locale);
                if localized.code != "server.unknown" {
                    *value = serde_json::Value::String(localized.message);
                }
            }
        }
    }
    if is_plugin
        && let Some(data) = obj.get_mut("data")
        && data
            .get("field_errors")
            .is_some_and(serde_json::Value::is_object)
        && data.get("code").is_some_and(serde_json::Value::is_string)
    {
        let _ = localize_json_error(data, locale);
    }
    if is_veto {
        Some("hook.before_save_veto")
    } else if is_plugin {
        code.or(Some("plugin.error"))
    } else {
        code
    }
}

// An oversized, failed or trailer-bearing stream must not lose bytes that have
// already been polled. Replay those frames before the untouched remainder.
struct ErrorBodyReplay {
    frames: std::collections::VecDeque<Result<http_body::Frame<axum::body::Bytes>, axum::Error>>,
    rest: axum::body::Body,
}

impl http_body::Body for ErrorBodyReplay {
    type Data = axum::body::Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        if let Some(frame) = self.frames.pop_front() {
            return std::task::Poll::Ready(Some(frame));
        }
        std::pin::Pin::new(&mut self.rest).poll_frame(cx)
    }
}

async fn read_bounded_error_body(
    mut body: axum::body::Body,
) -> Result<axum::body::Bytes, axum::body::Body> {
    use http_body::Body as _;
    if body
        .size_hint()
        .upper()
        .is_some_and(|n| n > MAX_LOCALIZED_ERROR_BYTES as u64)
    {
        return Err(body);
    }
    let mut frames = std::collections::VecDeque::new();
    let mut total = 0usize;
    while let Some(frame) =
        std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await
    {
        let data_len = frame
            .as_ref()
            .ok()
            .and_then(http_body::Frame::data_ref)
            .map(|b| b.len());
        frames.push_back(frame);
        if let Some(len) = data_len
            && len <= MAX_LOCALIZED_ERROR_BYTES.saturating_sub(total)
        {
            total += len;
        } else {
            return Err(axum::body::Body::new(ErrorBodyReplay {
                frames,
                rest: body,
            }));
        }
    }
    let mut bytes = Vec::with_capacity(total);
    for frame in frames {
        if let Ok(frame) = frame
            && let Ok(data) = frame.into_data()
        {
            bytes.extend_from_slice(&data);
        }
    }
    Ok(bytes.into())
}

pub(crate) async fn localize_error_response(
    axum::extract::State(state): axum::extract::State<crate::app::AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{Method, StatusCode, header};
    let method = request.method().clone();
    let admin = is_localized_admin_path(request.uri().path());
    let mut response = next.run(request).await;
    let status = response.status();
    if !status.is_client_error() && !status.is_server_error() {
        return response;
    }
    let guard = response
        .extensions()
        .get::<crate::app::HostOriginRejected>()
        .is_some();
    if !guard && !admin {
        return response;
    }
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if content_type == "text/html"
        || response
            .headers()
            .get(header::CONTENT_ENCODING)
            .is_some_and(|v| v.as_bytes() != b"identity")
    {
        return response;
    }
    let error_header = header::HeaderName::from_static("x-opendash-error-code");
    let fallback_code = if guard {
        if status == StatusCode::BAD_REQUEST {
            "request.bad_host"
        } else {
            "request.bad_origin"
        }
    } else {
        status_error_code(status)
    };
    response.headers_mut().insert(
        error_header.clone(),
        header::HeaderValue::from_static(fallback_code),
    );
    if method == Method::HEAD {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let original = match read_bounded_error_body(body).await {
        Ok(bytes) => bytes,
        Err(body) => return axum::response::Response::from_parts(parts, body),
    };
    let locale = effective_locale_for_error(&state.db, &state.ui_locale).await;
    let (replacement, code) = if original.is_empty() {
        let translated = translate_api_message(empty_error_message(status, guard), locale);
        parts.headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        (Some(translated.message.into_bytes()), translated.code)
    } else if content_type == "application/json" {
        match serde_json::from_slice::<serde_json::Value>(&original) {
            Ok(mut value) => {
                let before = value.clone();
                let code = localize_json_error(&mut value, locale);
                let replacement = if value != before {
                    serde_json::to_vec(&value).ok()
                } else {
                    None
                };
                (replacement, code.unwrap_or(fallback_code))
            }
            Err(_) => (None, fallback_code),
        }
    } else if let Ok(raw) = std::str::from_utf8(&original) {
        let translated = translate_api_message(raw, locale);
        (Some(translated.message.into_bytes()), translated.code)
    } else {
        (None, fallback_code)
    };
    let code = if guard || code == "server.unknown" {
        fallback_code
    } else {
        code
    };
    parts
        .headers
        .insert(error_header, header::HeaderValue::from_static(code));
    if let Some(bytes) = replacement
        && bytes.as_slice() != original.as_ref()
    {
        parts.headers.remove(header::CONTENT_LENGTH);
        parts.headers.remove(header::TRANSFER_ENCODING);
        parts.headers.insert(
            header::CONTENT_LENGTH,
            header::HeaderValue::from(bytes.len()),
        );
        return axum::response::Response::from_parts(parts, axum::body::Body::from(bytes));
    }
    axum::response::Response::from_parts(parts, axum::body::Body::from(original))
}

#[cfg(test)]
mod localization_middleware_tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use serde_json::json;
    use std::sync::atomic::{AtomicU8, Ordering};

    #[test]
    fn admin_scope_is_segment_bounded() {
        for path in [
            "/admin",
            "/admin/assets/x",
            "/builder",
            "/builder/x",
            "/auth/me",
            "/api/admin/settings/general",
            "/api/themes/a/options",
        ] {
            assert!(is_localized_admin_path(path), "{path}");
        }
        for path in [
            "/",
            "/admin-guide",
            "/builder-info",
            "/theme/a",
            "/api/public/contact/1",
            "/robots.txt",
            "/sitemap.xml",
            "/pageant",
        ] {
            assert!(!is_localized_admin_path(path), "{path}");
        }
    }

    #[test]
    fn json_errors_preserve_veto_metadata_and_unrecognized_plugin_text() {
        let mut veto = json!({
            "code": "plugin.validation",
            "message": "Cần đăng nhập.",
            "field_errors": {"known": "Cần đăng nhập.", "custom": "My custom error #47"}
        });
        let original = veto.clone();
        assert_eq!(
            localize_json_error(&mut veto, "en"),
            Some("hook.before_save_veto")
        );
        assert_ne!(veto["message"], original["message"]);
        assert_eq!(veto["field_errors"]["known"], veto["message"]);
        assert_eq!(
            veto["field_errors"]["custom"],
            original["field_errors"]["custom"]
        );
        assert_eq!(veto["code"], original["code"]);
        assert_eq!(
            localize_json_error(&mut veto, "vi"),
            Some("hook.before_save_veto")
        );
        assert_eq!(veto, original);

        let mut plugin = json!({
            "success": false, "error": "My custom error #47",
            "code": "PLUGIN_VALIDATION", "correlation_id": "unchanged",
            "data": original, "conflicts": [{"path": "/test"}]
        });
        let original = plugin.clone();
        assert_eq!(localize_json_error(&mut plugin, "en"), Some("plugin.error"));
        assert_ne!(plugin["data"]["message"], original["data"]["message"]);
        for key in ["success", "error", "code", "correlation_id", "conflicts"] {
            assert_eq!(plugin[key], original[key]);
        }
    }

    #[tokio::test]
    async fn error_body_limit_preserves_oversized_content() {
        let original = vec![b'x'; MAX_LOCALIZED_ERROR_BYTES + 1];
        let body = read_bounded_error_body(Body::from(original.clone()))
            .await
            .expect_err("oversized body must not be buffered");
        let actual = to_bytes(body, original.len()).await.unwrap();
        assert_eq!(actual.as_ref(), original.as_slice());
        let exact = vec![b'y'; MAX_LOCALIZED_ERROR_BYTES];
        let actual = read_bounded_error_body(Body::from(exact.clone()))
            .await
            .unwrap();
        assert_eq!(actual.as_ref(), exact.as_slice());
    }

    #[tokio::test]
    async fn error_body_limit_buffers_unsized_streams_until_cap() {
        struct UnsizedChunkBody {
            chunks: std::collections::VecDeque<axum::body::Bytes>,
        }
        impl http_body::Body for UnsizedChunkBody {
            type Data = axum::body::Bytes;
            type Error = std::convert::Infallible;

            fn poll_frame(
                mut self: std::pin::Pin<&mut Self>,
                _cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>>
            {
                std::task::Poll::Ready(
                    self.chunks
                        .pop_front()
                        .map(|b| Ok(http_body::Frame::data(b))),
                )
            }
        }

        let small_msg = axum::body::Bytes::from_static("Cần đăng nhập.".as_bytes());
        let small = Body::new(UnsizedChunkBody {
            chunks: std::collections::VecDeque::from([small_msg.clone()]),
        });
        assert!(http_body::Body::size_hint(&small).upper().is_none());
        let actual = read_bounded_error_body(small)
            .await
            .expect("unsized error stream within cap must still be localized");
        assert_eq!(actual.as_ref(), small_msg.as_ref());

        let half = axum::body::Bytes::from(vec![b'a'; (MAX_LOCALIZED_ERROR_BYTES / 2) + 1]);
        let oversized = Body::new(UnsizedChunkBody {
            chunks: std::collections::VecDeque::from([half.clone(), half.clone()]),
        });
        let replay = read_bounded_error_body(oversized)
            .await
            .expect_err("unsized stream exceeding cap must replay unread frames");
        let replayed = to_bytes(replay, MAX_LOCALIZED_ERROR_BYTES + 16)
            .await
            .unwrap();
        assert_eq!(replayed.len(), half.len() * 2);
        assert!(replayed.iter().all(|&b| b == b'a'));
    }

    #[tokio::test]
    async fn unset_locale_does_not_wait_for_exhausted_pool() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let _held = db.acquire().await.unwrap();
        let cache = AtomicU8::new(0);
        let locale = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            effective_locale_for_error(&db, &cache),
        )
        .await
        .expect("503 must never wait for a pooled connection");
        assert_eq!(locale, "en");
        assert_eq!(cache.load(Ordering::Acquire), 0);
        cache.store(2, Ordering::Release);
        assert_eq!(effective_locale_for_error(&db, &cache).await, "vi");
    }
}
