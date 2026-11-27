use opendash_core::{
    builder::{BUILDER_SCHEMA_VERSION, BuilderDocument},
    builder_blocks::serialize_builder_document_to_blocks,
    content::{
        ContentPublication, ContentSupport, ContentTypeDefinition, ContentTypeDefinitionInput,
        ContentTypeSettings,
    },
    field_schema::{FieldRole, FieldType, MediaFieldKind, PublicExposure},
};
use serde_json::json;
use sqlx::SqlitePool;

use super::super::*;
use super::{TempDirGuard, spec};

fn build_test_cpts() -> (ContentTypeDefinition, ContentTypeDefinition) {
    let mut group_spec = spec(
        FieldType::Group,
        "meta_group",
        "Meta",
        FieldRole::Editor,
        PublicExposure::Public,
    );
    group_spec.fields = vec![
        spec(
            FieldType::Text,
            "note",
            "Note",
            FieldRole::Editor,
            PublicExposure::Public,
        ),
        spec(
            FieldType::Text,
            "priv_note",
            "Priv Note",
            FieldRole::Editor,
            PublicExposure::Private,
        ),
    ];
    let mut rep_spec = spec(
        FieldType::Repeater,
        "steps",
        "Steps",
        FieldRole::Editor,
        PublicExposure::Public,
    );
    rep_spec.fields = vec![
        spec(
            FieldType::Richtext,
            "detail",
            "Detail",
            FieldRole::Editor,
            PublicExposure::Public,
        ),
        spec(
            FieldType::Url,
            "sec_step",
            "Step URL",
            FieldRole::Editor,
            PublicExposure::Public,
        ),
    ];
    let mut banner = spec(
        FieldType::Media,
        "banner",
        "Banner",
        FieldRole::Editor,
        PublicExposure::Public,
    );
    banner.media_kind = Some(MediaFieldKind::Image);

    let guide_def = ContentTypeDefinitionInput {
        key: "guide".into(),
        label: "Hướng dẫn".into(),
        fields: vec![
            spec(
                FieldType::Richtext,
                "body",
                "Body",
                FieldRole::Editor,
                PublicExposure::Public,
            ),
            group_spec,
            rep_spec,
            spec(
                FieldType::Email,
                "api_secret",
                "Contact Email",
                FieldRole::Editor,
                PublicExposure::Public,
            ),
            spec(
                FieldType::Url,
                "doc_url",
                "URL",
                FieldRole::Editor,
                PublicExposure::Public,
            ),
            banner,
            spec(
                FieldType::Text,
                "private_f",
                "Private",
                FieldRole::Editor,
                PublicExposure::Private,
            ),
            spec(
                FieldType::Text,
                "admin_f",
                "Admin",
                FieldRole::Admin,
                PublicExposure::Private,
            ),
        ],
        settings: Some(ContentTypeSettings {
            public: true,
            has_archive: true,
            publication: ContentPublication::Immediate,
            supports: vec![ContentSupport::Editor, ContentSupport::Builder],
        }),
    }
    .into_canonical()
    .unwrap();

    let priv_def = ContentTypeDefinitionInput {
        key: "note_priv".into(),
        label: "Internal Note".into(),
        fields: vec![spec(
            FieldType::Text,
            "body",
            "Body",
            FieldRole::Editor,
            PublicExposure::Public,
        )],
        settings: Some(ContentTypeSettings {
            public: false,
            has_archive: false,
            publication: ContentPublication::Immediate,
            supports: vec![ContentSupport::Editor],
        }),
    }
    .into_canonical()
    .unwrap();

    (guide_def, priv_def)
}

