CREATE TABLE theme_blocks (
    id TEXT PRIMARY KEY NOT NULL,
    fields_json TEXT NOT NULL,
    template TEXT NOT NULL
);

CREATE TABLE home_page (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    block_id TEXT NOT NULL REFERENCES theme_blocks(id),
    values_json TEXT NOT NULL
);
