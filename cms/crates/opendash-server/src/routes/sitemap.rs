//! M7-S04-T01 dynamic `/robots.txt`, `/sitemap.xml`, and `/sitemaps/{part}` routes.
use axum::{
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use sqlx::SqlitePool;

use crate::{
    error::{ApiError, ApiResult},
    seo::{render_robots_txt, render_sitemap_index, render_sitemap_part},
    storage::{
        settings,
        sitemap::{
            SITEMAP_MAX_PARTS, SITEMAP_MAX_TOTAL_URLS, SITEMAP_PART_SIZE,
            count_indexable_routes_in_tx, list_indexable_routes_in_tx,
        },
    },
};

fn parse_sitemap_part(raw: &str) -> Option<usize> {
    let num_str = raw.strip_suffix(".xml")?;
    if num_str.is_empty() || num_str.starts_with('0') || !num_str.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let page = num_str.parse::<usize>().ok()?;
    if (1..=SITEMAP_MAX_PARTS).contains(&page) {
        Some(page)
    } else {
        None
    }
}

pub async fn robots_txt(State(db): State<SqlitePool>) -> ApiResult<Response> {
    let pub_settings = settings::get_public(&db).await?;
    let body = render_robots_txt(&pub_settings);
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response())
}

pub async fn sitemap_index(State(db): State<SqlitePool>) -> ApiResult<Response> {
    let mut tx = db.begin().await?;
    let pub_settings = settings::get_public_in_conn(&mut tx).await?;
    let Some(origin) = pub_settings.public_origin.as_deref() else {
        return Err(ApiError::service_unavailable(
            "Chưa cấu hình public_origin để tạo sitemap.",
        ));
    };
    if !pub_settings.indexing_enabled {
        tx.commit().await?;
        let xml = render_sitemap_index(&pub_settings, 0);
        return Ok((
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/xml; charset=utf-8"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            xml,
        )
            .into_response());
    }
    let total = count_indexable_routes_in_tx(&mut tx, origin).await?;
    if total > SITEMAP_MAX_TOTAL_URLS {
        return Err(ApiError::service_unavailable(
            "Số lượng URL vượt quá giới hạn sitemap tối đa (1.000.000).",
        ));
    }
    let parts = if total <= 0 {
        0
    } else {
        ((total + SITEMAP_PART_SIZE - 1) / SITEMAP_PART_SIZE) as usize
    };
    tx.commit().await?;
    let xml = render_sitemap_index(&pub_settings, parts);
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/xml; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        xml,
    )
        .into_response())
}

pub async fn sitemap_part(
    State(db): State<SqlitePool>,
    Path(part): Path<String>,
) -> ApiResult<Response> {
    let Some(page) = parse_sitemap_part(&part) else {
        return Err(ApiError::not_found("Không tìm thấy phần sitemap."));
    };
    let mut tx = db.begin().await?;
    let pub_settings = settings::get_public_in_conn(&mut tx).await?;
    let Some(origin) = pub_settings.public_origin.as_deref() else {
        return Err(ApiError::service_unavailable(
            "Chưa cấu hình public_origin để tạo sitemap.",
        ));
    };
    if !pub_settings.indexing_enabled {
        return Err(ApiError::not_found("Không tìm thấy phần sitemap."));
    }
    let total = count_indexable_routes_in_tx(&mut tx, origin).await?;
    if total > SITEMAP_MAX_TOTAL_URLS {
        return Err(ApiError::service_unavailable(
            "Số lượng URL vượt quá giới hạn sitemap tối đa (1.000.000).",
        ));
    }
    let parts = if total <= 0 {
        0
    } else {
        ((total + SITEMAP_PART_SIZE - 1) / SITEMAP_PART_SIZE) as usize
    };
    if page > parts {
        return Err(ApiError::not_found("Không tìm thấy phần sitemap."));
    }
    let offset = ((page - 1) as i64) * SITEMAP_PART_SIZE;
    let routes =
        list_indexable_routes_in_tx(&mut tx, origin, offset, SITEMAP_PART_SIZE).await?;
    tx.commit().await?;

    let xml = render_sitemap_part(&pub_settings, &routes)?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/xml; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        xml,
    )
        .into_response())
}
