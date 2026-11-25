# Admin HTTP API Client (`cms/admin/src/api/`)

Tầng giao tiếp HTTP duy nhất của React Admin. Theo [Core Foundation](../../../../docs/contracts/CORE_FOUNDATION.md) và `cms/scripts/check-standards.py`, **100% lời gọi `fetch` trong `cms/admin/src/` phải đi qua [`client.ts`](client.ts)**.

- `client.ts`, `index.ts`: `requestJson`, `requestText`, `requestBlob`, `ApiClientError`, gắn `X-CSRF-Token`, xử lý `401`/`412` và re-export.
- **Nội dung & Taxonomy:** `content.ts`, `taxonomy.ts`, `patterns.ts`, `sections.ts` (`getLocalTemplateLibrary`), `media.ts`.
- **Giao diện & Studio/Builder:** `studio.ts`, `builder.ts`, `themes.ts`, `menus.ts`.
- **Mở rộng & Hệ thống:** `plugins.ts`, `auth.ts`, `users.ts`, `dashboard.ts`, `settings.ts`, `locales.ts`, `seo.ts`, `contact.ts`, `smtp.ts`, `operations.ts`.
