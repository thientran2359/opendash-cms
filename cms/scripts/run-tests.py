#!/usr/bin/env python3
"""Grouped and change-aware Rust test runner for OpenDash CMS.

Splits the 88+ integration test binaries in `cms/crates/opendash-server/tests/`
into 7 domain groups so each task/Slice only compiles and executes the tests
relevant to its changed files, while keeping `opendash-core` always fast and
providing `--group all --release` for pre-release full verification.

Usage:
    python3 cms/scripts/run-tests.py --list
    python3 cms/scripts/run-tests.py --check
    python3 cms/scripts/run-tests.py --changed
    python3 cms/scripts/run-tests.py --group theme,builder
    python3 cms/scripts/run-tests.py --group content --filter revisions
    python3 cms/scripts/run-tests.py --group all --release
    python3 cms/scripts/run-tests.py --self-test
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import fnmatch
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
import unittest


ROOT = Path(__file__).resolve().parents[2]
CMS_DIR = ROOT / "cms"
CARGO_MANIFEST = CMS_DIR / "Cargo.toml"
SERVER_TESTS_DIR = CMS_DIR / "crates" / "opendash-server" / "tests"
ADMIN_DIST_INDEX = CMS_DIR / "admin" / "dist" / "index.html"


@dataclass(frozen=True)
class GroupSpec:
    name: str
    description: str
    test_patterns: tuple[str, ...]
    unit_filters: tuple[str, ...]
    run_plugin_sdk: bool = False


GROUPS: tuple[GroupSpec, ...] = (
    GroupSpec(
        name="theme",
        description="Theme v3, Starter revisions, C-SEC templates, overrides, imports, public rendering",
        test_patterns=("theme",),
        unit_filters=(
            "theme::",
            "template_sec::",
            "content_render::",
            "document::",
            "field_hydrate::",
            "public_context::",
            "package::",
            "cache::",
            "dev_mode::",
            "storage::theme",
            "storage::import",
            "storage::public_content",
            "routes::themes",
        ),
    ),
    GroupSpec(
        name="builder",
        description="Visual Builder, Studio Mode 1/2, Patterns, Page Scope & Declarative Blocks",
        test_patterns=("builder",),
        unit_filters=(
            "page::",
            "field_hydrate::",
            "storage::builder",
            "storage::page",
            "storage::revisions",
            "routes::builder",
            "routes::revisions",
        ),
    ),
    GroupSpec(
        name="content",
        description="CPT schemas, Field Groups, Entries, Entry Table, Lifecycle & Taxonomy",
        test_patterns=("content",),
        unit_filters=(
            "i18n_revisions::",
            "field_hydrate::",
            "storage::content",
            "storage::entries",
            "storage::entry_lifecycle",
            "storage::entry_table",
            "storage::fields",
            "storage::revisions",
            "storage::taxonomy",
            "routes::entries",
            "routes::fields",
            "routes::revisions",
            "routes::taxonomy",
        ),
    ),
    GroupSpec(
        name="plugin",
        description="Plugin runtime (Wasmi/JS), Hooks, Egress SSRF, Projections, Admin Extensions & SDK",
        test_patterns=("plugin", "plugin_egress_ssrf"),
        unit_filters=(
            "plugin::",
            "hooks::",
            "cache::",
            "field_hydrate::",
            "storage::entry_lifecycle",
            "storage::plugin",
            "routes::plugin",
        ),
        run_plugin_sdk=True,
    ),
    GroupSpec(
        name="i18n",
        description="Admin i18n, Multilingual Content, Locale Routing & Theme L10n",
        test_patterns=("i18n",),
        unit_filters=(
            "i18n::",
            "i18n_redirects::",
            "i18n_revisions::",
            "l10n::",
            "field_hydrate::",
            "storage::settings",
            "routes::settings",
        ),
    ),
    GroupSpec(
        name="site-media-seo",
        description="Settings, Menus, Permalinks, SEO, Sitemap, Contact, SMTP & Media/Video",
        test_patterns=("site_media_seo",),
        unit_filters=(
            "media::",
            "media_video::",
            "i18n_redirects::",
            "smtp::",
            "field_hydrate::",
            "storage::menu",
            "storage::permalinks",
            "storage::settings",
            "storage::builder::s03_media_acl_tests",
            "routes::media",
            "routes::settings",
        ),
    ),
    GroupSpec(
        name="ops",
        description="SiteCoordinator, Maintenance Barriers, Media Reconcile, Backup, Restore & Runtime Config",
        test_patterns=("ops",),
        unit_filters=(
            "operations::",
            "paths::",
            "secrets::",
            "storage::operations",
            "storage::auth",
            "routes::operations",
        ),
    ),
)

GROUP_MAP: dict[str, GroupSpec] = {group.name: group for group in GROUPS}
ALL_GROUP_NAMES: tuple[str, ...] = tuple(group.name for group in GROUPS)


@dataclass(frozen=True)
class SelectionPlan:
    groups: tuple[str, ...]
    run_core: bool
    run_plugin_sdk: bool
    run_all_workspace: bool
    integration_tests: tuple[str, ...]
    unit_filters: tuple[str, ...]
    admin_only_note: bool
    reasons: tuple[str, ...]


def discover_integration_tests(tests_dir: Path = SERVER_TESTS_DIR) -> list[str]:
    if not tests_dir.is_dir():
        return []
    return sorted(path.stem for path in tests_dir.glob("*.rs") if path.is_file())


def map_tests_to_groups(
    test_stems: list[str],
) -> tuple[dict[str, list[str]], list[str]]:
    by_group: dict[str, list[str]] = {group.name: [] for group in GROUPS}
    unmapped: list[str] = []
    for stem in sorted(test_stems):
        matched = False
        for group in GROUPS:
            if any(fnmatch.fnmatch(stem, pat) for pat in group.test_patterns):
                by_group[group.name].append(stem)
                matched = True
        if not matched:
            unmapped.append(stem)
    return by_group, unmapped


def normalize_relpath(raw: str) -> str:
    return raw.strip().replace("\\", "/").lstrip("./")


def classify_changed_file(rel: str) -> tuple[set[str], set[str], bool, bool]:
    """Return (groups, extra_integration_tests, escalate_all, is_admin_only)."""
    path = normalize_relpath(rel)
    if not path:
        return set(), set(), False, False

    if not path.startswith("cms/"):
        return set(), set(), False, False

    if path.endswith(".md") or path.startswith("cms/deploy/") or path.startswith("cms/examples/"):
        return set(), set(), False, False

    if path == "cms/admin/scripts/build-public-tailwind.mjs":
        return {"theme"}, set(), False, False

    if path.startswith("cms/admin/"):
        return set(), set(), False, True

    if path.startswith("cms/scripts/"):
        return set(), set(), False, False

    if path.startswith("cms/fixtures/demo-website/"):
        return {"theme"}, set(), False, False

    if path.startswith("cms/fixtures/"):
        return {"ops", "content"}, set(), False, False

    if path in {
        "cms/Cargo.toml",
        "cms/Cargo.lock",
        "cms/rust-toolchain.toml",
        "cms/crates/opendash-server/Cargo.toml",
        "cms/crates/opendash-core/Cargo.toml",
        "cms/crates/opendash-plugin-sdk/Cargo.toml",
    }:
        return set(ALL_GROUP_NAMES), set(), True, False

    if path.startswith("cms/crates/opendash-plugin-sdk/") or path.startswith("cms/sdk/"):
        return {"plugin"}, set(), False, False

    if path.startswith("cms/crates/opendash-core/src/"):
        sub_core = path[len("cms/crates/opendash-core/src/") :]
        core_prefixes: tuple[tuple[str, set[str]], ...] = (
            ("theme/", {"theme"}),
            ("builder/", {"builder", "content"}),
            ("builder_blocks/", {"builder"}),
            ("richtext/", {"builder", "content"}),
            ("content/", {"content"}),
            ("plugin/", {"plugin"}),
            ("l10n/", {"i18n"}),
            ("redirects/", {"site-media-seo"}),
            ("revisions/", {"builder", "content"}),
        )
        for prefix, groups in core_prefixes:
            if sub_core.startswith(prefix):
                return groups, set(), False, False
        filename = path.rsplit("/", 1)[-1]
        core_routes: dict[str, set[str]] = {
            "theme.rs": {"theme"},
            "icons.rs": {"theme"},
            "builder.rs": {"builder", "content"},
            "builder_blocks.rs": {"builder"},
            "block_schema.rs": {"builder", "theme"},
            "richtext.rs": {"builder", "content"},
            "content.rs": {"content"},
            "plugin.rs": {"plugin"},
            "hooks.rs": {"plugin"},
            "admin_extensions.rs": {"plugin", "content"},
            "i18n.rs": {"i18n"},
            "contact.rs": {"site-media-seo"},
            "backup.rs": {"ops"},
        }
        if filename in core_routes:
            return core_routes[filename], set(), False, False
        return set(ALL_GROUP_NAMES), set(), True, False

    if path.startswith("cms/crates/opendash-server/tests/") and path.endswith(".rs"):
        rel_test = path[len("cms/crates/opendash-server/tests/") :]
        if "/" in rel_test:
            top_dir = rel_test.split("/", 1)[0]
            dir_to_group = {
                "theme": "theme",
                "builder": "builder",
                "content": "content",
                "plugin": "plugin",
                "i18n": "i18n",
                "site_media_seo": "site-media-seo",
                "ops": "ops",
            }
            if top_dir in dir_to_group:
                return {dir_to_group[top_dir]}, set(), False, False
            return set(ALL_GROUP_NAMES), set(), True, False
        stem = Path(path).stem
        groups = {
            group.name
            for group in GROUPS
            if any(fnmatch.fnmatch(stem, pat) for pat in group.test_patterns)
        }
        return groups, {stem}, False, False

    if path.startswith("cms/migrations/") and path.endswith(".sql"):
        lower = path.rsplit("/", 1)[-1].lower()
        groups: set[str] = {"theme"}  # Starter startup exercises all migrations.
        keyword_map = (
            (("theme", "override", "layout"), "theme"),
            (("plugin", "projection", "extension", "hook", "functions_runtime"), "plugin"),
            (("entry", "content", "taxonomy", "field", "lifecycle", "trash", "search", "fts", "revision", "autosave"), "content"),
            (("builder", "pattern", "section", "block", "revision", "autosave"), "builder"),
            (("i18n", "multilingual", "locale"), "i18n"),
            (("permalink", "redirect", "404", "seo", "menu", "media", "responsive", "variant", "contact", "smtp", "option", "setting"), "site-media-seo"),
            (("backup", "operation", "maintenance", "restore", "job"), "ops"),
        )
        matched_any = False
        for keywords, target in keyword_map:
            if any(kw in lower for kw in keywords):
                groups.add(target)
                matched_any = True
        if not matched_any:
            return set(ALL_GROUP_NAMES), set(), True, False
        return groups, set(), False, False

    if path.startswith("cms/crates/opendash-server/src/builtin_themes/"):
        return {"theme"}, set(), False, False

    if path.startswith("cms/crates/opendash-server/src/builtin_blocks/"):
        return {"builder", "theme"}, set(), False, False

    if path.startswith("cms/crates/opendash-server/src/plugin/"):
        return {"plugin"}, set(), False, False

    if path.startswith("cms/crates/opendash-server/src/"):
        sub = path[len("cms/crates/opendash-server/src/") :]
        if sub.startswith("storage/revisions/"):
            return {"builder", "content"}, set(), False, False
        if sub == "http_middleware.rs":
            return set(ALL_GROUP_NAMES), set(), True, False
        server_routes: dict[str, set[str]] = {
            # Theme & public rendering
            "theme.rs": {"theme"},
            "template_sec.rs": {"theme"},
            "content_render.rs": {"theme", "content"},
            "document.rs": {"theme"},
            "public_context.rs": {"theme"},
            "package.rs": {"theme", "plugin"},
            "cache.rs": {"theme", "plugin"},
            "storage/theme.rs": {"theme"},
            "storage/theme_tests.rs": {"theme"},
            "storage/overrides.rs": {"theme", "builder"},
            "storage/import.rs": {"theme"},
            "storage/public_content.rs": {"theme", "content"},
            "routes/themes.rs": {"theme"},
            "routes/import.rs": {"theme"},
            "routes/public.rs": {"theme"},
            "routes/public_content.rs": {"theme", "content"},
            # Builder & Studio
            "page.rs": {"builder", "theme"},
            "storage/builder.rs": {"builder"},
            "storage/builder_binding.rs": {"builder", "content"},
            "storage/page.rs": {"builder"},
            "storage/patterns.rs": {"builder"},
            "routes/builder.rs": {"builder"},
            "routes/patterns.rs": {"builder"},
            # Content, Entries & Taxonomy
            "storage/content.rs": {"content"},
            "storage/entries.rs": {"content"},
            "storage/entry_table.rs": {"content"},
            "storage/fields.rs": {"theme", "builder", "content", "plugin", "i18n", "site-media-seo"},
            "field_hydrate.rs": {"theme", "builder", "content", "plugin", "i18n", "site-media-seo"},
            "field_hydrate_tests.rs": {"theme", "builder", "content", "plugin", "i18n", "site-media-seo"},
            "storage/taxonomy.rs": {"content"},
            "storage/dashboard.rs": {"content"},
            "routes/content.rs": {"content"},
            "routes/entries.rs": {"content"},
            "routes/fields.rs": {"theme", "builder", "content", "plugin", "i18n", "site-media-seo"},
            "routes/taxonomy.rs": {"content"},
            "routes/taxonomy_tests.rs": {"content"},
            "routes/dashboard.rs": {"content"},
            "search.rs": {"content"},
            "revisions.rs": {"content"},
            # Plugins & Hooks
            "hooks.rs": {"plugin"},
            "storage/plugin.rs": {"plugin"},
            "storage/plugin_kv.rs": {"plugin"},
            "storage/plugin_projections.rs": {"plugin"},
            "storage/admin_extensions.rs": {"plugin", "content"},
            "routes/plugin.rs": {"plugin"},
            # Multilingual & i18n
            "i18n.rs": {"i18n"},
            "i18n_redirects.rs": {"i18n", "site-media-seo"},
            "i18n_revisions.rs": {"i18n", "content"},
            "routes/l10n.rs": {"i18n"},
            # Site, Media, SEO, Contact & SMTP
            "contact.rs": {"site-media-seo"},
            "media.rs": {"site-media-seo"},
            "media_lifecycle.rs": {"site-media-seo", "ops"},
            "media_video.rs": {"site-media-seo"},
            "redirects.rs": {"site-media-seo"},
            "seo.rs": {"site-media-seo"},
            "smtp.rs": {"site-media-seo"},
            "smtp_tests.rs": {"site-media-seo"},
            "storage/contact.rs": {"site-media-seo"},
            "storage/media.rs": {"site-media-seo"},
            "storage/media_lifecycle.rs": {"site-media-seo", "ops"},
            "storage/media_refs.rs": {"site-media-seo", "builder", "theme"},
            "storage/s03_media_acl_tests.rs": {"site-media-seo", "theme"},
            "storage/menu.rs": {"site-media-seo", "theme"},
            "storage/permalinks.rs": {"site-media-seo", "content"},
            "storage/seo.rs": {"site-media-seo"},
            "storage/sitemap.rs": {"site-media-seo"},
            "storage/smtp.rs": {"site-media-seo"},
            "storage/settings.rs": {"site-media-seo", "i18n", "theme"},
            "routes/contact.rs": {"site-media-seo"},
            "routes/media.rs": {"site-media-seo"},
            "routes/menu.rs": {"site-media-seo"},
            "routes/seo.rs": {"site-media-seo"},
            "routes/sitemap.rs": {"site-media-seo"},
            "routes/smtp.rs": {"site-media-seo"},
            "routes/settings.rs": {"site-media-seo", "i18n", "theme"},
            # Operations, Backup & Recovery
            "backup.rs": {"ops"},
            "backup_archive.rs": {"ops"},
            "backup_worker.rs": {"ops"},
            "restore.rs": {"ops"},
            "operations.rs": {"ops"},
            "operations_tests.rs": {"ops"},
            "operations_worker_process.rs": {"ops"},
            "config.rs": {"ops"},
            "paths.rs": {"ops"},
            "secrets.rs": {"ops", "site-media-seo"},
            "storage/backup.rs": {"ops"},
            "storage/restore.rs": {"ops"},
            "storage/operations.rs": {"ops"},
            "storage/operations_tests.rs": {"ops"},
            "routes/operations.rs": {"ops"},
        }
        if sub in server_routes:
            return server_routes[sub], set(), False, False
        server_prefixes: tuple[tuple[str, set[str]], ...] = (
            ("theme/", {"theme"}),
            ("template_sec/", {"theme"}),
            ("content_render/", {"theme", "content"}),
            ("document/", {"theme"}),
            ("dev_mode/", {"theme"}),
            ("storage/theme/", {"theme"}),
            ("storage/theme_tests/", {"theme"}),
            ("storage/overrides/", {"theme"}),
            ("storage/import/", {"theme"}),
            ("storage/public_content/", {"theme", "content"}),
            ("routes/themes/", {"theme"}),
            ("routes/public_content/", {"theme", "content"}),
            ("page/", {"builder", "theme"}),
            ("storage/builder/", {"builder"}),
            ("storage/page/", {"builder", "theme"}),
            ("storage/content/", {"content"}),
            ("storage/entries/", {"content"}),
            ("storage/entry_lifecycle/", {"content", "plugin"}),
            ("storage/entry_table/", {"content"}),
            ("storage/fields/", {"theme", "builder", "content", "plugin", "i18n", "site-media-seo"}),
            ("storage/taxonomy/", {"content"}),
            ("routes/entries/", {"content"}),
            ("search/", {"content"}),
            ("storage/search/", {"content"}),
            ("routes/search/", {"content"}),
            ("revisions/", {"builder", "content"}),
            ("storage/revisions/", {"builder", "content"}),
            ("routes/revisions/", {"builder", "content", "ops"}),
            ("routes/extensions/", {"plugin", "content"}),
            ("hooks/", {"plugin"}),
            ("storage/plugin/", {"plugin"}),
            ("storage/plugin_projections/", {"plugin"}),
            ("routes/plugin/", {"plugin"}),
            ("l10n/", {"i18n"}),
            ("media/", {"site-media-seo"}),
            ("media_lifecycle/", {"site-media-seo", "ops"}),
            ("media_video/", {"site-media-seo"}),
            ("storage/menu/", {"site-media-seo", "theme"}),
            ("storage/permalinks/", {"site-media-seo", "content"}),
            ("storage/redirects/", {"site-media-seo"}),
            ("storage/seo/", {"site-media-seo"}),
            ("storage/settings/", {"site-media-seo", "i18n", "theme"}),
            ("routes/redirects/", {"site-media-seo"}),
            ("backup_worker/", {"ops"}),
            ("secrets/", {"ops", "site-media-seo"}),
            ("routes/operations/", {"ops"}),
        )
        for prefix, groups in server_prefixes:
            if sub.startswith(prefix):
                return groups, set(), False, False
        # Shared runtime entrypoints (main.rs, app.rs, state.rs, auth.rs, error.rs, storage/mod.rs, etc.)
        return set(ALL_GROUP_NAMES), set(), True, False

    # Any other unrecognized file inside cms/crates/ escalates safely.
    if path.startswith("cms/crates/"):
        return set(ALL_GROUP_NAMES), set(), True, False

    return set(), set(), False, False


def build_plan_from_groups(
    group_names: list[str],
    all_tests: list[str],
    extra_tests: set[str] | None = None,
    reasons: list[str] | None = None,
    force_all_workspace: bool = False,
) -> SelectionPlan:
    extra = set(extra_tests or ())
    if force_all_workspace or "all" in group_names:
        return SelectionPlan(
            groups=ALL_GROUP_NAMES,
            run_core=True,
            run_plugin_sdk=True,
            run_all_workspace=True,
            integration_tests=tuple(sorted(set(all_tests) | extra)),
            unit_filters=(),
            admin_only_note=False,
            reasons=tuple(reasons or ["explicit all"]),
        )

    if group_names == ["core"]:
        return SelectionPlan(
            groups=(),
            run_core=True,
            run_plugin_sdk=False,
            run_all_workspace=False,
            integration_tests=tuple(sorted(extra)),
            unit_filters=(),
            admin_only_note=False,
            reasons=tuple(reasons or ["core only"]),
        )

    ordered_groups = tuple(g for g in ALL_GROUP_NAMES if g in group_names)
    by_group, _ = map_tests_to_groups(all_tests)

    selected_tests: set[str] = set(extra)
    unit_filters: list[str] = []
    run_plugin_sdk = False
    for name in ordered_groups:
        spec = GROUP_MAP[name]
        selected_tests.update(by_group.get(name, ()))
        for flt in spec.unit_filters:
            if flt not in unit_filters:
                unit_filters.append(flt)
        if spec.run_plugin_sdk:
            run_plugin_sdk = True

    return SelectionPlan(
        groups=ordered_groups,
        run_core=True,
        run_plugin_sdk=run_plugin_sdk,
        run_all_workspace=False,
        integration_tests=tuple(sorted(selected_tests)),
        unit_filters=tuple(unit_filters),
        admin_only_note=False,
        reasons=tuple(reasons or [f"groups={','.join(ordered_groups)}"]),
    )


WIRING_ONLY_FILES: frozenset[str] = frozenset(
    {
        "cms/crates/opendash-core/src/lib.rs",
        "cms/crates/opendash-server/src/routes/mod.rs",
        "cms/crates/opendash-server/src/storage/mod.rs",
        "cms/crates/opendash-server/src/main.rs",
        "cms/crates/opendash-server/src/app.rs",
    }
)

_MOD_DECL_RE = re.compile(
    r"^(?:pub(?:\s*\([^)]+\))?\s+)?mod\s+[A-Za-z_][A-Za-z0-9_]*\s*;$"
)
_USE_DECL_RE = re.compile(
    r"^(?:pub(?:\s*\([^)]+\))?\s+)?use\s+[A-Za-z0-9_:{},\s\*]+\s*;$"
)
_ATTR_DECL_RE = re.compile(r"^#\[(?:cfg|allow)\([^)\n]+\)\]$")
_FORBIDDEN_DECL_TOKENS_RE = re.compile(
    r"\b(?:fn|struct|enum|impl|static|const|let|unsafe|async|extern|trait|type|macro_rules)\b"
)

_APP_FORBIDDEN_TOKENS_RE = re.compile(
    r"\b(?:fn|let|const|static|struct|enum|impl|layer|with_state|from_fn|"
    r"middleware|cors|if|match|return|async|auth)\b|Router::|\bAppState\b"
)
_APP_USE_START_RE = re.compile(
    r"^use\s+(?:crate::)?routes::[A-Za-z0-9_:{},\s\*]*;?$"
)
_APP_IMPORT_ITEM_RE = re.compile(
    r"^(?:[A-Za-z_][A-Za-z0-9_]*(?:\s+as\s+[A-Za-z_][A-Za-z0-9_]*)?"
    r"(?:::[A-Za-z0-9_:{},\s\*]*)?|\}[,;]?)$"
)
_APP_ROUTE_STRING_RE = re.compile(r'"/[A-Za-z0-9_/{}.*:-]*"')
_APP_ROUTE_ALLOWED_CHARS_RE = re.compile(r"^[A-Za-z0-9_:,().\s]+$")
_APP_CALL_NAME_RE = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(")
_APP_ALLOWED_CALLS = frozenset({"route", "nest", "get", "post", "put", "patch", "delete"})


def _extract_diff_changed_lines(diff_text: str) -> list[str]:
    changed: list[str] = []
    for raw in diff_text.splitlines():
        if raw.startswith(("+++", "---", "@@", "diff ", "index ", "\\")):
            continue
        if not raw.startswith(("+", "-")):
            continue
        stripped = raw[1:].strip()
        if not stripped or stripped.startswith("//"):
            continue
        changed.append(stripped)
    return changed


def _is_app_wiring_line(line: str) -> bool:
    if _APP_FORBIDDEN_TOKENS_RE.search(line):
        return False
    if line.startswith("use ") or line.startswith("use\t"):
        return bool(_APP_USE_START_RE.match(line))
    if _APP_IMPORT_ITEM_RE.match(line):
        return True
    if line in {")", "),", ");"}:
        return True
    without_strings = _APP_ROUTE_STRING_RE.sub("", line)
    if '"' in without_strings or not _APP_ROUTE_ALLOWED_CHARS_RE.match(without_strings):
        return False
    for match in _APP_CALL_NAME_RE.finditer(without_strings):
        if match.group(1) not in _APP_ALLOWED_CALLS:
            return False
    return True


def is_wiring_only_diff(rel: str, diff_text: str) -> bool:
    """Return True if a diff in a WIRING_ONLY_FILES entrypoint only registers mods/routes."""
    path = normalize_relpath(rel)
    if path not in WIRING_ONLY_FILES:
        return False
    changed_lines = _extract_diff_changed_lines(diff_text)
    if not changed_lines:
        return False
    if path == "cms/crates/opendash-server/src/app.rs":
        return all(_is_app_wiring_line(line) for line in changed_lines)
    for line in changed_lines:
        if _FORBIDDEN_DECL_TOKENS_RE.search(line):
            return False
        if not (
            _MOD_DECL_RE.match(line)
            or _USE_DECL_RE.match(line)
            or _ATTR_DECL_RE.match(line)
        ):
            return False
    return True


def build_plan_from_files(
    changed_files: list[str],
    all_tests: list[str],
    wiring_only_files: set[str] | None = None,
) -> SelectionPlan:
    groups: set[str] = set()
    extra_tests: set[str] = set()
    escalate_all = False
    saw_admin_only = False
    reasons: list[str] = []
    wiring_set = {normalize_relpath(item) for item in (wiring_only_files or ())}
    deferred_wiring: list[str] = []

    for raw in changed_files:
        rel = normalize_relpath(raw)
        if not rel:
            continue
        if rel in wiring_set:
            deferred_wiring.append(rel)
            continue
        file_groups, file_extra, file_all, is_admin = classify_changed_file(rel)
        if file_all:
            escalate_all = True
            reasons.append(f"{rel} -> all (shared core)")
        elif file_groups or file_extra:
            groups.update(file_groups)
            extra_tests.update(file_extra)
            tag = ",".join(sorted(file_groups)) if file_groups else ",".join(sorted(file_extra))
            reasons.append(f"{rel} -> {tag}")
        elif is_admin:
            saw_admin_only = True

    if deferred_wiring:
        if groups or extra_tests:
            for rel in deferred_wiring:
                reasons.append(f"{rel} -> wiring-only (deferred to matched domain)")
        else:
            escalate_all = True
            for rel in deferred_wiring:
                reasons.append(f"{rel} -> all (wiring-only without domain file)")

    if escalate_all:
        return build_plan_from_groups(
            ["all"], all_tests, extra_tests=extra_tests, reasons=reasons, force_all_workspace=True
        )

    if not groups and not extra_tests:
        return SelectionPlan(
            groups=(),
            run_core=False,
            run_plugin_sdk=False,
            run_all_workspace=False,
            integration_tests=(),
            unit_filters=(),
            admin_only_note=saw_admin_only,
            reasons=tuple(reasons),
        )

    return build_plan_from_groups(
        sorted(groups), all_tests, extra_tests=extra_tests, reasons=reasons
    )


def resolve_git_diff_target(base_ref: str, repo_root: Path = ROOT) -> str | None:
    candidates = [base_ref, "origin/main", "HEAD^", "HEAD"]
    resolved_base: str | None = None
    for ref in candidates:
        res = subprocess.run(
            ["git", "-C", str(repo_root), "rev-parse", "--verify", "--quiet", ref],
            capture_output=True,
            text=True,
            check=False,
        )
        if res.returncode == 0:
            resolved_base = ref
            break

    if resolved_base is None:
        return None
    mb = subprocess.run(
        ["git", "-C", str(repo_root), "merge-base", resolved_base, "HEAD"],
        capture_output=True,
        text=True,
        check=False,
    )
    return mb.stdout.strip() if mb.returncode == 0 and mb.stdout.strip() else resolved_base


def detect_git_changed_files(base_ref: str, repo_root: Path = ROOT) -> list[str]:
    diff_target = resolve_git_diff_target(base_ref, repo_root=repo_root)
    files: set[str] = set()
    if diff_target is not None:
        # Compare working tree (committed + staged + unstaged) against merge-base.
        diff = subprocess.run(
            ["git", "-C", str(repo_root), "diff", "--name-only", diff_target],
            capture_output=True,
            text=True,
            check=True,
        )
        files.update(line.strip() for line in diff.stdout.splitlines() if line.strip())

    untracked = subprocess.run(
        ["git", "-C", str(repo_root), "ls-files", "--others", "--exclude-standard"],
        capture_output=True,
        text=True,
        check=True,
    )
    files.update(line.strip() for line in untracked.stdout.splitlines() if line.strip())
    return sorted(files)


def detect_wiring_only_files(
    changed_files: list[str], base_ref: str, repo_root: Path = ROOT
) -> set[str]:
    diff_target = resolve_git_diff_target(base_ref, repo_root=repo_root)
    if diff_target is None:
        return set()
    wiring_only: set[str] = set()
    for raw in changed_files:
        rel = normalize_relpath(raw)
        if rel not in WIRING_ONLY_FILES:
            continue
        proc = subprocess.run(
            ["git", "-C", str(repo_root), "diff", "-U0", diff_target, "--", rel],
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode == 0 and is_wiring_only_diff(rel, proc.stdout):
            wiring_only.add(rel)
    return wiring_only


def canonical_tmpdir_env(base_env: dict[str, str], platform: str | None = None) -> dict[str, str]:
    """Avoid macOS `/var -> /private/var` symlink rejections in runtime path tests."""
    env = dict(base_env)
    active_platform = platform or sys.platform
    if active_platform == "darwin":
        raw_tmp = env.get("TMPDIR") or tempfile.gettempdir()
        resolved = os.path.realpath(raw_tmp)
        if not resolved or not Path(resolved).is_dir():
            resolved = "/private/tmp"
        env["TMPDIR"] = resolved
    return env


def test_runner_env(base_env: dict[str, str], platform: str | None = None) -> dict[str, str]:
    """Build subprocess environment with canonical TMPDIR and bounded default parallelism."""
    env = canonical_tmpdir_env(base_env, platform=platform)
    if not env.get("RUST_TEST_THREADS", "").strip():
        env["RUST_TEST_THREADS"] = "4"
    return env


def build_cargo_commands(
    plan: SelectionPlan, release: bool, test_filter: str | None = None
) -> list[list[str]]:
    base = ["cargo", "test", "--manifest-path", str(CARGO_MANIFEST), "--locked"]
    if release:
        base.append("--release")

    if plan.run_all_workspace:
        cmd = [*base, "--workspace"]
        if test_filter:
            cmd.extend(["--", test_filter])
        return [cmd]

    commands: list[list[str]] = []
    if plan.run_core:
        cmd = [*base, "-p", "opendash-core"]
        if test_filter:
            cmd.extend(["--", test_filter])
        commands.append(cmd)
    if plan.run_plugin_sdk:
        cmd = [*base, "-p", "opendash-plugin-sdk"]
        if test_filter:
            cmd.extend(["--", test_filter])
        commands.append(cmd)
    if plan.unit_filters:
        filters = [test_filter] if test_filter else list(plan.unit_filters)
        commands.append([*base, "-p", "opendash-server", "--bin", "opendash", "--", *filters])
    if plan.integration_tests:
        cmd = [*base, "-p", "opendash-server"]
        for stem in plan.integration_tests:
            cmd.extend(["--test", stem])
        if test_filter:
            cmd.extend(["--", test_filter])
        commands.append(cmd)
    return commands


def print_group_listing(all_tests: list[str]) -> int:
    by_group, unmapped = map_tests_to_groups(all_tests)
    print(f"OpenDash Rust Test Groups ({len(all_tests)} integration test files):\n")
    for group in GROUPS:
        stems = by_group[group.name]
        print(f"  {group.name:<15} ({len(stems):>2} integration tests) — {group.description}")
        print(f"    tests: {', '.join(stems) if stems else '(none)'}")
        print(f"    unit : {', '.join(group.unit_filters)}")
    print("\nSpecial targets:")
    print("  core            — only opendash-core unit tests (~2s)")
    print("  all             — full workspace (all 7 groups + all unit tests + SDK)")
    if unmapped:
        print(f"\nERROR: {len(unmapped)} unmapped integration test file(s): {', '.join(unmapped)}", file=sys.stderr)
        return 1
    print("\nCoverage check: PASS (0 unmapped integration test files).")
    return 0


class RunTestsSelfTest(unittest.TestCase):
    def test_zero_unmapped_integration_tests(self) -> None:
        all_tests = discover_integration_tests()
        self.assertGreaterEqual(len(all_tests), 7, "expected consolidated domain integration tests")
        _, unmapped = map_tests_to_groups(all_tests)
        self.assertEqual(unmapped, [], f"every tests/*.rs must belong to a group: {unmapped}")

    def test_docs_only_skips_rust(self) -> None:
        all_tests = discover_integration_tests()
        plan = build_plan_from_files(["AGENTS.md", "docs/designs/SITEMAP.md"], all_tests)
        self.assertFalse(plan.run_core)
        self.assertEqual(plan.groups, ())
        self.assertEqual(build_cargo_commands(plan, release=False), [])

    def test_admin_only_skips_rust_with_note(self) -> None:
        all_tests = discover_integration_tests()
        plan = build_plan_from_files(["cms/admin/src/App.tsx"], all_tests)
        self.assertFalse(plan.run_core)
        self.assertTrue(plan.admin_only_note)
        self.assertEqual(build_cargo_commands(plan, release=False), [])

    def test_theme_starter_routes_only_theme_group(self) -> None:
        all_tests = discover_integration_tests()
        plan = build_plan_from_files(
            [
                "cms/crates/opendash-server/src/builtin_themes/mod.rs",
                "cms/crates/opendash-server/src/storage/theme.rs",
            ],
            all_tests,
        )
        self.assertEqual(plan.groups, ("theme",))
        self.assertTrue(plan.run_core)
        self.assertFalse(plan.run_all_workspace)
        self.assertIn("theme", plan.integration_tests)
        self.assertNotIn("ops", plan.integration_tests)

    def test_nested_integration_test_routes_to_group(self) -> None:
        all_tests = discover_integration_tests()
        plan = build_plan_from_files(
            ["cms/crates/opendash-server/tests/theme/starter_theme.rs"],
            all_tests,
        )
        self.assertEqual(plan.groups, ("theme",))
        self.assertFalse(plan.run_all_workspace)
        self.assertIn("theme", plan.integration_tests)

    def test_brand_new_integration_test_always_included(self) -> None:
        all_tests = discover_integration_tests()
        plan = build_plan_from_files(
            ["cms/crates/opendash-server/tests/brand_new_feature_test.rs"],
            all_tests,
        )
        self.assertIn("brand_new_feature_test", plan.integration_tests)
        self.assertFalse(plan.run_all_workspace)

    def test_shared_core_change_escalates_to_all(self) -> None:
        all_tests = discover_integration_tests()
        for critical in (
            "cms/Cargo.toml",
            "cms/crates/opendash-core/src/id.rs",
            "cms/crates/opendash-core/src/field_schema.rs",
            "cms/crates/opendash-server/src/app.rs",
            "cms/crates/opendash-server/src/auth.rs",
        ):
            plan = build_plan_from_files([critical], all_tests)
            self.assertTrue(plan.run_all_workspace, f"{critical} must escalate to all")

    def test_fields_endpoints_route_to_domain_groups(self) -> None:
        all_tests = discover_integration_tests()
        plan = build_plan_from_files(
            [
                "cms/crates/opendash-server/src/field_hydrate.rs",
                "cms/crates/opendash-server/src/storage/fields.rs",
                "cms/crates/opendash-server/src/routes/fields.rs",
            ],
            all_tests,
        )
        self.assertFalse(plan.run_all_workspace)
        self.assertEqual(
            set(plan.groups),
            {"theme", "builder", "content", "plugin", "i18n", "site-media-seo"},
        )
        self.assertIn("field_hydrate::", plan.unit_filters)
        self.assertIn("storage::fields", plan.unit_filters)
        self.assertIn("routes::fields", plan.unit_filters)

    def test_domain_migration_stays_targeted(self) -> None:
        all_tests = discover_integration_tests()
        plan = build_plan_from_files(
            ["cms/migrations/0053_theme_functions_runtime.sql"],
            all_tests,
        )
        self.assertFalse(plan.run_all_workspace)
        self.assertEqual(set(plan.groups), {"theme", "plugin"})

    def test_macos_tmpdir_resolves_symlink(self) -> None:
        env = canonical_tmpdir_env({"TMPDIR": "/var/tmp"}, platform="darwin")
        self.assertTrue(env["TMPDIR"].startswith("/private/"))

    def test_revisions_and_autosaves_routing(self) -> None:
        all_tests = discover_integration_tests()
        mig_plan = build_plan_from_files(
            ["cms/migrations/0058_entry_revisions_and_autosaves.sql"],
            all_tests,
        )
        self.assertFalse(mig_plan.run_all_workspace)
        self.assertEqual(set(mig_plan.groups), {"theme", "content", "builder"})

        core_plan = build_plan_from_files(
            ["cms/crates/opendash-core/src/revisions/text_diff.rs"],
            all_tests,
        )
        self.assertFalse(core_plan.run_all_workspace)
        self.assertEqual(set(core_plan.groups), {"builder", "content"})
        self.assertIn("storage::revisions", core_plan.unit_filters)

        server_plan = build_plan_from_files(
            ["cms/crates/opendash-server/src/storage/revisions/capture.rs"],
            all_tests,
        )
        self.assertFalse(server_plan.run_all_workspace)
        self.assertEqual(set(server_plan.groups), {"builder", "content"})
        self.assertIn("storage::revisions", server_plan.unit_filters)

    def test_runner_env_defaults_rust_test_threads_to_4(self) -> None:
        env = test_runner_env({"TMPDIR": "/private/tmp"}, platform="linux")
        self.assertEqual(env["RUST_TEST_THREADS"], "4")

    def test_runner_env_preserves_explicit_rust_test_threads(self) -> None:
        env = test_runner_env(
            {"TMPDIR": "/private/tmp", "RUST_TEST_THREADS": "8"}, platform="linux"
        )
        self.assertEqual(env["RUST_TEST_THREADS"], "8")

    def test_modular_subdirectories_route_without_escalating_to_all(self) -> None:
        all_tests = discover_integration_tests()
        cases = (
            ("cms/crates/opendash-core/src/theme/mod.rs", {"theme"}),
            ("cms/crates/opendash-core/src/content/types.rs", {"content"}),
            ("cms/crates/opendash-server/src/template_sec/policy/mod.rs", {"theme"}),
            ("cms/crates/opendash-server/src/storage/entries/list.rs", {"content"}),
            ("cms/crates/opendash-server/src/storage/entry_lifecycle/trash.rs", {"content", "plugin"}),
            ("cms/crates/opendash-server/src/routes/revisions/revisions_handlers.rs", {"builder", "content", "ops"}),
            ("cms/crates/opendash-server/src/routes/operations/backup.rs", {"ops"}),
        )
        for path, expected_groups in cases:
            plan = build_plan_from_files([path], all_tests)
            self.assertFalse(plan.run_all_workspace, f"{path} should stay targeted")
            self.assertEqual(set(plan.groups), expected_groups, path)

        rev_route_plan = build_plan_from_files(
            ["cms/crates/opendash-server/src/routes/revisions/autosave_handlers.rs"],
            all_tests,
        )
        self.assertIn("routes::revisions", rev_route_plan.unit_filters)

        middleware_plan = build_plan_from_files(
            ["cms/crates/opendash-server/src/http_middleware.rs"],
            all_tests,
        )
        self.assertTrue(middleware_plan.run_all_workspace)

        shared = build_plan_from_files(["cms/crates/opendash-core/src/field_schema/spec/mod.rs"], all_tests)
        self.assertTrue(shared.run_all_workspace)

    def test_wiring_only_diff_accepts_mod_use_and_route_registrations(self) -> None:
        mod_diff = (
            "diff --git a/cms/crates/opendash-server/src/routes/mod.rs "
            "b/cms/crates/opendash-server/src/routes/mod.rs\n"
            "@@ -20,0 +21 @@\n"
            "+pub mod revisions;\n"
        )
        self.assertTrue(is_wiring_only_diff("cms/crates/opendash-server/src/routes/mod.rs", mod_diff))

        main_diff = (
            "diff --git a/cms/crates/opendash-server/src/main.rs "
            "b/cms/crates/opendash-server/src/main.rs\n"
            "@@ -31,0 +32 @@\n"
            "+mod revisions;\n"
        )
        self.assertTrue(is_wiring_only_diff("cms/crates/opendash-server/src/main.rs", main_diff))

        app_diff = (
            "diff --git a/cms/crates/opendash-server/src/app.rs "
            "b/cms/crates/opendash-server/src/app.rs\n"
            "@@ -40,0 +41 @@\n"
            "+        revisions::{revision_list, revision_restore},\n"
            "@@ -250,0 +252,4 @@\n"
            "+        .route(\n"
            '+            "/api/admin/entries/{id}/revisions",\n'
            "+            get(revision_list).post(revision_restore),\n"
            "+        )\n"
        )
        self.assertTrue(is_wiring_only_diff("cms/crates/opendash-server/src/app.rs", app_diff))

    def test_wiring_only_diff_rejects_runtime_or_security_edits(self) -> None:
        bad_storage = (
            "@@ -50,0 +51,2 @@\n"
            "+pub async fn init_db_extra() {}\n"
        )
        self.assertFalse(is_wiring_only_diff("cms/crates/opendash-server/src/storage/mod.rs", bad_storage))

        bad_app_layer = (
            "@@ -100,0 +101 @@\n"
            '+        .route("/api/admin/x", post(handler).layer(DefaultBodyLimit::max(4096)))\n'
        )
        self.assertFalse(is_wiring_only_diff("cms/crates/opendash-server/src/app.rs", bad_app_layer))

        bad_app_middleware = (
            "@@ -100,0 +101 @@\n"
            "+        .layer(middleware::from_fn(require_admin))\n"
        )
        self.assertFalse(is_wiring_only_diff("cms/crates/opendash-server/src/app.rs", bad_app_middleware))

        bad_app_use = (
            "@@ -10,0 +11 @@\n"
            "+use crate::auth::require_admin;\n"
        )
        self.assertFalse(is_wiring_only_diff("cms/crates/opendash-server/src/app.rs", bad_app_use))

    def test_wiring_only_files_defer_to_domain_or_escalate_alone(self) -> None:
        all_tests = discover_integration_tests()
        wiring = {
            "cms/crates/opendash-server/src/routes/mod.rs",
            "cms/crates/opendash-server/src/app.rs",
        }
        plan = build_plan_from_files(
            [
                "cms/crates/opendash-server/src/routes/mod.rs",
                "cms/crates/opendash-server/src/app.rs",
                "cms/crates/opendash-server/src/storage/revisions/list.rs",
            ],
            all_tests,
            wiring_only_files=wiring,
        )
        self.assertFalse(plan.run_all_workspace)
        self.assertEqual(plan.groups, ("content",))

        alone = build_plan_from_files(
            ["cms/crates/opendash-server/src/app.rs"],
            all_tests,
            wiring_only_files={"cms/crates/opendash-server/src/app.rs"},
        )
        self.assertTrue(alone.run_all_workspace)

    def test_feature_migrations_stay_targeted(self) -> None:
        all_tests = discover_integration_tests()
        search_plan = build_plan_from_files(
            ["cms/migrations/0059_public_site_search_fts5.sql"], all_tests
        )
        self.assertFalse(search_plan.run_all_workspace)
        self.assertEqual(set(search_plan.groups), {"theme", "content"})

        rev_plan = build_plan_from_files(
            ["cms/migrations/0060_entry_revisions_autosave.sql"], all_tests
        )
        self.assertFalse(rev_plan.run_all_workspace)
        self.assertEqual(set(rev_plan.groups), {"theme", "content"})

        media_plan = build_plan_from_files(
            ["cms/migrations/0061_responsive_media_variants.sql"], all_tests
        )
        self.assertFalse(media_plan.run_all_workspace)
        self.assertEqual(set(media_plan.groups), {"theme", "site-media-seo"})

    def test_build_cargo_commands_applies_test_filter(self) -> None:
        all_tests = discover_integration_tests()
        plan = build_plan_from_files(
            ["cms/crates/opendash-server/src/routes/revisions/list.rs"], all_tests
        )
        cmds = build_cargo_commands(plan, release=False, test_filter="revisions")
        self.assertTrue(any("--bin" in c and c[-2:] == ["--", "revisions"] for c in cmds))
        self.assertTrue(any("--test" in c and c[-2:] == ["--", "revisions"] for c in cmds))
        self.assertTrue(any("-p" in c and "opendash-core" in c and c[-2:] == ["--", "revisions"] for c in cmds))

    def test_check_dry_run_exits_zero(self) -> None:
        self.assertEqual(main(["--check", "--dry-run"]), 0)


def run_self_test() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(RunTestsSelfTest)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if result.wasSuccessful() else 1


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run OpenDash Rust tests by domain group or git diff."
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--list", action="store_true", help="List groups and verify test coverage.")
    mode.add_argument(
        "--check",
        action="store_true",
        help="Tier-0 fast syntax/type check (cargo check --workspace --bins --lib).",
    )
    mode.add_argument(
        "--group",
        help=f"Comma-separated group names: core, all, {', '.join(ALL_GROUP_NAMES)}",
    )
    mode.add_argument(
        "--changed",
        action="store_true",
        help="Auto-select test groups from changed/untracked files against --base.",
    )
    mode.add_argument("--self-test", action="store_true", help="Run built-in runner self-tests.")
    parser.add_argument(
        "--base",
        default="origin/main",
        help="Git base ref for --changed (default: origin/main).",
    )
    parser.add_argument(
        "--files",
        nargs="+",
        help="Explicit list of changed repo-relative paths (implies --changed routing).",
    )
    parser.add_argument(
        "--filter",
        help="Optional test name filter passed after `--` for targeted inner-loop runs.",
    )
    parser.add_argument(
        "--release",
        action="store_true",
        help="Run Cargo in --release mode (default uses fast dev profile with opt-level=2 deps).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print selected groups and Cargo commands without executing them.",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv if argv is not None else sys.argv[1:])

    if args.self_test:
        return run_self_test()

    if args.check:
        if not args.dry_run and not ADMIN_DIST_INDEX.is_file():
            print(
                "ERROR: cms/admin/dist/index.html is missing (required by rust-embed in opendash-server).\n"
                "Run first: npm --prefix cms/admin ci && npm --prefix cms/admin run build",
                file=sys.stderr,
            )
            return 1
        cmd = [
            "cargo",
            "check",
            "--manifest-path",
            str(CARGO_MANIFEST),
            "--locked",
            "--workspace",
            "--bins",
            "--lib",
        ]
        if args.release:
            cmd.append("--release")
        profile_label = "release" if args.release else "fast-dev (check)"
        print(f"Profile : {profile_label}")
        print(f"> {' '.join(cmd)}")
        if args.dry_run:
            return 0
        env = test_runner_env(dict(os.environ))
        started = time.monotonic()
        proc = subprocess.run(cmd, cwd=str(ROOT), env=env, check=False)
        elapsed = time.monotonic() - started
        if proc.returncode != 0:
            print(f"FAILED (exit {proc.returncode}) after {elapsed:.1f}s", file=sys.stderr)
            return proc.returncode
        print(f"PASS ({elapsed:.1f}s)")
        return 0

    all_tests = discover_integration_tests()

    if args.list:
        return print_group_listing(all_tests)

    if args.files:
        plan = build_plan_from_files(args.files, all_tests)
    elif args.changed:
        changed = detect_git_changed_files(args.base)
        wiring_only = detect_wiring_only_files(changed, args.base)
        plan = build_plan_from_files(changed, all_tests, wiring_only_files=wiring_only)
    elif args.group:
        requested = [item.strip() for item in args.group.split(",") if item.strip()]
        valid = {"core", "all", *ALL_GROUP_NAMES}
        unknown = [item for item in requested if item not in valid]
        if unknown:
            print(
                f"ERROR: unknown group(s): {', '.join(unknown)}. Valid: {', '.join(sorted(valid))}",
                file=sys.stderr,
            )
            return 2
        plan = build_plan_from_groups(requested, all_tests)
    else:
        print("Specify one of --list, --check, --group <name>, --changed, --files <paths>, or --self-test.", file=sys.stderr)
        return 2

    commands = build_cargo_commands(plan, release=args.release, test_filter=args.filter)
    profile_label = "release" if args.release else "fast-dev (deps opt-level=2)"

    print(f"Profile : {profile_label}")
    print(f"Groups  : {', '.join(plan.groups) if plan.groups else ('core' if plan.run_core else 'none')}")
    if plan.reasons:
        print("Routing :")
        for line in plan.reasons[:25]:
            print(f"  - {line}")
        if len(plan.reasons) > 25:
            print(f"  - ... (+{len(plan.reasons) - 25} more)")
    print(
        f"Targets : core={plan.run_core}, plugin_sdk={plan.run_plugin_sdk}, "
        f"unit_filters={len(plan.unit_filters)}, integration_tests={len(plan.integration_tests)}"
    )

    if not commands:
        if plan.admin_only_note:
            print("Admin-only changes detected; skip Rust tests. Run:")
            print("  node cms/scripts/check-i18n.mjs --strict")
            print("  npm --prefix cms/admin run typecheck && npm --prefix cms/admin run build")
        else:
            print("No Rust test targets affected; skipping Cargo.")
        return 0

    needs_server = plan.run_all_workspace or bool(plan.unit_filters) or bool(plan.integration_tests)
    if needs_server and not args.dry_run and not ADMIN_DIST_INDEX.is_file():
        print(
            "ERROR: cms/admin/dist/index.html is missing (required by rust-embed in opendash-server).\n"
            "Run first: npm --prefix cms/admin ci && npm --prefix cms/admin run build",
            file=sys.stderr,
        )
        return 1

    env = test_runner_env(dict(os.environ))
    started = time.monotonic()
    for cmd in commands:
        printable = " ".join(cmd)
        print(f"\n> {printable}")
        if args.dry_run:
            continue
        step_start = time.monotonic()
        proc = subprocess.run(cmd, cwd=str(ROOT), env=env, check=False)
        elapsed = time.monotonic() - step_start
        if proc.returncode != 0:
            print(f"FAILED (exit {proc.returncode}) after {elapsed:.1f}s: {printable}", file=sys.stderr)
            return proc.returncode
        print(f"PASS ({elapsed:.1f}s)")

    if not args.dry_run:
        print(f"\nAll selected test commands PASSED in {time.monotonic() - started:.1f}s.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