async fn search(
    pool: &SqlitePool,
    q: &str,
    content_type: Option<&str>,
    page: i64,
) -> PublicSearchOutput {
    query_public_search(
        pool,
        &PublicSearchQueryInput {
            raw_query: Some(q),
            locale: None,
            content_type,
            page,
            per_page: 10,
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn query_public_search_bm25_vietnamese_facets_acl_and_snippets() {
    let tmp = TempDirGuard::new("query");
    let db_path = tmp.path().join("test.sqlite");
    let pool = crate::storage::init_db(&db_path).await.unwrap();

    let (guide_def, priv_def) = build_test_cpts();
    for def in [&guide_def, &priv_def] {
        sqlx::query(
            "INSERT INTO content_types (key, definition_json, owner_kind, owner_id)
             VALUES (?, ?, 'user', NULL)",
        )
        .bind(&def.key)
        .bind(serde_json::to_string(def).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    }

    sqlx::query(
        "INSERT INTO taxonomy_definitions (key, label, hierarchical, public, owner_kind)
         VALUES ('city', 'Thành phố', 0, 1, 'user'), ('internal_tag', 'Ẩn', 0, 0, 'user')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO content_type_taxonomy_overrides (content_type_key, taxonomies_json)
         VALUES ('guide', '[\"city\",\"internal_tag\"]')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let city_tid = sqlx::query(
        "INSERT INTO taxonomy_terms (kind, locale, translation_group_id, slug, name)
         VALUES ('city', 'vi', 't-city', 'do-thi', 'Đô thị')",
    )
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();
    let priv_tid = sqlx::query(
        "INSERT INTO taxonomy_terms (kind, locale, translation_group_id, slug, name)
         VALUES ('internal_tag', 'vi', 't-priv', 'bi-mat', 'secret_tax_tag')",
    )
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();

    let p_title = sqlx::query(
        "INSERT INTO content_entries
         (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status)
         VALUES ('post', 'vi', 'g-p1', 'p1', 'Kiến trúc Đường dẫn Mở rộng', ?, 'draft')",
    )
    .bind(json!({"body": "<p>Nội dung tổng quan hệ thống.</p><script>evil_js_tok()</script>"}).to_string())
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();

    let p_body = sqlx::query(
        "INSERT INTO content_entries
         (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status)
         VALUES ('post', 'vi', 'g-p2', 'p2', 'Bản tin tuần', ?, 'draft')",
    )
    .bind(json!({"body": "<p>Giới thiệu kiến trúc Đường dẫn tĩnh hiệu năng cao.</p>"}).to_string())
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();

    sqlx::query(
        "UPDATE content_entries
            SET publication_status = 'published',
                published_at = CASE WHEN id = ? THEN '2026-01-01T00:00:00Z' ELSE '2026-01-02T00:00:00Z' END
          WHERE id IN (?, ?)",
    )
    .bind(p_title)
    .bind(p_title)
    .bind(p_body)
    .execute(&pool)
    .await
    .unwrap();

    let bdoc: BuilderDocument = serde_json::from_value(json!({
        "schema_version": BUILDER_SCHEMA_VERSION,
        "sections": [{
            "id": "s1",
            "children": [{
                "id": "r1",
                "children": [{
                    "id": "c1",
                    "children": [
                        {"id": "e1", "type": "core/heading", "props": {"level": 2, "text": "Hướng dẫn Đường phố"}},
                        {"id": "e2", "type": "core/text", "props": {"text": "<p>Thiết kế kiến trúc Builder</p><style>.badcss{}</style>", "rich": true}}
                    ]
                }]
            }]
        }]
    }))
    .unwrap();
    let block_markup = serialize_builder_document_to_blocks(&bdoc).unwrap();
    let g1 = sqlx::query(
        "INSERT INTO content_entries
         (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, content_mode, published_at)
         VALUES ('guide', 'vi', 'g-g1', 'g1', 'Tài liệu UI', ?, 'published', 'builder', '2026-01-03T00:00:00Z')",
    )
    .bind(
        json!({
            "body": block_markup,
            "meta_group": {"note": "Ghi chú kiến trúc nhóm", "priv_note": "groupprivtok"},
            "steps": [{"detail": "<p>Bước kiến trúc repeater</p>", "sec_step": "https://repurltok.example"}],
            "api_secret": "emailsecrettok@example.com",
            "doc_url": "https://urlhidtok.example",
            "private_f": "privfieldtok",
            "admin_f": "admfieldtok"
        })
        .to_string(),
    )
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("INSERT INTO entry_terms (entry_id, term_id) VALUES (?, ?), (?, ?)")
        .bind(g1)
        .bind(city_tid)
        .bind(g1)
        .bind(priv_tid)
        .execute(&pool)
        .await
        .unwrap();

    // Legacy entry_builder_documents fallback (guide #2 with empty values_json)
    let g2 = sqlx::query(
        "INSERT INTO content_entries
         (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, content_mode, published_at)
         VALUES ('guide', 'vi', 'g-g2', 'g2', 'Cấu trúc cũ', '{}', 'published', 'builder', '2026-01-04T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();
    let legacy_json = json!({
        "schema_version": BUILDER_SCHEMA_VERSION,
        "sections": [{
            "id": "ls1",
            "children": [{
                "id": "lr1",
                "children": [{
                    "id": "lc1",
                    "children": [{"id": "le1", "type": "core/heading", "props": {"level": 2, "text": "Di sản Kiến trúc cổ"}}]
                }]
            }]
        }]
    })
    .to_string();
    let _: BuilderDocument = BuilderDocument::from_json(legacy_json.as_bytes()).unwrap();
    sqlx::query(
        "INSERT INTO entry_builder_documents (entry_id, document_json, updated_at) VALUES (?, ?, datetime('now'))",
    )
        .bind(g2)
        .bind(legacy_json)
        .execute(&pool)
        .await
        .unwrap();

    // Non-eligible rows (must never match): draft, trashed, pattern (draft trigger), private CPT
    sqlx::query(
        "INSERT INTO content_entries
         (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, trashed_at)
         VALUES
         ('post', 'vi', 'g-d', 'draft-1', 'Draft Entry', '{\"body\":\"draft_only_kw\"}', 'draft', NULL),
         ('post', 'vi', 'g-t', 'trash-1', 'Trashed Entry', '{\"body\":\"trashed_only_kw\"}', 'draft', NULL),
         ('pattern', 'vi', 'g-pat', 'pat-1', 'Pattern One', '{\"body\":\"pattern_only_kw\"}', 'draft', NULL),
         ('note_priv', 'vi', 'g-np', 'np-1', 'Private CPT', '{\"body\":\"private_cpt_kw\"}', 'published', NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE content_entries
            SET publication_status = 'published', published_at = datetime('now'), trashed_at = datetime('now')
          WHERE slug = 'trash-1'",
    )
    .execute(&pool)
    .await
    .unwrap();

    rebuild_search_index(&pool).await.unwrap();

    // 1. Diacritic + đ->d folding + BM25 title > body ranking + SnippetSegment highlight
    let res = search(&pool, "kien truc duong", None, 1).await;
    assert!(res.hits.len() >= 2);
    assert_eq!(
        res.hits[0].view.id, p_title,
        "Title match must rank first by BM25"
    );
    assert!(res.hits.iter().any(|h| h.view.id == p_body));
    let body_hit = res.hits.iter().find(|h| h.view.id == p_body).unwrap();
    assert!(body_hit.snippet_segments.iter().any(|s| s.matched));

    // 2. Public taxonomy term with đ->d + filter by CPT keeping all facets intact
    let city_res = search(&pool, "do thi", None, 1).await;
    assert_eq!(city_res.hits.len(), 1);
    assert_eq!(city_res.hits[0].view.id, g1);

    let guide_only = search(&pool, "kien", Some("guide"), 1).await;
    let hit_ids: Vec<i64> = guide_only.hits.iter().map(|h| h.view.id).collect();
    assert!(
        hit_ids.contains(&g1) && hit_ids.contains(&g2),
        "expected g1={g1} and g2={g2} in hit_ids={hit_ids:?}, total={}, facets={:?}",
        guide_only.total_items,
        guide_only.facets
    );
    assert!(!hit_ids.contains(&p_title));
    assert!(
        guide_only
            .facets
            .iter()
            .any(|f| f.type_key == "post" && !f.is_active)
    );
    assert!(
        guide_only
            .facets
            .iter()
            .any(|f| f.type_key == "guide" && f.is_active)
    );

    // 3. pattern / section / private CPT type filter -> empty hits, preserved facets
    for hidden_type in ["pattern", "note_priv"] {
        let hidden = search(&pool, "kien", Some(hidden_type), 1).await;
        assert_eq!(hidden.total_items, 0);
        assert!(hidden.hits.is_empty());
        assert!(!hidden.facets.is_empty());
    }

    // 4. Invalid type -> 400; out-of-range page -> 200 empty; Group/Repeater included; forbidden excluded
    assert_eq!(
        query_public_search(
            &pool,
            &PublicSearchQueryInput {
                raw_query: Some("kien"),
                locale: None,
                content_type: Some("Bad-Key"),
                page: 1,
                per_page: 10,
            },
        )
        .await
        .unwrap_err()
        .0,
        axum::http::StatusCode::BAD_REQUEST
    );
    let oob = search(&pool, "kien", None, 50).await;
    assert!(oob.total_items >= 4);
    assert!(oob.hits.is_empty());

    assert_eq!(search(&pool, "nhom", None, 1).await.hits[0].view.id, g1);
    assert_eq!(search(&pool, "repeater", None, 1).await.hits[0].view.id, g1);

    for secret_kw in [
        "evil_js_tok",
        "badcss",
        "groupprivtok",
        "repurltok",
        "emailsecrettok",
        "urlhidtok",
        "privfieldtok",
        "admfieldtok",
        "secret_tax_tag",
        "draft_only_kw",
        "trashed_only_kw",
        "pattern_only_kw",
        "private_cpt_kw",
    ] {
        let r = search(&pool, secret_kw, None, 1).await;
        assert_eq!(r.total_items, 0, "excluded token matched FTS5: {secret_kw}");
    }

    pool.close().await;
}
