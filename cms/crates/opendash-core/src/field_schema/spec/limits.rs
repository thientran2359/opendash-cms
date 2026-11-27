use super::*;

pub const MAX_TEXT_CHARS: usize = 2_000;

pub const MAX_TEXTAREA_CHARS: usize = 100_000;

pub const MIN_TEXTAREA_ROWS: u8 = 2;

pub const MAX_TEXTAREA_ROWS: u8 = 16;

pub const MAX_EMAIL_CHARS: usize = 254;

pub const MAX_URL_BYTES: usize = 2048;

pub const MAX_LINK_LABEL_CHARS: usize = 160;

pub const MAX_OPTION_VALUE_CHARS: usize = 64;

pub const MAX_SELECT_OPTIONS: usize = 100;

pub const MAX_RADIO_OPTIONS: usize = 12;

pub const MAX_GALLERY_ITEMS: usize = 50;

pub const MAX_ENTRY_REF_TYPES: usize = 16;

pub const MAX_ENTRY_REF_ITEMS: usize = 50;

pub const MAX_TERM_REF_ITEMS: usize = 50;

pub const MAX_REPEATER_ITEMS: usize = 50;

pub const MAX_BLOCK_REPEATER_ITEMS: usize = 32;

pub const MAX_BLOCK_REPEATER_TOTAL_ITEMS: usize = 64;

pub const MAX_SCHEMA_DATA_FIELDS: usize = 64;

pub const MAX_TERM_PANEL_DATA_FIELDS: usize = 16;

pub const MAX_BLOCK_TOP_DATA_FIELDS: usize = 32;

pub const MAX_BLOCK_LEAF_DATA_FIELDS: usize = 48;

pub const MAX_BLOCK_TOTAL_SPECS: usize = 64;

pub const MAX_CONDITION_GROUPS: usize = 8;

pub const MAX_CONDITION_RULES_PER_GROUP: usize = 8;

pub const MAX_MESSAGE_BODY_CHARS: usize = 1_000;

pub const MAX_BUTTON_LABEL_CHARS: usize = 64;

pub const MAX_ON_OFF_LABEL_CHARS: usize = 64;

pub const MAX_GROUP_DEPTH: usize = 2;

pub const ALLOWED_COLOR_TOKENS: [&str; 6] = [
    "var(--od-token-primary)",
    "var(--od-token-accent)",
    "var(--od-token-surface)",
    "var(--od-token-muted)",
    "var(--od-token-text)",
    "var(--od-token-border)",
];

pub const GLOBAL_PUBLIC_URL_OPTION_KEYS: &[&str] = &[
    "social_facebook",
    "social_youtube",
    "social_tiktok",
    "social_instagram",
    "social_x",
    "social_linkedin",
    "social_email",
];

pub const GLOBAL_PUBLIC_MEDIA_OPTION_KEYS: &[&str] = &["site_logo"];
