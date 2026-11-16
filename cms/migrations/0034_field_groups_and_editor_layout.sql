-- 0034: Field groups, target association tombstones, editor layouts, and content_types version column (M7-S09-T01)

-- 1. Bổ sung cột version vào bảng content_types hiện có để hỗ trợ CAS ETag ("v<version>")
ALTER TABLE content_types
    ADD COLUMN version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1);

-- 2. Bảng lưu định nghĩa nhóm custom fields (mở rộng cho post, page, và mọi CPT).
CREATE TABLE IF NOT EXISTS content_field_groups (
    group_id        TEXT PRIMARY KEY NOT NULL COLLATE BINARY,
    owner_kind      TEXT NOT NULL CHECK (owner_kind IN ('user', 'theme', 'plugin')),
    owner_id        TEXT,
    local_key       TEXT NOT NULL CHECK (length(local_key) BETWEEN 1 AND 64),
    title           TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 100),
    description     TEXT NOT NULL DEFAULT '',
    fields_json     TEXT NOT NULL
                    CHECK (json_valid(fields_json) AND json_type(fields_json) = 'array'),
    default_region  TEXT NOT NULL DEFAULT 'main_bottom'
                    CHECK (default_region IN ('main_top', 'main_primary', 'main_bottom', 'sidebar')),
    default_order   INTEGER NOT NULL DEFAULT 50,
    collapsible     INTEGER NOT NULL DEFAULT 1 CHECK (collapsible IN (0, 1)),
    status          TEXT NOT NULL DEFAULT 'active'
                    CHECK (status IN ('active', 'provider_inactive', 'withdrawn')),
    version         INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (
        (owner_kind = 'user' AND owner_id IS NULL)
        OR (owner_kind IN ('theme', 'plugin') AND owner_id IS NOT NULL AND length(owner_id) > 0)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_content_field_groups_owner_local
    ON content_field_groups(owner_kind, COALESCE(owner_id, ''), local_key);

-- 3. Bảng liên kết giữa nhóm field và từng type_key (bảo toàn association tombstone khi gỡ target — 270-R1).
CREATE TABLE IF NOT EXISTS content_field_group_targets (
    group_id          TEXT NOT NULL REFERENCES content_field_groups(group_id) ON DELETE RESTRICT,
    type_key          TEXT NOT NULL REFERENCES content_types(key) ON DELETE RESTRICT,
    association_state TEXT NOT NULL DEFAULT 'declared'
                      CHECK (association_state IN ('declared', 'withdrawn')),
    created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (group_id, type_key)
);

CREATE INDEX IF NOT EXISTS idx_content_field_group_targets_type
    ON content_field_group_targets(type_key, association_state);

-- 4. Bảng lưu cấu hình bố cục màn hình chỉnh sửa (EditorLayoutConfigV1) cấp website cho từng type_key.
CREATE TABLE IF NOT EXISTS content_editor_layouts (
    type_key    TEXT PRIMARY KEY NOT NULL REFERENCES content_types(key) ON DELETE RESTRICT,
    layout_json TEXT NOT NULL
                CHECK (json_valid(layout_json) AND json_type(layout_json) = 'object'),
    version     INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    updated_by  INTEGER REFERENCES users(id) ON DELETE SET NULL,
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
